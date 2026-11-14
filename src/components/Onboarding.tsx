import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface OnboardingProps {
  onComplete: () => void;
}

const KeyIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-400"
  >
    <path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" />
  </svg>
);

const FolderPlusIcon = () => (
  <svg
    width="18"
    height="18"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
    <line x1="12" y1="11" x2="12" y2="17" />
    <line x1="9" y1="14" x2="15" y2="14" />
  </svg>
);

const XIcon = () => (
  <svg
    width="14"
    height="14"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
  >
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

const CheckIcon = () => (
  <svg
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-green-600"
  >
    <polyline points="20 6 9 17 4 12" />
  </svg>
);

const LoadingSpinner = () => (
  <svg
    className="animate-spin h-4 w-4 text-white"
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

function Onboarding({ onComplete }: OnboardingProps) {
  const [apiKey, setApiKey] = useState("");
  const [isValidating, setIsValidating] = useState(false);
  const [isKeyValid, setIsKeyValid] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [folders, setFolders] = useState<string[]>([]);
  const [isAddingFolder, setIsAddingFolder] = useState(false);

  const handleValidateKey = async () => {
    if (!apiKey.trim()) {
      setKeyError("Please enter an API key");
      return;
    }

    setIsValidating(true);
    setKeyError(null);

    try {
      const isValid = await invoke<boolean>("validate_api_key", {
        apiKey: apiKey.trim(),
      });

      if (isValid) {
        setIsKeyValid(true);
        setKeyError(null);
      }
    } catch (error) {
      setIsKeyValid(false);
      setKeyError(error as string);
    } finally {
      setIsValidating(false);
    }
  };

  const handleAddFolder = async () => {
    setIsAddingFolder(true);
    try {
      const folder = await invoke<string | null>("pick_folder");
      if (folder && !folders.includes(folder)) {
        setFolders([...folders, folder]);
      }
    } catch (error) {
      console.error("Failed to pick folder:", error);
    } finally {
      setIsAddingFolder(false);
    }
  };

  const handleRemoveFolder = (folderToRemove: string) => {
    setFolders(folders.filter((f) => f !== folderToRemove));
  };

  const handleGetStarted = async () => {
    try {
      await invoke("save_optional_api_key", { apiKey: apiKey.trim() });
      await invoke("save_config", { folders });
      onComplete();
    } catch (error) {
      console.error("Failed to save config:", error);
    }
  };

  const canGetStarted = isKeyValid && folders.length > 0;

  return (
    <div className="min-h-screen bg-zinc-50 dark:bg-zinc-900 flex items-center justify-center p-8">
      <div className="w-full max-w-lg bg-white dark:bg-zinc-800 rounded-2xl border border-zinc-200 dark:border-zinc-700 p-8 shadow-sm">
        {/* Wordmark */}
        <div className="text-center mb-8">
          <h1 className="text-3xl font-semibold text-zinc-900 dark:text-zinc-100 tracking-tight">
            Vault
          </h1>
          <div className="w-10 h-1 bg-accent mx-auto mt-2 rounded-full" />
        </div>

        {/* Step 1: API Key */}
        <div className="mb-8">
          <div className="flex items-center gap-2 mb-3">
            <span className="w-6 h-6 rounded-full bg-zinc-100 dark:bg-zinc-700 flex items-center justify-center text-xs font-medium text-zinc-500 dark:text-zinc-400">
              1
            </span>
            <span className="font-medium text-zinc-900 dark:text-zinc-100">
              Gemini API Key
            </span>
          </div>

          <div className="flex gap-3">
            <div className="flex-1 relative">
              <div className="absolute inset-y-0 left-3 flex items-center pointer-events-none">
                <KeyIcon />
              </div>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => {
                  setApiKey(e.target.value);
                  if (isKeyValid) {
                    setIsKeyValid(false);
                  }
                  setKeyError(null);
                }}
                placeholder="Enter your API key"
                disabled={isKeyValid}
                className={`
                  w-full py-3 pl-10 pr-10 text-sm
                  text-zinc-900 dark:text-zinc-100
                  placeholder:text-zinc-400 dark:placeholder:text-zinc-500
                  bg-white dark:bg-zinc-700
                  border rounded-lg
                  outline-none transition-all
                  ${
                    isKeyValid
                      ? "border-green-500 bg-green-50 dark:bg-green-900/20"
                      : keyError
                      ? "border-red-500"
                      : "border-zinc-200 dark:border-zinc-600 focus:ring-2 focus:ring-accent focus:border-transparent"
                  }
                  disabled:opacity-60 disabled:cursor-not-allowed
                `}
              />
              {isKeyValid && (
                <div className="absolute inset-y-0 right-3 flex items-center">
                  <CheckIcon />
                </div>
              )}
            </div>
            <button
              onClick={handleValidateKey}
              disabled={isValidating || isKeyValid || !apiKey.trim()}
              className={`
                px-5 py-3 rounded-lg font-medium text-sm
                transition-all
                ${
                  isKeyValid
                    ? "bg-green-500 text-white cursor-not-allowed"
                    : "bg-accent hover:bg-accent-dark text-white disabled:opacity-50 disabled:cursor-not-allowed"
                }
              `}
            >
              {isValidating ? (
                <LoadingSpinner />
              ) : isKeyValid ? (
                "Valid"
              ) : (
                "Validate"
              )}
            </button>
          </div>

          {keyError && (
            <p className="mt-2 text-sm text-red-600 dark:text-red-400">
              {keyError}
            </p>
          )}
        </div>

        {/* Step 2: Folder Selection (only visible after API key is valid) */}
        {isKeyValid && (
          <div className="mb-8">
            <div className="flex items-center gap-2 mb-3">
              <span className="w-6 h-6 rounded-full bg-zinc-100 dark:bg-zinc-700 flex items-center justify-center text-xs font-medium text-zinc-500 dark:text-zinc-400">
                2
              </span>
              <span className="font-medium text-zinc-900 dark:text-zinc-100">
                Choose folders to index
              </span>
            </div>

            {/* Selected folders */}
            <div className="mb-3 min-h-[52px] p-3 border border-dashed border-zinc-300 dark:border-zinc-600 rounded-lg">
              {folders.length === 0 ? (
                <p className="text-sm text-zinc-400 dark:text-zinc-500 text-center">
                  No folders selected
                </p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {folders.map((folder) => (
                    <div
                      key={folder}
                      className="flex items-center gap-2 px-3 py-1.5 bg-zinc-100 dark:bg-zinc-700 rounded-md text-sm text-zinc-700 dark:text-zinc-300"
                    >
                      <span className="truncate max-w-[200px]" title={folder}>
                        {folder.split("/").pop() || folder}
                      </span>
                      <button
                        onClick={() => handleRemoveFolder(folder)}
                        className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 transition-colors"
                      >
                        <XIcon />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <button
              onClick={handleAddFolder}
              disabled={isAddingFolder}
              className="w-full py-3 px-4 border border-zinc-200 dark:border-zinc-600 rounded-lg text-sm font-medium text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-700 transition-colors flex items-center justify-center gap-2 disabled:opacity-50"
            >
              <FolderPlusIcon />
              Add Folder
            </button>
          </div>
        )}

        {/* Get Started Button */}
        <button
          onClick={handleGetStarted}
          disabled={!canGetStarted}
          className="w-full py-4 bg-accent hover:bg-accent-dark text-white font-medium rounded-xl transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Get Started
        </button>
      </div>
    </div>
  );
}

export default Onboarding;
