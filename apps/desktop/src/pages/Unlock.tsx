import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onUnlocked: () => void;
}

/** Map raw backend error strings to user-friendly messages. */
function friendlyError(raw: string): string {
  const msg = String(raw).toLowerCase();
  if (msg.includes("invalid vault file") || msg.includes("authentication")) {
    return "Wrong password or the file is corrupted.";
  }
  if (msg.includes("no such file") || msg.includes("not found") || msg.includes("os error 2")) {
    return "Vault file not found. Check the path and try again.";
  }
  if (msg.includes("permission denied")) {
    return "Permission denied. Check file permissions.";
  }
  if (msg.includes("already exists")) {
    return "A file already exists at that path. Choose a different location.";
  }
  return String(raw);
}

function Unlock({ onUnlocked }: Props) {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [path, setPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [mode, setMode] = useState<"open" | "create">("open");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!path.trim()) {
      setError("Please enter a vault file path.");
      return;
    }

    // Validate password confirmation for create mode
    if (mode === "create") {
      if (password.length < 8) {
        setError("Password must be at least 8 characters.");
        return;
      }
      if (password !== confirmPassword) {
        setError("Passwords do not match.");
        return;
      }
    }

    setLoading(true);

    try {
      if (mode === "open") {
        await invoke("open_vault", { password, path });
      } else {
        await invoke("create_vault", { password, path });
      }
      onUnlocked();
    } catch (err) {
      setError(friendlyError(String(err)));
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

        {/* Mode tabs */}
        <div
          className="flex mb-6 border-b border-gray-200 dark:border-gray-700"
          role="tablist"
          aria-label="Vault action"
        >
          <button
            type="button"
            role="tab"
            aria-selected={mode === "open"}
            aria-controls="unlock-form"
            className={`flex-1 pb-2 text-sm font-medium transition-colors ${
              mode === "open"
                ? "text-zvault-600 border-b-2 border-zvault-600"
                : "text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
            }`}
            onClick={() => { setMode("open"); setError(null); }}
          >
            Unlock Existing Vault
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === "create"}
            aria-controls="unlock-form"
            className={`flex-1 pb-2 text-sm font-medium transition-colors ${
              mode === "create"
                ? "text-zvault-600 border-b-2 border-zvault-600"
                : "text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
            }`}
            onClick={() => { setMode("create"); setError(null); }}
          >
            Create New Vault
          </button>
        </div>

        {/* Description text for the active mode */}
        <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
          {mode === "open"
            ? "Enter the path to your existing .zvault file and your master password."
            : "Choose a path for your new vault file and set a strong master password."}
        </p>

        <form id="unlock-form" onSubmit={handleSubmit} className="space-y-4">
          {/* Path input */}
          <div>
            <label
              htmlFor="vault-path"
              className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              {mode === "open" ? "Vault file path" : "New vault path"}
            </label>
            <input
              id="vault-path"
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder={mode === "open" ? "/path/to/my.zvault" : "/path/to/new.zvault"}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent text-sm"
              required
              aria-describedby="path-hint"
            />
            <p id="path-hint" className="text-xs text-gray-400 dark:text-gray-500 mt-1">
              {mode === "open"
                ? "Full path to the encrypted .zvault file on disk."
                : "Full path where the new vault will be created (e.g. ~/my.zvault)."}
            </p>
          </div>

          {/* Master password */}
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
              placeholder={mode === "open" ? "Enter your master password" : "Choose a strong password (min 8 chars)"}
              className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
              required
              autoComplete={mode === "open" ? "current-password" : "new-password"}
            />
          </div>

          {/* Confirm password (create mode only) */}
          {mode === "create" && (
            <div>
              <label
                htmlFor="confirm-password"
                className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
              >
                Confirm password
              </label>
              <input
                id="confirm-password"
                type="password"
                value={confirmPassword}
                onChange={(e) => setConfirmPassword(e.target.value)}
                placeholder="Re-enter your master password"
                className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
                required
                autoComplete="new-password"
              />
              <p className="text-xs text-amber-600 dark:text-amber-400 mt-1">
                ⚠ This password cannot be recovered. Make sure you remember it.
              </p>
            </div>
          )}

          {/* Error display */}
          {error && (
            <div
              className="p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg"
              role="alert"
              aria-live="assertive"
            >
              {error}
            </div>
          )}

          {/* Submit button */}
          <button
            type="submit"
            disabled={loading}
            className="w-full py-2.5 px-4 bg-zvault-600 hover:bg-zvault-700 disabled:bg-zvault-400 text-white font-medium rounded-lg transition-colors focus:ring-2 focus:ring-zvault-500 focus:ring-offset-2"
          >
            {loading
              ? mode === "open" ? "Unlocking…" : "Creating…"
              : mode === "open"
                ? "Unlock Vault"
                : "Create Vault"}
          </button>
        </form>
      </div>
    </div>
  );
}

export default Unlock;
