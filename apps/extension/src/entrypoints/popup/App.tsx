import React, { useEffect, useState } from "react";

// ─── Types ──────────────────────────────────────────────────────────────────

interface VaultItem {
  id: string;
  kind: string;
  name: string;
  username?: string;
  uris?: { uri: string }[];
}

type View = "loading" | "unlock" | "items";

// ─── App ────────────────────────────────────────────────────────────────────

export function App() {
  const [view, setView] = useState<View>("loading");
  const [items, setItems] = useState<VaultItem[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    checkStatus();
  }, []);

  async function checkStatus() {
    const response = await browser.runtime.sendMessage({ type: "GET_STATUS" });
    if (response.unlocked) {
      await loadItems();
    } else {
      setView("unlock");
    }
  }

  async function loadItems() {
    const response = await browser.runtime.sendMessage({ type: "LIST_ITEMS" });
    if (response.error) {
      setError(response.error);
      setView("unlock");
    } else {
      setItems(response.items || []);
      setView("items");
    }
  }

  async function handleUnlock(password: string) {
    setError(null);
    // Try to load existing vault from storage
    const stored = await browser.storage.local.get("vault");
    if (stored.vault) {
      const response = await browser.runtime.sendMessage({
        type: "UNLOCK",
        payload: { password, data: stored.vault },
      });
      if (response.error) {
        setError(response.error);
      } else {
        await loadItems();
      }
    } else {
      // No vault exists yet — create one
      const response = await browser.runtime.sendMessage({
        type: "CREATE",
        payload: { password },
      });
      if (response.error) {
        setError(response.error);
      } else if (response.data) {
        await browser.storage.local.set({ vault: response.data });
        await loadItems();
      }
    }
  }

  async function handleLock() {
    await browser.runtime.sendMessage({ type: "LOCK" });
    setView("unlock");
    setItems([]);
  }

  async function handleCopyPassword(itemId: string) {
    // For now, find the item in list and notify user.
    // Full implementation would decrypt and copy to clipboard.
    const item = items.find((i) => i.id === itemId);
    if (item) {
      // In a full implementation, we'd fetch the password from the background
      // and use navigator.clipboard.writeText(). For now, show a brief notification.
      await navigator.clipboard.writeText(`[password for ${item.name}]`);
    }
  }

  switch (view) {
    case "loading":
      return <LoadingView />;
    case "unlock":
      return <UnlockView onUnlock={handleUnlock} error={error} />;
    case "items":
      return (
        <ItemListView
          items={items}
          onLock={handleLock}
          onCopyPassword={handleCopyPassword}
        />
      );
  }
}

// ─── Loading ────────────────────────────────────────────────────────────────

function LoadingView() {
  return (
    <div style={{ padding: "2rem", textAlign: "center" }}>
      <p>Loading ZVault…</p>
    </div>
  );
}

// ─── Unlock ─────────────────────────────────────────────────────────────────

function UnlockView({
  onUnlock,
  error,
}: {
  onUnlock: (password: string) => void;
  error: string | null;
}) {
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!password.trim()) return;
    setLoading(true);
    await onUnlock(password);
    setLoading(false);
  }

  return (
    <div style={{ padding: "1.5rem" }}>
      <h1 style={{ fontSize: "1.25rem", marginBottom: "1rem" }}>
        🔐 ZVault
      </h1>
      <form onSubmit={handleSubmit}>
        <label
          htmlFor="password"
          style={{ display: "block", marginBottom: "0.5rem", fontSize: "0.9rem" }}
        >
          Master Password
        </label>
        <input
          id="password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Enter master password"
          autoFocus
          disabled={loading}
          style={{
            width: "100%",
            padding: "0.6rem",
            borderRadius: "4px",
            border: "1px solid #444",
            background: "#16213e",
            color: "#e0e0e0",
            marginBottom: "0.75rem",
            fontSize: "1rem",
          }}
        />
        {error && (
          <p
            style={{ color: "#ff6b6b", fontSize: "0.85rem", marginBottom: "0.5rem" }}
            role="alert"
          >
            {error}
          </p>
        )}
        <button
          type="submit"
          disabled={loading || !password.trim()}
          style={{
            width: "100%",
            padding: "0.6rem",
            borderRadius: "4px",
            border: "none",
            background: "#0f3460",
            color: "#e0e0e0",
            fontSize: "1rem",
            cursor: loading ? "wait" : "pointer",
          }}
        >
          {loading ? "Unlocking…" : "Unlock"}
        </button>
      </form>
    </div>
  );
}

// ─── Item List ──────────────────────────────────────────────────────────────

function ItemListView({
  items,
  onLock,
  onCopyPassword,
}: {
  items: VaultItem[];
  onLock: () => void;
  onCopyPassword: (id: string) => void;
}) {
  return (
    <div style={{ padding: "1rem" }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "1rem",
        }}
      >
        <h1 style={{ fontSize: "1.1rem" }}>🔓 ZVault</h1>
        <button
          onClick={onLock}
          style={{
            padding: "0.3rem 0.6rem",
            borderRadius: "4px",
            border: "1px solid #444",
            background: "transparent",
            color: "#e0e0e0",
            cursor: "pointer",
            fontSize: "0.8rem",
          }}
        >
          Lock
        </button>
      </div>

      {items.length === 0 ? (
        <p style={{ color: "#888", textAlign: "center", padding: "2rem 0" }}>
          No items yet. Add credentials from the desktop app.
        </p>
      ) : (
        <ul style={{ listStyle: "none" }}>
          {items.map((item) => (
            <li
              key={item.id}
              style={{
                padding: "0.6rem",
                borderBottom: "1px solid #2a2a4a",
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
              }}
            >
              <div>
                <div style={{ fontWeight: 500 }}>{item.name}</div>
                {item.username && (
                  <div style={{ fontSize: "0.8rem", color: "#888" }}>
                    {item.username}
                  </div>
                )}
              </div>
              {item.kind === "login" && (
                <button
                  onClick={() => onCopyPassword(item.id)}
                  title="Copy password"
                  style={{
                    padding: "0.25rem 0.5rem",
                    borderRadius: "4px",
                    border: "1px solid #444",
                    background: "transparent",
                    color: "#e0e0e0",
                    cursor: "pointer",
                    fontSize: "0.75rem",
                  }}
                >
                  📋
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
