import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ItemCard from "../components/ItemCard";

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
  onSelectItem: (id: string) => void;
  onLocked: () => void;
}

function VaultList({ onSelectItem, onLocked }: Props) {
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);

  const loadItems = async () => {
    try {
      const result = await invoke<ItemSummary[]>("list_items");
      setItems(result);
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    loadItems();
  }, []);

  const handleLock = async () => {
    try {
      await invoke("lock_vault");
      onLocked();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleAddItem = async (name: string, kind: string) => {
    try {
      const itemJson = JSON.stringify({ name, kind });
      await invoke("add_item", { itemJson });
      setShowAdd(false);
      loadItems();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleDeleteItem = async (id: string) => {
    try {
      await invoke("delete_item", { id });
      loadItems();
    } catch (err) {
      setError(String(err));
    }
  };

  const filteredItems = items.filter(
    (item) =>
      item.name.toLowerCase().includes(filter.toLowerCase()) ||
      (item.username &&
        item.username.toLowerCase().includes(filter.toLowerCase()))
  );

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900">
      {/* Header */}
      <header className="bg-white dark:bg-gray-800 shadow-sm border-b border-gray-200 dark:border-gray-700">
        <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between">
          <h1 className="text-xl font-bold text-zvault-700 dark:text-zvault-300">
            ZVault
          </h1>
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={() => setShowAdd(true)}
              className="px-3 py-1.5 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
            >
              + Add Item
            </button>
            <button
              type="button"
              onClick={handleLock}
              className="px-3 py-1.5 text-sm bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg transition-colors"
            >
              Lock
            </button>
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6">
        {/* Search */}
        <div className="mb-4">
          <input
            type="search"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Search items…"
            className="w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
            aria-label="Search vault items"
          />
        </div>

        {error && (
          <div
            className="mb-4 p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg"
            role="alert"
          >
            {error}
          </div>
        )}

        {/* Item list */}
        {filteredItems.length === 0 ? (
          <div className="text-center py-12 text-gray-500 dark:text-gray-400">
            {items.length === 0
              ? "Your vault is empty. Add your first item to get started."
              : "No items match your search."}
          </div>
        ) : (
          <div className="space-y-2">
            {filteredItems.map((item) => (
              <ItemCard
                key={item.id}
                item={item}
                onClick={() => onSelectItem(item.id)}
                onDelete={() => handleDeleteItem(item.id)}
              />
            ))}
          </div>
        )}
      </main>

      {/* Add Item Modal */}
      {showAdd && (
        <AddItemModal
          onAdd={handleAddItem}
          onClose={() => setShowAdd(false)}
        />
      )}
    </div>
  );
}

// ─── Add Item Modal ──────────────────────────────────────────────────────────

interface AddItemModalProps {
  onAdd: (name: string, kind: string) => void;
  onClose: () => void;
}

function AddItemModal({ onAdd, onClose }: AddItemModalProps) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState("login");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (name.trim()) {
      onAdd(name.trim(), kind);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
          Add New Item
        </h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="item-name"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Name
            </label>
            <input
              id="item-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. GitHub"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
              required
              autoFocus
            />
          </div>
          <div>
            <label
              htmlFor="item-kind"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Type
            </label>
            <select
              id="item-kind"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
            >
              <option value="login">Login</option>
              <option value="secure_note">Secure Note</option>
              <option value="card">Card</option>
              <option value="identity">Identity</option>
            </select>
          </div>
          <div className="flex gap-3 justify-end">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-2 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
            >
              Add
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

export default VaultList;
