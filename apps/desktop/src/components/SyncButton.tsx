import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface SyncResult {
  peersSent: number;
  relaysPublished: number;
  messagesReceived: number;
  vaultVersion: number;
  warnings: string[];
}

interface Props {
  onSyncComplete?: () => void;
}

/**
 * Sync Now button for the desktop toolbar.
 * Triggers a full bidirectional sync (push to all peers + pull pending).
 * Shows spinner during sync, then a brief toast with results.
 */
export default function SyncButton({ onSyncComplete }: Props) {
  const [syncing, setSyncing] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  async function handleSync() {
    if (syncing) return;
    setSyncing(true);
    setToast(null);

    try {
      const result = await invoke<SyncResult>("force_sync");
      const msg = `Synced: sent to ${result.peersSent} device(s), received ${result.messagesReceived} message(s). v${result.vaultVersion}`;
      setToast(msg);

      if (result.warnings.length > 0) {
        console.warn("[ZVault] Sync warnings:", result.warnings);
      }

      onSyncComplete?.();
    } catch (err) {
      setToast(`Sync error: ${err}`);
    } finally {
      setSyncing(false);
      // Hide toast after 4 seconds
      setTimeout(() => setToast(null), 4000);
    }
  }

  return (
    <>
      <button
        onClick={handleSync}
        disabled={syncing}
        title="Sync Now — push and pull from all relays"
        style={{
          padding: "0.4rem 0.8rem",
          borderRadius: "4px",
          border: "1px solid #444",
          background: syncing ? "#1a1a3a" : "transparent",
          color: "#e0e0e0",
          cursor: syncing ? "wait" : "pointer",
          fontSize: "0.85rem",
          display: "flex",
          alignItems: "center",
          gap: "0.3rem",
        }}
        aria-label="Sync Now"
      >
        <span
          style={{
            display: "inline-block",
            animation: syncing ? "spin 1s linear infinite" : "none",
          }}
        >
          🔄
        </span>
        {syncing ? "Syncing…" : "Sync"}
      </button>

      {toast && (
        <div
          role="status"
          style={{
            position: "fixed",
            bottom: "16px",
            left: "50%",
            transform: "translateX(-50%)",
            background: "#0f3460",
            color: "#e0e0e0",
            padding: "0.6rem 1.2rem",
            borderRadius: "6px",
            fontSize: "0.85rem",
            zIndex: 9999,
            boxShadow: "0 2px 12px rgba(0,0,0,0.4)",
            maxWidth: "90vw",
            textAlign: "center",
          }}
        >
          {toast}
        </div>
      )}

      <style>{`
        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
      `}</style>
    </>
  );
}
