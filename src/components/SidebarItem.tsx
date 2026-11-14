import { ReactNode } from "react";

interface SidebarItemProps {
  icon: ReactNode;
  label: string;
  isActive: boolean;
  onClick: () => void;
}

function SidebarItem({ icon, label, isActive, onClick }: SidebarItemProps) {
  return (
    <button
      onClick={onClick}
      className={`
        w-full flex items-center gap-3 px-3 py-2 text-left text-sm
        transition-colors duration-150 ease-in-out
        ${
          isActive
            ? "border-l-[3px] border-accent bg-accent-light/50 dark:bg-accent/10 text-zinc-900 dark:text-zinc-100"
            : "border-l-[3px] border-transparent text-zinc-600 dark:text-zinc-400 hover:bg-zinc-100 dark:hover:bg-zinc-800"
        }
      `}
    >
      <span className="w-5 h-5 flex items-center justify-center">{icon}</span>
      <span className="font-medium">{label}</span>
    </button>
  );
}

export default SidebarItem;
