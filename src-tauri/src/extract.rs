//! Turns a file into position-tagged text: pages for PDFs, timestamps for
//! audio and video, a single untagged segment for other documents.

use crate::indexer::Priority;
use crate::transcription::transcribe;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::{Path, PathBuf};

/// shortcut: pdf-extract reads the whole file into memory, so huge PDFs are
/// skipped; switch to pdfium-render (streams pages) if they matter.
const MAX_PDF_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Segment {
    pub text: String,
    pub page: Option<u32>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

#[derive(Debug, Default)]
pub struct Extracted {
    pub segments: Vec<Segment>,
    /// Some PDF page had no text layer (a scan); OCR comes in a later phase.
    pub needs_ocr: bool,
}

pub async fn extract(path: &Path, kind: Priority) -> Result<Extracted, String> {
    match kind {
        Priority::Image => Ok(Extracted::default()),
        Priority::Audio | Priority::Video => {
            let segments = transcribe(path)
                .await?
                .into_iter()
                .map(|s| Segment {
                    text: s.text,
                    page: None,
                    start_ms: Some(s.start_ms),
                    end_ms: Some(s.end_ms),
                })
                .collect();
            Ok(Extracted { segments, needs_ocr: false })
        }
        Priority::Document => {
            let p: PathBuf = path.to_path_buf();
            tokio::task::spawn_blocking(move || extract_document(&p))
                .await
                // pdf-extract panics on some malformed PDFs; the blocking
                // task contains the panic, so the file fails, not the app.
                .map_err(|_| "Unreadable document (parser crashed)".to_string())?
        }
    }
}

fn extract_document(path: &Path) -> Result<Extracted, String> {
    if extension(path) == "pdf" {
        return pdf_pages(path);
    }
    let text = document_text(path)?;
    let segments = if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![Segment { text, ..Default::default() }]
    };
    Ok(Extracted { segments, needs_ocr: false })
}

fn pdf_pages(path: &Path) -> Result<Extracted, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("Failed to read file: {e}"))?.len();
    if size > MAX_PDF_BYTES {
        return Err(format!("PDF too large to index ({} MB)", size / 1_000_000));
    }
    let pages = pdf_extract::extract_text_by_pages(path).map_err(|e| format!("Unreadable PDF: {e}"))?;
    let mut out = Extracted::default();
    for (i, text) in pages.into_iter().enumerate() {
        if text.trim().is_empty() {
            out.needs_ocr = true;
        } else {
            out.segments.push(Segment { text, page: Some(i as u32 + 1), ..Default::default() });
        }
    }
    Ok(out)
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default()
}

/// Whole-document text for txt, md and docx.
fn document_text(path: &Path) -> Result<String, String> {
    match extension(path).as_str() {
        "txt" | "md" => {
            let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
        "docx" => docx_text(path),
        other => Err(format!("Unsupported document type: {other}")),
    }
}

/// DOCX is a zip; the body text lives in `<w:t>` runs of `word/document.xml`.
fn docx_text(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open DOCX: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Not a valid DOCX: {e}"))?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|e| format!("DOCX has no document.xml: {e}"))?
        .read_to_string(&mut xml)
        .map_err(|e| format!("Failed to read document.xml: {e}"))?;
    docx_xml_text(&xml)
}

fn docx_xml_text(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut text = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(|e| format!("Bad DOCX XML: {e}"))? {
            Event::Start(e) if e.name().as_ref() == "w:t" => in_text = true,
            Event::End(e) => match e.name().as_ref() {
                "w:t" => in_text = false,
                "w:p" => text.push('\n'),
                _ => {}
            },
            Event::Empty(e) => match e.name().as_ref() {
                "w:tab" => text.push('\t'),
                "w:br" | "w:cr" => text.push('\n'),
                _ => {}
            },
            Event::Text(t) if in_text => text.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if in_text => {
                if let Ok(Some(c)) = r.resolve_char_ref() {
                    text.push(c);
                } else if let Some(s) = resolve_predefined_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Hand-written minimal PDF: one Helvetica text line per page, with a
    /// correct xref table. An empty string makes a page with no text layer.
    pub fn write_test_pdf(path: &Path, pages: &[&str]) {
        let n = pages.len();
        // Objects: 1 catalog, 2 pages, 3 font, then (page, content) pairs.
        let mut objs: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            format!(
                "<< /Type /Pages /Kids [{}] /Count {n} >>",
                (0..n).map(|i| format!("{} 0 R", 4 + i * 2)).collect::<Vec<_>>().join(" ")
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        ];
        for (i, text) in pages.iter().enumerate() {
            objs.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
                5 + i * 2
            ));
            let stream = if text.is_empty() {
                String::new()
            } else {
                format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET")
            };
            objs.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()));
        }
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
        }
        let xref = pdf.len();
        pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1));
        for off in offsets {
            pdf.push_str(&format!("{off:010} 00000 n \n"));
        }
        pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1));
        std::fs::write(path, pdf).unwrap();
    }

    fn words(e: &Extracted) -> Vec<(Option<u32>, String)> {
        e.segments
            .iter()
            .map(|s| (s.page, s.text.split_whitespace().collect::<Vec<_>>().join(" ")))
            .collect()
    }

    #[tokio::test]
    async fn pdf_pages_keep_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("report.pdf");
        write_test_pdf(&p, &["Quarterly revenue grew", "Hiring plan for Lagos office"]);
        let e = extract(&p, Priority::Document).await.unwrap();
        assert!(!e.needs_ocr);
        assert_eq!(
            words(&e),
            vec![
                (Some(1), "Quarterly revenue grew".into()),
                (Some(2), "Hiring plan for Lagos office".into())
            ]
        );
    }

    #[tokio::test]
    async fn pdf_page_without_text_needs_ocr() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("scan.pdf");
        write_test_pdf(&p, &["first", "", "third"]);
        let e = extract(&p, Priority::Document).await.unwrap();
        assert!(e.needs_ocr);
        assert_eq!(e.segments.iter().map(|s| s.page).collect::<Vec<_>>(), vec![Some(1), Some(3)]);
    }

    #[tokio::test]
    async fn garbage_pdf_is_an_error_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("broken.pdf");
        std::fs::write(&p, b"%PDF-1.4\nthis is not really a pdf \x00\xff").unwrap();
        assert!(extract(&p, Priority::Document).await.is_err());
    }

    #[tokio::test]
    async fn txt_with_bad_utf8_is_lossy() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::write(&p, b"caf\xe9 menu").unwrap();
        let e = extract(&p, Priority::Document).await.unwrap();
        assert_eq!(e.segments[0].text, "caf\u{FFFD} menu");
        assert_eq!(e.segments[0].page, None);
    }

    #[test]
    fn docx_xml_paragraphs_entities_and_tabs() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>Q3 budget</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve"> &amp; plan</w:t></w:r></w:p>
            <w:p><w:r><w:t>caf&#233;</w:t></w:r></w:p>
            <w:p><w:r><w:instrText>IGNORED</w:instrText></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(docx_xml_text(xml).unwrap(), "Q3 budget\t & plan\ncafé");
    }
}
