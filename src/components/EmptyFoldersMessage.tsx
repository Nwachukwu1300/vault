interface EmptyFoldersMessageProps {
  onDismiss: () => void;
}

const FolderIcon = () => (
  <svg
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-300 dark:text-zinc-600"
  >
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
  </svg>
);

function EmptyFoldersMessage({ onDismiss }: EmptyFoldersMessageProps) {
  return (
    <div className="fixed inset-0 bg-black/20 dark:bg-black/40 flex items-center justify-center z-50">
      <div className="w-full max-w-md bg-white dark:bg-zinc-800 rounded-2xl border border-zinc-200 dark:border-zinc-700 p-8 shadow-lg mx-4 text-center">
        <div className="flex justify-center mb-4">
          <FolderIcon />
        </div>

        <h2 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100 mb-2">
          No supported files found
        </h2>

        <p className="text-sm text-zinc-500 dark:text-zinc-400 mb-6">
          No supported files found in your selected folders. Vault indexes images (jpg, png, webp, heic), videos (mp4, mov), audio (mp3, wav, m4a, aac), and documents (pdf, txt, md, docx).
        </p>

        <button
          onClick={onDismiss}
          className="w-full py-3 bg-accent hover:bg-accent-dark text-white font-medium rounded-xl transition-colors"
        >
          Continue Anyway
        </button>
      </div>
    </div>
  );
}

export default EmptyFoldersMessage;
