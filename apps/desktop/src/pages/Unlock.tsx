import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onUnlocked: () => void;
}

function Unlock({ onUnlocked }: Props) {
  const [password, setPassword] = useState("");
  const [path, setPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [mode, setMode] = useState<"open" | "create">("open");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      if (mode === "open") {
        await invoke("open_vault", { password, path });
      } else {
        await invoke("create_vault", { password, path });
      }
      onUnlocked();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-center min-h-screen bg-gradient-to-br from-zvault-50 to-zvault-100 dark:from-gray-900 dark:to-gray-800">
      <div className="w-full max-w-md p-8 bg-white dark:bg-gray-800 rounded-xl shadow-lg">
        <div className="text-center mb-8">
          <h1 className="text-3xl font-bold text-zvault-700 dark:text-zvault-300">
            ZVault
          </h1>
          <p className="text-sm text-gray-500 dark:text-gray-400 mt-2">
            Local-first encrypted password manager
          </p>
        </div>

        <div className="flex mb-6 border-b border-gray-200 dark:border-gray-700">
          <button
            type="button"
            className={`flex-1 pb-2 text-sm font-medium ${
              mode === "open"
                ? "text-zvault-600 border-b-2 border-zvault-600"
                : "text-gray-500 hover:text-gray-700"
            }`}
            onClick={() => setMode("open")}
          >
            Open Vault
          </button>
          <button
            type="button"
            className={`flex-1 pb-2 text-sm font-medium ${
              mode === "create"
                ? "text-zvault-600 border-b-2 border-zvault-600"
                : "text-gray-500 hover:text-gray-700"
            }`}
            onClick={() => setMode("create")}
          >
            Create Vault
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label
              htmlFor="vault-path"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Vault file path
            </label>
            <input
              id="vault-path"
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/path/to/my.zvault"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
              required
            />
          </div>

          <div>
            <label
              htmlFor="master-password"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Master password
            </label>
            <input
              id="master-password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="Enter your master password"
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
              required
            />
          </div>

          {error && (
            <div
              className="p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg"
              role="alert"
            >
              {error}
            </div>
          )}

          <button
            type="submit"
            disabled={loading}
            className="w-full py-2.5 px-4 bg-zvault-600 hover:bg-zvault-700 disabled:bg-zvault-400 text-white font-medium rounded-lg transition-colors focus:ring-2 focus:ring-zvault-500 focus:ring-offset-2"
          >
            {loading
              ? "Unlocking…"
              : mode === "open"
                ? "Unlock"
                : "Create"}
          </button>
        </form>
      </div>
    </div>
  );
}

export default Unlock;
