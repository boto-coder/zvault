import React, { useState } from "react";

type SyncStatus = "idle" | "syncing" | "synced" | "error";

interface ForceSyncResult {
  sent: number;
  received: number;
  version: number;
  warnings: string[];
  error?: string;
}

/**
 * Sync Now button for the browser extension popup.
 * Sends FORCE_SYNC message to background script, shows status indicator.
 */
export function SyncButton() {
  const [status, setStatus] = useState<SyncStatus>("idle");
  const [message, setMessage] = useState<string | null>(null);

  async function handleSync() {
    if (status === "syncing") return;
    setStatus("syncing");
    setMessage(null);

    try {
      const response = (await browser.runtime.sendMessage({
        type: "FORCE_SYNC",
      })) as ForceSyncResult;

      if (response.error) {
        setStatus("error");
        setMessage(response.error);
      } else {
        setStatus("synced");
        setMessage(
          `Sent: ${response.sent}, Received: ${response.received}`
        );
        if (response.warnings && response.warnings.length > 0) {
          console.warn("[ZVault] Sync warnings:", response.warnings);
        }
      }
    } catch (err) {
      setStatus("error");
      setMessage(String(err));
    }

    // Reset to idle after 3 seconds
    setTimeout(() => {
      setStatus("idle");
      setMessage(null);
    }, 3000);
  }

  const statusIcon =
    status === "syncing"
      ? "⏳"
      : status === "synced"
        ? "✓"
        : status === "error"
          ? "⚠"
          : "🔄";

  const buttonStyle: React.CSSProperties = {
    padding: "0.3rem 0.6rem",
    borderRadius: "4px",
    border: "1px solid #444",
    background: status === "syncing" ? "#1a1a3a" : "transparent",
    color:
      status === "synced"
        ? "#4ade80"
        : status === "error"
          ? "#ff6b6b"
          : "#e0e0e0",
    cursor: status === "syncing" ? "wait" : "pointer",
    fontSize: "0.8rem",
    display: "flex",
    alignItems: "center",
    gap: "0.25rem",
    transition: "all 0.2s ease",
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", alignItems: "center" }}>
      <button
        onClick={handleSync}
        disabled={status === "syncing"}
        style={buttonStyle}
        title="Sync Now — push and pull from all relays"
        aria-label="Sync Now"
      >
        <span
          style={{
            display: "inline-block",
            animation: status === "syncing" ? "sync-spin 1s linear infinite" : "none",
          }}
        >
          {statusIcon}
        </span>
        {status === "syncing" ? "Syncing…" : "Sync"}
      </button>

      {message && (
        <div
          style={{
            fontSize: "0.7rem",
            color: status === "error" ? "#ff6b6b" : "#4ade80",
            marginTop: "0.25rem",
            textAlign: "center",
            maxWidth: "200px",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
          role="status"
        >
          {message}
        </div>
      )}

      <style>{`
        @keyframes sync-spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
      `}</style>
    </div>
  );
}
