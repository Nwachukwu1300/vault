import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { DownloadProgress, ModelStatus } from "../types";

interface ModelDownloadProps {
  status: ModelStatus;
  onReady: () => void;
}

const mb = (bytes: number) => `${Math.round(bytes / 1_000_000)} MB`;

// shortcut: minimal first-run step; Phase 9 folds this into the real onboarding.
function ModelDownload({ status, onReady }: ModelDownloadProps) {
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<DownloadProgress>("model-download-progress", (e) => setProgress(e.payload));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const start = async () => {
    setError(null);
    setDownloading(true);
    try {
      await invoke("download_models");
      onReady();
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
    }
  };

  const done = progress?.downloaded_bytes ?? status.total_bytes - status.missing_bytes;
  const pct = Math.min(100, Math.round((done / status.total_bytes) * 100));

  return (
    <div className="flex h-screen items-center justify-center bg-white dark:bg-zinc-900">
      <div className="w-full max-w-md bg-white dark:bg-zinc-800 rounded-2xl border border-zinc-200 dark:border-zinc-700 p-8 shadow-lg mx-4">
        <h2 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100 mb-2">Download search models</h2>
        <p className="text-sm text-zinc-500 dark:text-zinc-400 mb-6">
          Vault searches your files on this Mac, without sending them anywhere. It needs a one-time download of{" "}
          {mb(status.missing_bytes)}.
        </p>

        {(downloading || done > 0) && (
          <div className="mb-6">
            <div
              className="h-2 rounded-full bg-zinc-100 dark:bg-zinc-700 overflow-hidden"
              role="progressbar"
              aria-valuenow={pct}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <div className="h-full bg-accent transition-all" style={{ width: `${pct}%` }} />
            </div>
            <p className="mt-2 text-xs text-zinc-500 dark:text-zinc-400">
              {mb(done)} of {mb(status.total_bytes)}
            </p>
          </div>
        )}

        {error && (
          <p className="mb-4 text-sm text-red-600 dark:text-red-400" role="alert">
            Download stopped: {error}
          </p>
        )}

        {downloading ? (
          <button
            onClick={() => invoke("cancel_model_download")}
            className="w-full py-3 border border-zinc-200 dark:border-zinc-600 text-zinc-700 dark:text-zinc-200 font-medium rounded-xl"
          >
            Cancel
          </button>
        ) : (
          <button
            onClick={start}
            className="w-full py-3 bg-accent hover:bg-accent-dark text-white font-medium rounded-xl transition-colors"
          >
            {error || done > 0 ? "Resume download" : "Download"}
          </button>
        )}
      </div>
    </div>
  );
}

export default ModelDownload;
