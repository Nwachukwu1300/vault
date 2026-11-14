const SearchIcon = () => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="text-zinc-400"
  >
    <circle cx="11" cy="11" r="8" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);

function SearchBar() {
  return (
    <div className="w-full max-w-3xl mx-auto">
      <div className="relative">
        <div className="absolute inset-y-0 left-4 flex items-center pointer-events-none">
          <SearchIcon />
        </div>
        <input
          type="text"
          placeholder="Search anything... try 'team dinner'"
          className="
            w-full py-3.5 pl-12 pr-4
            text-base text-zinc-900 dark:text-zinc-100
            placeholder:text-zinc-400 dark:placeholder:text-zinc-500
            bg-white dark:bg-zinc-800
            border border-zinc-200 dark:border-zinc-700
            rounded-xl
            shadow-sm
            outline-none
            focus:ring-2 focus:ring-accent focus:border-transparent
            transition-shadow duration-150 ease-in-out
          "
        />
      </div>
    </div>
  );
}

export default SearchBar;
