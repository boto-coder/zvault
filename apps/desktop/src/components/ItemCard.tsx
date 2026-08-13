interface ItemSummary {
  id: string;
  kind: string;
  name: string;
  username: string | null;
  favourite: boolean;
  createdAt: string;
  updatedAt: string;
}

interface Props {
  item: ItemSummary;
  onClick: () => void;
  onDelete: () => void;
}

const kindIcons: Record<string, string> = {
  login: "🔑",
  secure_note: "📝",
  card: "💳",
  identity: "👤",
};

function ItemCard({ item, onClick, onDelete }: Props) {
  return (
    <div
      className="flex items-center gap-3 p-3 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 hover:border-zvault-300 dark:hover:border-zvault-600 transition-colors cursor-pointer group"
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
      role="button"
      tabIndex={0}
      aria-label={`Open ${item.name}`}
    >
      {/* Icon */}
      <div className="text-2xl w-10 h-10 flex items-center justify-center bg-gray-100 dark:bg-gray-700 rounded-lg">
        {kindIcons[item.kind] || "📦"}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">
            {item.name}
          </h3>
          {item.favourite && (
            <span className="text-yellow-500 text-xs" aria-label="Favourite">
              ★
            </span>
          )}
        </div>
        {item.username && (
          <p className="text-xs text-gray-500 dark:text-gray-400 truncate">
            {item.username}
          </p>
        )}
      </div>

      {/* Delete button */}
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="opacity-0 group-hover:opacity-100 p-1.5 text-red-500 hover:text-red-700 hover:bg-red-50 dark:hover:bg-red-900/30 rounded transition-all"
        aria-label={`Delete ${item.name}`}
      >
        <svg
          className="w-4 h-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
          />
        </svg>
      </button>
    </div>
  );
}

export default ItemCard;
