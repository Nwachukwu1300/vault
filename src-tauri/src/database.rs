//! LanceDB vector database module
//!
//! This module handles all interactions with the local LanceDB vector store.
//! LanceDB stores 3072-dimension vectors alongside file metadata.

use arrow_array::{
    ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
    TimestampMillisecondArray,
};
use arrow_schema::{DataType, Field, Schema, TimeUnit};
use futures::TryStreamExt;
use lancedb::connect;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::{Connection, Table};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::embedding::VECTOR_DIMENSION;

/// Table name for storing file embeddings
const TABLE_NAME: &str = "file_embeddings";

/// Record stored in LanceDB for each indexed file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub file_path: String,
    pub file_name: String,
    pub file_type: String,
    pub last_modified: i64, // Unix timestamp in milliseconds
    pub transcript: String, // Empty for non-audio/video files
    pub vector: Vec<f32>,
}

/// Database wrapper for LanceDB operations
pub struct VectorDatabase {
    connection: Connection,
    table: RwLock<Option<Table>>,
}

impl VectorDatabase {
    /// Create or open the database at the given path
    pub async fn open(db_path: PathBuf) -> Result<Self, String> {
        // Ensure the directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create database directory: {}", e))?;
        }

        let connection = connect(db_path.to_str().unwrap())
            .execute()
            .await
            .map_err(|e| format!("Failed to connect to LanceDB: {}", e))?;

        let db = Self {
            connection,
            table: RwLock::new(None),
        };

        // Try to open existing table or create new one
        db.ensure_table().await?;

        Ok(db)
    }

    /// Get the schema for the file embeddings table
    fn get_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("file_path", DataType::Utf8, false),
            Field::new("file_name", DataType::Utf8, false),
            Field::new("file_type", DataType::Utf8, false),
            Field::new(
                "last_modified",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("transcript", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    VECTOR_DIMENSION as i32,
                ),
                false,
            ),
        ]))
    }

    /// Ensure the table exists, creating it if necessary
    async fn ensure_table(&self) -> Result<(), String> {
        let table_names = self
            .connection
            .table_names()
            .execute()
            .await
            .map_err(|e| format!("Failed to list tables: {}", e))?;

        let table = if table_names.contains(&TABLE_NAME.to_string()) {
            self.connection
                .open_table(TABLE_NAME)
                .execute()
                .await
                .map_err(|e| format!("Failed to open table: {}", e))?
        } else {
            // Create empty table with schema
            let schema = Self::get_schema();
            let batch = RecordBatch::new_empty(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

            self.connection
                .create_table(TABLE_NAME, Box::new(batches))
                .execute()
                .await
                .map_err(|e| format!("Failed to create table: {}", e))?
        };

        *self.table.write().await = Some(table);
        Ok(())
    }

    /// Insert a new file record into the database
    pub async fn insert(&self, record: FileRecord) -> Result<(), String> {
        let table = self.get_table().await?;

        let schema = Self::get_schema();
        let batch = Self::record_to_batch(&record, schema.clone())?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| format!("Failed to insert record: {}", e))?;

        Ok(())
    }

    /// Update an existing file record (by file_path)
    pub async fn update(&self, record: FileRecord) -> Result<(), String> {
        // LanceDB doesn't have direct update, so we delete and re-insert
        self.delete(&record.file_path).await?;
        self.insert(record).await
    }

    /// Delete a record by file path
    pub async fn delete(&self, file_path: &str) -> Result<(), String> {
        let table = self.get_table().await?;

        table
            .delete(&format!("file_path = '{}'", file_path.replace('\'', "''")))
            .await
            .map_err(|e| format!("Failed to delete record: {}", e))?;

        Ok(())
    }

    /// Check if a file is already indexed
    pub async fn exists(&self, file_path: &str) -> Result<bool, String> {
        let table = self.get_table().await?;

        let results = table
            .query()
            .only_if(format!("file_path = '{}'", file_path.replace('\'', "''")))
            .limit(1)
            .execute()
            .await
            .map_err(|e| format!("Failed to query: {}", e))?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| format!("Failed to collect results: {}", e))?;

        Ok(batches.iter().any(|b| b.num_rows() > 0))
    }

    /// Get the last modified timestamp for a file
    pub async fn get_last_modified(&self, file_path: &str) -> Result<Option<i64>, String> {
        let table = self.get_table().await?;

        let results = table
            .query()
            .only_if(format!("file_path = '{}'", file_path.replace('\'', "''")))
            .select(Select::Columns(vec!["last_modified".to_string()]))
            .limit(1)
            .execute()
            .await
            .map_err(|e| format!("Failed to query: {}", e))?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| format!("Failed to collect results: {}", e))?;

        for batch in batches {
            if batch.num_rows() > 0 {
                let col = batch
                    .column_by_name("last_modified")
                    .ok_or("Column not found")?;
                let arr = col
                    .as_any()
                    .downcast_ref::<TimestampMillisecondArray>()
                    .ok_or("Invalid column type")?;
                return Ok(Some(arr.value(0)));
            }
        }

        Ok(None)
    }

    /// Get total count of indexed files
    pub async fn count(&self) -> Result<usize, String> {
        let table = self.get_table().await?;

        let results = table
            .query()
            .execute()
            .await
            .map_err(|e| format!("Failed to count: {}", e))?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| format!("Failed to collect: {}", e))?;

        Ok(batches.iter().map(|b| b.num_rows()).sum())
    }

    /// Get the table, returning an error if not initialized
    async fn get_table(&self) -> Result<Table, String> {
        self.table
            .read()
            .await
            .clone()
            .ok_or_else(|| "Database table not initialized".to_string())
    }

    /// Convert a FileRecord to a RecordBatch
    fn record_to_batch(record: &FileRecord, schema: Arc<Schema>) -> Result<RecordBatch, String> {
        let file_paths: ArrayRef = Arc::new(StringArray::from(vec![record.file_path.clone()]));
        let file_names: ArrayRef = Arc::new(StringArray::from(vec![record.file_name.clone()]));
        let file_types: ArrayRef = Arc::new(StringArray::from(vec![record.file_type.clone()]));
        let last_modified: ArrayRef =
            Arc::new(TimestampMillisecondArray::from(vec![record.last_modified]));
        let transcripts: ArrayRef = Arc::new(StringArray::from(vec![record.transcript.clone()]));

        // Create the vector as a FixedSizeListArray
        let vector_values = Float32Array::from(record.vector.clone());
        let vector_field = Arc::new(Field::new("item", DataType::Float32, true));
        let vectors: ArrayRef = Arc::new(
            FixedSizeListArray::try_new(
                vector_field,
                VECTOR_DIMENSION as i32,
                Arc::new(vector_values),
                None,
            )
            .map_err(|e| format!("Failed to create vector array: {}", e))?,
        );

        RecordBatch::try_new(
            schema,
            vec![
                file_paths,
                file_names,
                file_types,
                last_modified,
                transcripts,
                vectors,
            ],
        )
        .map_err(|e| format!("Failed to create record batch: {}", e))
    }
}

/// Get the default database path in the app's data directory
pub fn get_database_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.vault.app")
        .join("lancedb")
}
