import { useState, useEffect, useCallback } from "react";

interface RelayEntry {
  url: string;
  enabled: boolean;
  added_at: string;
}

interface RelaySettingsProps {
  onBack?: () => void;
}

export function RelaySettings({ onBack }: RelaySettingsProps) {
  const [relays, setRelays] = useState<RelayEntry[]>([]);
  const [newUrl, setNewUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const loadRelays = useCallback(async () => {
    const response = (await browser.runtime.sendMessage({
      type: "GET_RELAY_SETTINGS",
    })) as { error?: string; relays?: RelayEntry[] };
    if (response.error) {
      setError(response.error);
    } else {
      setRelays(response.relays || []);
      setError(null);
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
    const response = (await browser.runtime.sendMessage({
      type: "ADD_RELAY",
      payload: { url },
    })) as { error?: string; success?: boolean };
    if (response.error) {
      setError(response.error);
    } else {
      setNewUrl("");
      await loadRelays();
    }
    setLoading(false);
  };

  const handleRemove = async (url: string) => {
    setLoading(true);
    setError(null);
    const response = (await browser.runtime.sendMessage({
      type: "REMOVE_RELAY",
      payload: { url },
    })) as { error?: string; success?: boolean };
    if (response.error) {
      setError(response.error);
    } else {
      await loadRelays();
    }
    setLoading(false);
  };

  const handleToggle = async (url: string, enabled: boolean) => {
    setLoading(true);
    setError(null);
    const response = (await browser.runtime.sendMessage({
      type: "TOGGLE_RELAY",
      payload: { url, enabled },
    })) as { error?: string; success?: boolean };
    if (response.error) {
      setError(response.error);
    } else {
      await loadRelays();
    }
    setLoading(false);
  };

  const handleReset = async () => {
    setLoading(true);
    setError(null);
    const response = (await browser.runtime.sendMessage({
      type: "RESET_RELAYS",
    })) as { error?: string; success?: boolean };
    if (response.error) {
      setError(response.error);
    } else {
      await loadRelays();
    }
    setLoading(false);
  };

  const allDisabled = relays.length > 0 && relays.every((r) => !r.enabled);

  const styles = {
    container: {
      padding: "16px",
      backgroundColor: "#1a1a2e",
      color: "#e0e0e0",
      minHeight: "100%",
    } as React.CSSProperties,
    header: {
      display: "flex",
      alignItems: "center",
      gap: "8px",
      marginBottom: "16px",
    } as React.CSSProperties,
    title: {
      fontSize: "16px",
      fontWeight: 600,
      margin: 0,
    } as React.CSSProperties,
    backButton: {
      background: "none",
      border: "none",
      color: "#8888ff",
      cursor: "pointer",
      fontSize: "14px",
      padding: "4px 8px",
    } as React.CSSProperties,
    warning: {
      marginBottom: "12px",
      padding: "10px",
      backgroundColor: "rgba(255, 180, 0, 0.1)",
      border: "1px solid #b8860b",
      borderRadius: "6px",
      color: "#ffd700",
      fontSize: "12px",
    } as React.CSSProperties,
    errorBox: {
      marginBottom: "12px",
      padding: "10px",
      backgroundColor: "rgba(255, 60, 60, 0.1)",
      border: "1px solid #cc3333",
      borderRadius: "6px",
      color: "#ff6666",
      fontSize: "12px",
    } as React.CSSProperties,
    inputRow: {
      display: "flex",
      gap: "8px",
      marginBottom: "12px",
    } as React.CSSProperties,
    input: {
      flex: 1,
      padding: "8px 12px",
      backgroundColor: "#2a2a4a",
      border: "1px solid #444",
      borderRadius: "6px",
      color: "#fff",
      fontSize: "13px",
      outline: "none",
    } as React.CSSProperties,
    addButton: {
      padding: "8px 16px",
      backgroundColor: "#4444cc",
      border: "none",
      borderRadius: "6px",
      color: "#fff",
      fontSize: "13px",
      cursor: "pointer",
      fontWeight: 500,
    } as React.CSSProperties,
    relayItem: {
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      padding: "10px 12px",
      backgroundColor: "#2a2a4a",
      border: "1px solid #333",
      borderRadius: "6px",
      marginBottom: "8px",
    } as React.CSSProperties,
    relayUrl: {
      fontSize: "13px",
      overflow: "hidden",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap" as const,
      flex: 1,
      marginLeft: "10px",
    },
    removeButton: {
      background: "none",
      border: "none",
      color: "#ff6666",
      cursor: "pointer",
      fontSize: "14px",
      padding: "4px 8px",
      marginLeft: "8px",
    } as React.CSSProperties,
    resetButton: {
      marginTop: "8px",
      padding: "8px 14px",
      backgroundColor: "#3a3a5a",
      border: "1px solid #555",
      borderRadius: "6px",
      color: "#ccc",
      fontSize: "12px",
      cursor: "pointer",
    } as React.CSSProperties,
    toggle: {
      width: "36px",
      height: "20px",
      borderRadius: "10px",
      border: "none",
      cursor: "pointer",
      position: "relative" as const,
      transition: "background-color 0.2s",
    },
    toggleKnob: {
      width: "16px",
      height: "16px",
      borderRadius: "50%",
      backgroundColor: "#fff",
      position: "absolute" as const,
      top: "2px",
      transition: "left 0.2s",
    },
    emptyText: {
      color: "#888",
      fontSize: "13px",
      fontStyle: "italic" as const,
    },
  };

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        {onBack && (
          <button style={styles.backButton} onClick={onBack}>
            ← Back
          </button>
        )}
        <h2 style={styles.title}>Relay Settings</h2>
      </div>

      {allDisabled && (
        <div style={styles.warning}>
          ⚠ All relays are disabled. Sync will not work until at least one relay
          is enabled.
        </div>
      )}

      {error && <div style={styles.errorBox}>{error}</div>}

      {/* Add relay input */}
      <div style={styles.inputRow}>
        <input
          style={styles.input}
          type="text"
          value={newUrl}
          onChange={(e) => setNewUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          placeholder="wss://relay.example.com"
          disabled={loading}
          aria-label="New relay URL"
        />
        <button
          style={{
            ...styles.addButton,
            opacity: loading || !newUrl.trim() ? 0.5 : 1,
          }}
          onClick={handleAdd}
          disabled={loading || !newUrl.trim()}
        >
          Add
        </button>
      </div>

      {/* Relay list */}
      <div>
        {relays.length === 0 && (
          <p style={styles.emptyText}>No relays configured.</p>
        )}
        {relays.map((relay) => (
          <div key={relay.url} style={styles.relayItem}>
            <button
              style={{
                ...styles.toggle,
                backgroundColor: relay.enabled ? "#4444cc" : "#555",
              }}
              onClick={() => handleToggle(relay.url, !relay.enabled)}
              disabled={loading}
              aria-label={`Toggle ${relay.url}`}
            >
              <div
                style={{
                  ...styles.toggleKnob,
                  left: relay.enabled ? "18px" : "2px",
                }}
              />
            </button>
            <span
              style={{
                ...styles.relayUrl,
                color: relay.enabled ? "#e0e0e0" : "#888",
              }}
            >
              {relay.url}
            </span>
            <button
              style={styles.removeButton}
              onClick={() => handleRemove(relay.url)}
              disabled={loading}
              aria-label={`Remove ${relay.url}`}
            >
              ✕
            </button>
          </div>
        ))}
      </div>

      {/* Reset button */}
      <button
        style={{ ...styles.resetButton, opacity: loading ? 0.5 : 1 }}
        onClick={handleReset}
        disabled={loading}
      >
        Reset to Defaults
      </button>
    </div>
  );
}
