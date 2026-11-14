interface StatusIndicatorProps {
  isIndexing?: boolean;
  indexingComplete?: boolean;
}

function StatusIndicator({
  isIndexing = false,
  indexingComplete = false,
}: StatusIndicatorProps) {
  if (isIndexing) {
    return (
      <div className="flex items-center gap-1.5 text-xs text-zinc-400 dark:text-zinc-500">
        <svg
          className="animate-spin h-3.5 w-3.5 text-accent"
          xmlns="http://www.w3.org/2000/svg"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
          />
        </svg>
        <span>Indexing in progress...</span>
      </div>
    );
  }

  if (indexingComplete) {
    return (
      <div className="flex items-center gap-1.5 text-xs text-zinc-400 dark:text-zinc-500">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className="text-accent"
        >
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
          <polyline points="22 4 12 14.01 9 11.01" />
        </svg>
        <span>Index up to date</span>
      </div>
    );
  }

  // Default state - no index yet
  return (
    <div className="flex items-center gap-1.5 text-xs text-zinc-400 dark:text-zinc-500">
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        className="text-zinc-400 dark:text-zinc-500"
      >
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="8" x2="12" y2="12" />
        <line x1="12" y1="16" x2="12.01" y2="16" />
      </svg>
      <span>Not indexed yet</span>
    </div>
  );
}

export default StatusIndicator;
