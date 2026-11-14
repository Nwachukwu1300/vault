import { IndexingProgress } from "../types";

interface IndexingProgressBarProps {
  progress: IndexingProgress;
  onPause: () => void;
  onResume: () => void;
}

const CheckIcon = () => (
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
    <polyline points="20 6 9 17 4 12" />
  </svg>
);

const SpinnerIcon = () => (
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
);

function IndexingProgressBar({
  progress,
  onPause,
  onResume,
}: IndexingProgressBarProps) {
  const totalFiles = progress.documents_total + progress.images_total + progress.audio_total + progress.videos_total;
  const doneFiles = progress.documents_done + progress.images_done + progress.audio_done + progress.videos_done;
  const percentComplete = totalFiles > 0 ? (doneFiles / totalFiles) * 100 : 0;

  const getCategoryLabel = () => {
    if (!progress.current_category) return "Preparing...";

    switch (progress.current_category) {
      case "documents":
        return `Documents (${progress.documents_total - progress.documents_done} remaining)`;
      case "images":
        return `Images (${progress.images_total - progress.images_done} remaining)`;
      case "audio":
        return `Audio (${progress.audio_total - progress.audio_done} remaining)`;
      case "videos":
        return `Videos (${progress.videos_total - progress.videos_done} remaining)`;
      default:
        return "Processing...";
    }
  };

  const isCategoryComplete = (category: string) => {
    switch (category) {
      case "documents":
        return progress.documents_done >= progress.documents_total && progress.documents_total > 0;
      case "images":
        return progress.images_done >= progress.images_total && progress.images_total > 0;
      case "audio":
        return progress.audio_done >= progress.audio_total && progress.audio_total > 0;
      case "videos":
        return progress.videos_done >= progress.videos_total && progress.videos_total > 0;
      default:
        return false;
    }
  };

  const isCategoryActive = (category: string) => {
    return progress.current_category === category;
  };

  return (
    <div className="fixed bottom-0 left-0 right-0 bg-white dark:bg-zinc-800 border-t border-zinc-200 dark:border-zinc-700 px-6 py-3 z-40">
      <div className="flex items-center justify-between max-w-screen-xl mx-auto">
        {/* Progress info */}
        <div className="flex items-center gap-4">
          <span className="text-sm text-zinc-700 dark:text-zinc-300">
            {progress.is_paused ? "Paused" : `Indexing in progress — ${getCategoryLabel()}`}
          </span>

          {/* Category indicators */}
          <div className="flex items-center gap-3 text-xs">
            <CategoryIndicator
              label="Documents"
              count={progress.documents_total}
              isComplete={isCategoryComplete("documents")}
              isActive={isCategoryActive("documents")}
            />
            <CategoryIndicator
              label="Images"
              count={progress.images_total}
              isComplete={isCategoryComplete("images")}
              isActive={isCategoryActive("images")}
            />
            <CategoryIndicator
              label="Audio"
              count={progress.audio_total}
              isComplete={isCategoryComplete("audio")}
              isActive={isCategoryActive("audio")}
            />
            <CategoryIndicator
              label="Videos"
              count={progress.videos_total}
              isComplete={isCategoryComplete("videos")}
              isActive={isCategoryActive("videos")}
            />
          </div>
        </div>

        {/* Pause/Resume button */}
        <button
          onClick={progress.is_paused ? onResume : onPause}
          className="text-sm text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200 transition-colors"
        >
          {progress.is_paused ? "Resume" : "Pause"}
        </button>
      </div>

      {/* Progress bar */}
      <div className="mt-2 h-1 bg-zinc-200 dark:bg-zinc-700 rounded-full overflow-hidden max-w-screen-xl mx-auto">
        <div
          className="h-full bg-accent transition-all duration-300 ease-out"
          style={{ width: `${percentComplete}%` }}
        />
      </div>
    </div>
  );
}

interface CategoryIndicatorProps {
  label: string;
  count: number;
  isComplete: boolean;
  isActive: boolean;
}

function CategoryIndicator({ label, count, isComplete, isActive }: CategoryIndicatorProps) {
  if (count === 0) return null;

  return (
    <div className="flex items-center gap-1">
      {isComplete ? (
        <CheckIcon />
      ) : isActive ? (
        <SpinnerIcon />
      ) : (
        <span className="w-3.5 h-3.5" />
      )}
      <span
        className={`
          ${isComplete ? "text-accent" : "text-zinc-400 dark:text-zinc-500"}
          ${isActive ? "font-medium text-zinc-700 dark:text-zinc-300" : ""}
        `}
      >
        {label}
      </span>
    </div>
  );
}

export default IndexingProgressBar;
