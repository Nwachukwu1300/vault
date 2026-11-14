interface ScanResult {
  documents: number;
  images: number;
  audio: number;
  videos: number;
  total: number;
  estimated_time: string;
  estimated_cost: string;
}

interface EstimateScreenProps {
  scanResult: ScanResult;
  onStartIndexing: () => void;
}

const DocumentIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-500 dark:text-zinc-400"
  >
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="16" y1="13" x2="8" y2="13" />
    <line x1="16" y1="17" x2="8" y2="17" />
    <polyline points="10 9 9 9 8 9" />
  </svg>
);

const ImageIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-500 dark:text-zinc-400"
  >
    <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
    <circle cx="8.5" cy="8.5" r="1.5" />
    <polyline points="21 15 16 10 5 21" />
  </svg>
);

const AudioIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-500 dark:text-zinc-400"
  >
    <path d="M9 18V5l12-2v13" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="18" cy="16" r="3" />
  </svg>
);

const VideoIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-500 dark:text-zinc-400"
  >
    <rect x="2" y="2" width="20" height="20" rx="2.18" ry="2.18" />
    <line x1="7" y1="2" x2="7" y2="22" />
    <line x1="17" y1="2" x2="17" y2="22" />
    <line x1="2" y1="12" x2="22" y2="12" />
    <line x1="2" y1="7" x2="7" y2="7" />
    <line x1="2" y1="17" x2="7" y2="17" />
    <line x1="17" y1="17" x2="22" y2="17" />
    <line x1="17" y1="7" x2="22" y2="7" />
  </svg>
);

function EstimateScreen({ scanResult, onStartIndexing }: EstimateScreenProps) {
  const formatNumber = (num: number) => {
    return num.toLocaleString();
  };

  return (
    <div className="fixed inset-0 bg-black/20 dark:bg-black/40 flex items-center justify-center z-50">
      <div className="w-full max-w-lg bg-white dark:bg-zinc-800 rounded-2xl border border-zinc-200 dark:border-zinc-700 p-8 shadow-lg mx-4">
        {/* Heading */}
        <h2 className="text-2xl font-semibold text-zinc-900 dark:text-zinc-100 text-center mb-8">
          Ready to index your files
        </h2>

        {/* File category grid */}
        <div className="grid grid-cols-2 gap-3 mb-4">
          {/* Documents */}
          <div className="flex items-center gap-3 p-4 bg-zinc-50 dark:bg-zinc-700/50 rounded-xl border border-zinc-100 dark:border-zinc-600">
            <DocumentIcon />
            <div>
              <p className="text-xs text-zinc-500 dark:text-zinc-400">Documents</p>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {formatNumber(scanResult.documents)}
              </p>
            </div>
          </div>

          {/* Images */}
          <div className="flex items-center gap-3 p-4 bg-zinc-50 dark:bg-zinc-700/50 rounded-xl border border-zinc-100 dark:border-zinc-600">
            <ImageIcon />
            <div>
              <p className="text-xs text-zinc-500 dark:text-zinc-400">Images</p>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {formatNumber(scanResult.images)}
              </p>
            </div>
          </div>

          {/* Audio */}
          <div className="flex items-center gap-3 p-4 bg-zinc-50 dark:bg-zinc-700/50 rounded-xl border border-zinc-100 dark:border-zinc-600">
            <AudioIcon />
            <div>
              <p className="text-xs text-zinc-500 dark:text-zinc-400">Audio</p>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {formatNumber(scanResult.audio)}
              </p>
            </div>
          </div>

          {/* Videos */}
          <div className="flex items-center gap-3 p-4 bg-zinc-50 dark:bg-zinc-700/50 rounded-xl border border-zinc-100 dark:border-zinc-600">
            <VideoIcon />
            <div>
              <p className="text-xs text-zinc-500 dark:text-zinc-400">Videos</p>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {formatNumber(scanResult.videos)}
              </p>
            </div>
          </div>
        </div>

        {/* Total files bar */}
        <div className="flex items-center justify-between p-4 bg-zinc-900 dark:bg-zinc-900 rounded-xl mb-6">
          <span className="text-sm font-medium text-zinc-300">Total files</span>
          <span className="text-2xl font-bold text-white">
            {formatNumber(scanResult.total)}
          </span>
        </div>

        {/* Estimates */}
        <div className="space-y-3 mb-6">
          <div className="flex items-center justify-between">
            <span className="text-sm text-zinc-600 dark:text-zinc-400">Estimated time</span>
            <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {scanResult.estimated_time}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-sm text-zinc-600 dark:text-zinc-400">Estimated API cost</span>
            <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
              {scanResult.estimated_cost}
            </span>
          </div>
        </div>

        {/* Info text */}
        <p className="text-xs text-zinc-400 dark:text-zinc-500 text-center mb-6">
          Indexing runs in the background. You can close the app.
        </p>

        {/* Start Indexing button */}
        <button
          onClick={onStartIndexing}
          className="w-full py-4 bg-accent hover:bg-accent-dark text-white font-medium rounded-xl transition-colors"
        >
          Start Indexing
        </button>
      </div>
    </div>
  );
}

export default EstimateScreen;
