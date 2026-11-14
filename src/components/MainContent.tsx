import { FileCategory } from "../App";
import SearchBar from "./SearchBar";
import StatusIndicator from "./StatusIndicator";
import SettingsMenu from "./SettingsMenu";

interface MainContentProps {
  activeCategory: FileCategory;
  isIndexing?: boolean;
  indexingComplete?: boolean;
  onSignOut: () => void;
  onReindex: () => void;
}

function MainContent({
  activeCategory: _activeCategory,
  isIndexing = false,
  indexingComplete = false,
  onSignOut,
  onReindex,
}: MainContentProps) {
  return (
    <main className="flex-1 flex flex-col bg-white dark:bg-zinc-900">
      <header className="flex items-start justify-between px-8 pt-8">
        <div className="flex-1">
          <SearchBar />
        </div>
        <div className="ml-6 pt-2 flex items-center gap-4">
          <StatusIndicator isIndexing={isIndexing} indexingComplete={indexingComplete} />
          <SettingsMenu onSignOut={onSignOut} onReindex={onReindex} />
        </div>
      </header>

      <div className="flex-1 flex items-center justify-center">
        <p className="text-zinc-400 dark:text-zinc-500 text-sm">
          Start typing to search your files.
        </p>
      </div>
    </main>
  );
}

export default MainContent;
