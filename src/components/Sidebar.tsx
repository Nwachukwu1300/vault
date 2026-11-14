import { FileCategory } from "../App";
import SidebarItem from "./SidebarItem";

interface SidebarProps {
  activeCategory: FileCategory;
  onCategoryChange: (category: FileCategory) => void;
}

const AllFilesIcon = () => (
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
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
    <line x1="16" y1="13" x2="8" y2="13" />
    <line x1="16" y1="17" x2="8" y2="17" />
    <polyline points="10 9 9 9 8 9" />
  </svg>
);

const ImagesIcon = () => (
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
    <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
    <circle cx="8.5" cy="8.5" r="1.5" />
    <polyline points="21 15 16 10 5 21" />
  </svg>
);

const VideosIcon = () => (
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
    <rect x="2" y="4" width="20" height="16" rx="2" />
    <polygon points="10 9 15 12 10 15 10 9" />
  </svg>
);

const AudioIcon = () => (
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
    <path d="M9 18V5l12-2v13" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="18" cy="16" r="3" />
  </svg>
);

const DocumentsIcon = () => (
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
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
    <polyline points="14 2 14 8 20 8" />
  </svg>
);

const categories: { id: FileCategory; label: string; icon: React.ReactNode }[] =
  [
    { id: "all", label: "All Files", icon: <AllFilesIcon /> },
    { id: "images", label: "Images", icon: <ImagesIcon /> },
    { id: "videos", label: "Videos", icon: <VideosIcon /> },
    { id: "audio", label: "Audio", icon: <AudioIcon /> },
    { id: "documents", label: "Documents", icon: <DocumentsIcon /> },
  ];

function Sidebar({ activeCategory, onCategoryChange }: SidebarProps) {
  return (
    <aside className="w-56 h-full border-r border-zinc-200 dark:border-zinc-700 bg-zinc-50 dark:bg-zinc-800/50 pt-12 flex flex-col">
      <nav className="flex flex-col gap-0.5 px-2">
        {categories.map((category) => (
          <SidebarItem
            key={category.id}
            icon={category.icon}
            label={category.label}
            isActive={activeCategory === category.id}
            onClick={() => onCategoryChange(category.id)}
          />
        ))}
      </nav>
    </aside>
  );
}

export default Sidebar;
