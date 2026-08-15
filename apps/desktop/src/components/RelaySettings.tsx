import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface RelayEntry {
  url: string;
  enabled: boolean;
  addedAt: string;
}

export function RelaySettings() {
  const [relays, setRelays] = useState<RelayEntry[]>([]);
  const [newUrl, setNewUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const loadRelays = useCallback(async () => {
    try {
      const result = await invoke<RelayEntry[]>("get_relay_settings");
      setRelays(result);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    loadRelays();
  }, [loadRelays]);

  const handleAdd = async () => {
    const url = newUrl.trim();
    if (!url) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("add_relay", { url });
      setNewUrl("");
      await loadRelays();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleRemove = async (url: string) => {
    setLoading(true);
    setError(null);
    try {
      await invoke("remove_relay", { url });
      await loadRelays();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleToggle = async (url: string, enabled: boolean) => {
    setLoading(true);
    setError(null);
    try {
      await invoke("toggle_relay", { url, enabled });
      await loadRelays();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleReset = async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke("reset_relays");
      await loadRelays();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const allDisabled = relays.length > 0 && relays.every((r) => !r.enabled);

  return (
    <div className="p-4">
      <h2 className="text-xl font-semibold mb-4">Relay Settings</h2>

      {allDisabled && (
        <div className="mb-4 p-3 bg-yellow-900/30 border border-yellow-600 rounded text-yellow-200 text-sm">
          ⚠ All relays are disabled. Sync will not work until at least one relay
          is enabled.
        </div>
      )}

      {error && (
        <div className="mb-4 p-3 bg-red-900/30 border border-red-600 rounded text-red-200 text-sm">
          {error}
        </div>
      )}

      {/* Add relay input */}
      <div className="flex gap-2 mb-4">
        <input
          type="text"
          value={newUrl}
          onChange={(e) => setNewUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          placeholder="wss://relay.example.com"
          className="flex-1 px-3 py-2 bg-gray-800 border border-gray-600 rounded text-white placeholder-gray-400 text-sm focus:outline-none focus:border-blue-500"
          disabled={loading}
          aria-label="New relay URL"
        />
        <button
          onClick={handleAdd}
          disabled={loading || !newUrl.trim()}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-600 disabled:cursor-not-allowed text-white rounded text-sm font-medium transition-colors"
        >
          Add
        </button>
      </div>

      {/* Relay list */}
      <div className="space-y-2 mb-4">
        {relays.length === 0 && (
          <p className="text-gray-400 text-sm">No relays configured.</p>
        )}
        {relays.map((relay) => (
          <div
            key={relay.url}
            className="flex items-center justify-between p-3 bg-gray-800 border border-gray-700 rounded"
          >
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={relay.enabled}
                  onChange={(e) => handleToggle(relay.url, e.target.checked)}
                  disabled={loading}
                  className="sr-only peer"
                  aria-label={`Toggle ${relay.url}`}
                />
                <div className="w-9 h-5 bg-gray-600 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:bg-blue-600 after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all"></div>
              </label>
              <span
                className={`text-sm truncate ${
                  relay.enabled ? "text-white" : "text-gray-400"
                }`}
              >
                {relay.url}
              </span>
            </div>
            <button
              onClick={() => handleRemove(relay.url)}
              disabled={loading}
              className="ml-2 px-2 py-1 text-red-400 hover:text-red-300 hover:bg-red-900/30 rounded text-sm transition-colors disabled:opacity-50"
              aria-label={`Remove ${relay.url}`}
            >
              ✕
            </button>
          </div>
        ))}
      </div>

      {/* Reset button */}
      <button
        onClick={handleReset}
        disabled={loading}
        className="px-3 py-2 bg-gray-700 hover:bg-gray-600 disabled:opacity-50 text-gray-200 rounded text-sm transition-colors"
      >
        Reset to Defaults
      </button>
    </div>
  );
}
