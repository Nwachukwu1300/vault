import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import MainContent from "./components/MainContent";
import Onboarding from "./components/Onboarding";
import EstimateScreen from "./components/EstimateScreen";
import EmptyFoldersMessage from "./components/EmptyFoldersMessage";
import IndexingProgressBar from "./components/IndexingProgressBar";
import ModelDownload from "./components/ModelDownload";
import { AppConfig, ScanResult, IndexingProgress, ModelStatus } from "./types";

export type FileCategory = "all" | "images" | "videos" | "audio" | "documents";

function App() {
  const [activeCategory, setActiveCategory] = useState<FileCategory>("all");
  const [isLoading, setIsLoading] = useState(true);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [showEmptyMessage, setShowEmptyMessage] = useState(false);
  const [folders, setFolders] = useState<string[]>([]);
  const [indexingProgress, setIndexingProgress] = useState<IndexingProgress | null>(null);
  const [indexingComplete, setIndexingComplete] = useState(false);
  const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);

  // Listen for indexing progress events from backend
  useEffect(() => {
    const unlistenProgress = listen<IndexingProgress>("indexing-progress", (event) => {
      setIndexingProgress(event.payload);

      // Check if indexing just completed
      if (!event.payload.is_indexing && event.payload.documents_done + event.payload.images_done + event.payload.audio_done + event.payload.videos_done > 0) {
        setIndexingComplete(true);
        setIndexingProgress(null);
      }
    });

    const unlistenComplete = listen("indexing-complete", () => {
      setIndexingComplete(true);
      setIndexingProgress(null);
    });

    return () => {
      unlistenProgress.then((unlisten) => unlisten());
      unlistenComplete.then((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    checkConfig();
  }, []);

  const checkConfig = async () => {
    try {
      // Initialize database on app start
      await invoke("init_database");
      setModelStatus(await invoke<ModelStatus>("get_model_status"));

      const config = await invoke<AppConfig | null>("load_config");

      if (config && config.onboarding_complete) {
        setShowOnboarding(false);
        setFolders(config.folders);

        // Check if there are already indexed files
        const indexedCount = await invoke<number>("get_indexed_count");
        if (indexedCount > 0) {
          // Already indexed, mark as complete
          setIndexingComplete(true);
        } else {
          // Trigger scan automatically for returning users who haven't indexed yet
          triggerScan(config.folders);
        }
      } else {
        setShowOnboarding(true);
      }
    } catch (error) {
      console.error("Failed to load config:", error);
      setShowOnboarding(true);
    } finally {
      setIsLoading(false);
    }
  };

  const triggerScan = async (foldersToScan: string[]) => {
    setIsScanning(true);
    try {
      const result = await invoke<ScanResult>("scan_folders", {
        folders: foldersToScan,
      });

      if (result.total === 0) {
        setShowEmptyMessage(true);
      } else {
        setScanResult(result);
      }
    } catch (error) {
      console.error("Failed to scan folders:", error);
    } finally {
      setIsScanning(false);
    }
  };

  const handleOnboardingComplete = async () => {
    setShowOnboarding(false);
    // Load the config to get the folders and trigger scan
    try {
      const config = await invoke<AppConfig | null>("load_config");
      if (config) {
        setFolders(config.folders);
        triggerScan(config.folders);
      }
    } catch (error) {
      console.error("Failed to load config after onboarding:", error);
    }
  };

  const handleStartIndexing = useCallback(async () => {
    // Dismiss the estimate screen
    setScanResult(null);

    // Start the actual indexing process
    try {
      await invoke("start_indexing", { folders });
    } catch (error) {
      console.error("Failed to start indexing:", error);
    }
  }, [folders]);

  const handlePauseIndexing = useCallback(async () => {
    try {
      await invoke("pause_indexing");
    } catch (error) {
      console.error("Failed to pause indexing:", error);
    }
  }, []);

  const handleResumeIndexing = useCallback(async () => {
    try {
      await invoke("resume_indexing");
    } catch (error) {
      console.error("Failed to resume indexing:", error);
    }
  }, []);

  const handleDismissEmptyMessage = () => {
    setShowEmptyMessage(false);
  };

  const handleSignOut = () => {
    // Reset all state and show onboarding
    setShowOnboarding(true);
    setFolders([]);
    setScanResult(null);
    setIndexingProgress(null);
    setIndexingComplete(false);
  };

  const handleReindex = () => {
    // Clear indexing state and trigger a new scan
    setIndexingComplete(false);
    setIndexingProgress(null);
    if (folders.length > 0) {
      triggerScan(folders);
    }
  };

  if (isLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-white dark:bg-zinc-900">
        <div className="animate-pulse text-zinc-400">Loading...</div>
      </div>
    );
  }

  if (modelStatus && !modelStatus.ready) {
    return <ModelDownload status={modelStatus} onReady={() => setModelStatus({ ...modelStatus, ready: true })} />;
  }

  if (showOnboarding) {
    return <Onboarding onComplete={handleOnboardingComplete} />;
  }

  return (
    <div className={`flex h-screen bg-white dark:bg-zinc-900 ${indexingProgress?.is_indexing ? "pb-16" : ""}`}>
      <Sidebar
        activeCategory={activeCategory}
        onCategoryChange={setActiveCategory}
      />
      <MainContent
        activeCategory={activeCategory}
        isIndexing={indexingProgress?.is_indexing || false}
        indexingComplete={indexingComplete}
        onSignOut={handleSignOut}
        onReindex={handleReindex}
      />

      {/* Scanning indicator */}
      {isScanning && (
        <div className="fixed inset-0 bg-black/20 dark:bg-black/40 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-zinc-800 rounded-2xl border border-zinc-200 dark:border-zinc-700 p-8 shadow-lg">
            <div className="flex items-center gap-3">
              <div className="animate-spin h-5 w-5 border-2 border-accent border-t-transparent rounded-full" />
              <span className="text-zinc-700 dark:text-zinc-300">Scanning folders...</span>
            </div>
          </div>
        </div>
      )}

      {/* Estimate screen */}
      {scanResult && (
        <EstimateScreen
          scanResult={scanResult}
          onStartIndexing={handleStartIndexing}
        />
      )}

      {/* Empty folders message */}
      {showEmptyMessage && (
        <EmptyFoldersMessage onDismiss={handleDismissEmptyMessage} />
      )}

      {/* Indexing progress bar */}
      {indexingProgress && indexingProgress.is_indexing && (
        <IndexingProgressBar
          progress={indexingProgress}
          onPause={handlePauseIndexing}
          onResume={handleResumeIndexing}
        />
      )}
    </div>
  );
}

export default App;
