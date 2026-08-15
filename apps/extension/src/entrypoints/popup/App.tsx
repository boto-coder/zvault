import React, { useEffect, useState, useCallback } from "react";

// ─── Types ──────────────────────────────────────────────────────────────────

interface VaultItem {
  id: string;
  kind: string;
  name: string;
  username?: string;
  password?: string;
  totp_secret?: string;
  uris?: { uri: string; match_strategy?: string }[];
  note?: string;
  cardholder_name?: string;
  card_number?: string;
  expiry_date?: string;
  cvv?: string;
  first_name?: string;
  last_name?: string;
  address?: string;
  city?: string;
  country?: string;
  phone?: string;
  email?: string;
  created_at?: string;
  updated_at?: string;
}

interface DeviceEntry {
  device_id: string;
  nostr_pubkey: string;
  label: string;
  added_at: string;
  added_by: string;
  revoked: boolean;
  revoked_at?: string;
}

type View =
  | "loading"
  | "create"
  | "unlock"
  | "items"
  | "create-item"
  | "item-detail"
  | "devices";

type ItemKind = "login" | "secure_note" | "card" | "identity";

interface UriEntry {
  uri: string;
  match_strategy: "Domain" | "Host" | "StartsWith" | "Exact" | "Regex" | "Never";
}

interface MessageResponse {
  success?: boolean;
  error?: string;
  unlocked?: boolean;
  items?: VaultItem[];
  item?: VaultItem;
  data?: number[];
  password?: string;
  code?: string;
  devices?: DeviceEntry[];
  deviceId?: string;
}

// ─── Toast Component ────────────────────────────────────────────────────────

function Toast({ message, visible }: { message: string; visible: boolean }) {
  if (!visible) return null;
  return (
    <div
      role="status"
      style={{
        position: "fixed",
        top: "8px",
        left: "50%",
        transform: "translateX(-50%)",
        background: "#0f3460",
        color: "#e0e0e0",
        padding: "0.5rem 1rem",
        borderRadius: "4px",
        fontSize: "0.85rem",
        zIndex: 1000,
        boxShadow: "0 2px 8px rgba(0,0,0,0.3)",
      }}
    >
      {message}
    </div>
  );
}

// ─── App ────────────────────────────────────────────────────────────────────

export function App() {
  const [view, setView] = useState<View>("loading");
  const [items, setItems] = useState<VaultItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);

  // Toast state
  const [toastMessage, setToastMessage] = useState("");
  const [toastVisible, setToastVisible] = useState(false);

  const showToast = useCallback((msg: string) => {
    setToastMessage(msg);
    setToastVisible(true);
    setTimeout(() => setToastVisible(false), 2000);
  }, []);

  useEffect(() => {
    checkStatus();
  }, []);

  async function checkStatus() {
    const response = (await browser.runtime.sendMessage({ type: "GET_STATUS" })) as MessageResponse;
    if (response.unlocked) {
      await loadItems();
    } else {
      const stored = await browser.storage.local.get("vault");
      if (stored.vault) {
        setView("unlock");
      } else {
        setView("create");
      }
    }
  }

  async function loadItems() {
    const response = (await browser.runtime.sendMessage({ type: "LIST_ITEMS" })) as MessageResponse;
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
    const stored = await browser.storage.local.get("vault");
    if (!stored.vault) {
      setView("create");
      return;
    }
    const response = (await browser.runtime.sendMessage({
      type: "UNLOCK",
      payload: { password, data: stored.vault },
    })) as MessageResponse;
    if (response.error) {
      setError(response.error);
    } else {
      await loadItems();
    }
  }

  async function handleCreate(password: string) {
    setError(null);
    const response = (await browser.runtime.sendMessage({
      type: "CREATE",
      payload: { password },
    })) as MessageResponse;
    if (response.error) {
      setError(response.error);
    } else if (response.data) {
      await browser.storage.local.set({ vault: response.data });
      await loadItems();
    }
  }

  async function handleLock() {
    await browser.runtime.sendMessage({ type: "LOCK" });
    setView("unlock");
    setItems([]);
  }

  async function handleCopyPassword(itemId: string) {
    try {
      const response = (await browser.runtime.sendMessage({
        type: "GET_PASSWORD",
        payload: { id: itemId },
      })) as MessageResponse;
      if (response.error) {
        showToast("Failed to copy: " + response.error);
        return;
      }
      if (response.password) {
        await navigator.clipboard.writeText(response.password);
        showToast("Copied!");
        setTimeout(() => {
          navigator.clipboard.writeText("").catch(() => {});
        }, 30000);
      } else {
        showToast("No password set");
      }
    } catch (err) {
      showToast("Failed to copy");
    }
  }

  function handleSelectItem(id: string) {
    setSelectedItemId(id);
    setView("item-detail");
  }

  return (
    <>
      <Toast message={toastMessage} visible={toastVisible} />
      {(() => {
        switch (view) {
          case "loading":
            return <LoadingView />;
          case "create":
            return <CreateVaultView onCreate={handleCreate} error={error} />;
          case "unlock":
            return <UnlockView onUnlock={handleUnlock} error={error} />;
          case "items":
            return (
              <ItemListView
                items={items}
                onLock={handleLock}
                onCopyPassword={handleCopyPassword}
                onAdd={() => setView("create-item")}
                onSelectItem={handleSelectItem}
                onDevices={() => setView("devices")}
                showToast={showToast}
              />
            );
          case "create-item":
            return (
              <ItemCreateView
                onSave={async () => {
                  await loadItems();
                  setView("items");
                }}
                onCancel={() => setView("items")}
              />
            );
          case "item-detail":
            return (
              <ItemDetailView
                itemId={selectedItemId!}
                onBack={() => setView("items")}
                showToast={showToast}
              />
            );
          case "devices":
            return (
              <DevicesView
                onBack={() => setView("items")}
                showToast={showToast}
              />
            );
        }
      })()}
    </>
  );
}

// ─── Loading ────────────────────────────────────────────────────────────────

function LoadingView() {
  return (
    <div style={{ padding: "2rem", textAlign: "center" }}>
      <p>Loading ZVault…</p>
    </div>
  );
}

// ─── Create Vault ───────────────────────────────────────────────────────────

function CreateVaultView({
  onCreate,
  error,
}: {
  onCreate: (password: string) => void;
  error: string | null;
}) {
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setValidationError(null);

    if (!password.trim()) {
      setValidationError("Password is required");
      return;
    }
    if (password !== confirmPassword) {
      setValidationError("Passwords do not match");
      return;
    }
    if (password.length < 8) {
      setValidationError("Password must be at least 8 characters");
      return;
    }

    setLoading(true);
    await onCreate(password);
    setLoading(false);
  }

  const displayError = validationError || error;

  return (
    <div style={{ padding: "1.5rem" }}>
      <h1 style={{ fontSize: "1.25rem", marginBottom: "0.5rem" }}>🔐 ZVault</h1>
      <h2 style={{ fontSize: "1rem", marginBottom: "1rem", color: "#aaa" }}>Create New Vault</h2>
      <form onSubmit={handleSubmit}>
        <label htmlFor="create-password" style={{ display: "block", marginBottom: "0.25rem", fontSize: "0.9rem" }}>
          Master Password
        </label>
        <input
          id="create-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Choose a strong master password"
          autoFocus
          disabled={loading}
          style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "1px solid #444", background: "#16213e", color: "#e0e0e0", marginBottom: "0.75rem", fontSize: "1rem" }}
        />
        <label htmlFor="confirm-password" style={{ display: "block", marginBottom: "0.25rem", fontSize: "0.9rem" }}>
          Confirm Password
        </label>
        <input
          id="confirm-password"
          type="password"
          value={confirmPassword}
          onChange={(e) => setConfirmPassword(e.target.value)}
          placeholder="Re-enter master password"
          disabled={loading}
          style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "1px solid #444", background: "#16213e", color: "#e0e0e0", marginBottom: "0.75rem", fontSize: "1rem" }}
        />
        {displayError && (
          <p style={{ color: "#ff6b6b", fontSize: "0.85rem", marginBottom: "0.5rem" }} role="alert">
            {displayError}
          </p>
        )}
        <button
          type="submit"
          disabled={loading || !password.trim() || !confirmPassword.trim()}
          style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "none", background: "#0f3460", color: "#e0e0e0", fontSize: "1rem", cursor: loading ? "wait" : "pointer" }}
        >
          {loading ? "Creating…" : "Create Vault"}
        </button>
      </form>
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
      <h1 style={{ fontSize: "1.25rem", marginBottom: "1rem" }}>🔐 ZVault</h1>
      <form onSubmit={handleSubmit}>
        <label htmlFor="password" style={{ display: "block", marginBottom: "0.5rem", fontSize: "0.9rem" }}>
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
          style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "1px solid #444", background: "#16213e", color: "#e0e0e0", marginBottom: "0.75rem", fontSize: "1rem" }}
        />
        {error && (
          <p style={{ color: "#ff6b6b", fontSize: "0.85rem", marginBottom: "0.5rem" }} role="alert">
            {error}
          </p>
        )}
        <button
          type="submit"
          disabled={loading || !password.trim()}
          style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "none", background: "#0f3460", color: "#e0e0e0", fontSize: "1rem", cursor: loading ? "wait" : "pointer" }}
        >
          {loading ? "Unlocking…" : "Unlock"}
        </button>
      </form>
    </div>
  );
}

// ─── Item List ──────────────────────────────────────────────────────────────

const kindIcons: Record<string, string> = {
  login: "🔑",
  secure_note: "📝",
  card: "💳",
  identity: "👤",
};

function getDomain(uri: string): string | null {
  try {
    return new URL(uri).hostname;
  } catch {
    return null;
  }
}

function ItemListView({
  items,
  onLock,
  onCopyPassword,
  onAdd,
  onSelectItem,
  onDevices,
  showToast,
}: {
  items: VaultItem[];
  onLock: () => void;
  onCopyPassword: (id: string) => void;
  onAdd: () => void;
  onSelectItem: (id: string) => void;
  onDevices: () => void;
  showToast: (msg: string) => void;
}) {
  const [search, setSearch] = useState("");
  const [currentDomain, setCurrentDomain] = useState<string | null>(null);

  useEffect(() => {
    async function getActiveTabDomain() {
      try {
        const tabs = await browser.tabs.query({ active: true, currentWindow: true });
        const tab = tabs[0];
        if (tab?.url && tab.url.startsWith("https://")) {
          const domain = getDomain(tab.url);
          setCurrentDomain(domain);
        }
      } catch {
        // ignore
      }
    }
    getActiveTabDomain();
  }, []);

  const filteredItems = items.filter(
    (item) =>
      item.name.toLowerCase().includes(search.toLowerCase()) ||
      (item.username && item.username.toLowerCase().includes(search.toLowerCase()))
  );

  // Split into suggested and all
  const suggestedItems = currentDomain
    ? filteredItems.filter((item) =>
        item.uris?.some((u) => {
          const d = getDomain(u.uri);
          return d && d === currentDomain;
        })
      )
    : [];

  const otherItems = currentDomain
    ? filteredItems.filter(
        (item) => !suggestedItems.some((s) => s.id === item.id)
      )
    : filteredItems;

  return (
    <div style={{ padding: "1rem" }}>
      {/* Header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.75rem" }}>
        <h1 style={{ fontSize: "1.1rem" }}>🔓 ZVault</h1>
        <div style={{ display: "flex", gap: "0.4rem" }}>
          <button
            onClick={onAdd}
            aria-label="Add item"
            style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}
          >
            +
          </button>
          <button
            onClick={onDevices}
            aria-label="Devices"
            title="Manage devices"
            style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}
          >
            📱
          </button>
          <button
            onClick={onLock}
            style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}
          >
            Lock
          </button>
        </div>
      </div>

      {/* Search */}
      <input
        type="search"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        placeholder="Search items…"
        autoFocus
        style={{ width: "100%", padding: "0.5rem", borderRadius: "4px", border: "1px solid #444", background: "#16213e", color: "#e0e0e0", fontSize: "0.9rem", marginBottom: "0.75rem" }}
        aria-label="Search vault items"
      />

      {filteredItems.length === 0 ? (
        <p style={{ color: "#888", textAlign: "center", padding: "2rem 0" }}>
          {items.length === 0 ? "No items yet. Click + to add credentials." : "No items match your search."}
        </p>
      ) : (
        <>
          {/* Suggested section */}
          {suggestedItems.length > 0 && (
            <>
              <div style={{ fontSize: "0.75rem", color: "#aaa", textTransform: "uppercase", marginBottom: "0.4rem", letterSpacing: "0.05em" }}>
                Suggested for this site
              </div>
              <ul style={{ listStyle: "none", marginBottom: "0.75rem" }}>
                {suggestedItems.map((item) => (
                  <ItemListRow
                    key={item.id}
                    item={item}
                    onSelect={() => onSelectItem(item.id)}
                    onCopy={() => onCopyPassword(item.id)}
                  />
                ))}
              </ul>
            </>
          )}

          {/* All items */}
          {suggestedItems.length > 0 && otherItems.length > 0 && (
            <div style={{ fontSize: "0.75rem", color: "#aaa", textTransform: "uppercase", marginBottom: "0.4rem", letterSpacing: "0.05em" }}>
              All Items
            </div>
          )}
          <ul style={{ listStyle: "none" }}>
            {otherItems.map((item) => (
              <ItemListRow
                key={item.id}
                item={item}
                onSelect={() => onSelectItem(item.id)}
                onCopy={() => onCopyPassword(item.id)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function ItemListRow({
  item,
  onSelect,
  onCopy,
}: {
  item: VaultItem;
  onSelect: () => void;
  onCopy: () => void;
}) {
  const icon = kindIcons[item.kind] || "📦";
  const firstDomain = item.kind === "login" && item.uris?.[0]
    ? getDomain(item.uris[0].uri)
    : null;

  return (
    <li
      style={{
        padding: "0.6rem",
        borderBottom: "1px solid #2a2a4a",
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        cursor: "pointer",
      }}
      onClick={onSelect}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", minWidth: 0, flex: 1 }}>
        <span style={{ fontSize: "1.1rem", flexShrink: 0 }}>{icon}</span>
        <div style={{ minWidth: 0 }}>
          <div style={{ fontWeight: 500, display: "flex", alignItems: "center", gap: "0.3rem" }}>
            <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{item.name}</span>
            {item.totp_secret && <span title="Has TOTP" style={{ fontSize: "0.75rem" }}>🕐</span>}
          </div>
          {item.username && (
            <div style={{ fontSize: "0.8rem", color: "#888", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {item.username}
            </div>
          )}
          {firstDomain && (
            <div style={{ fontSize: "0.75rem", color: "#666", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {firstDomain}
            </div>
          )}
        </div>
      </div>
      {item.kind === "login" && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onCopy();
          }}
          title="Copy password"
          style={{ padding: "0.25rem 0.5rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.75rem", flexShrink: 0 }}
        >
          📋
        </button>
      )}
    </li>
  );
}

// ─── Item Detail View ───────────────────────────────────────────────────────

function ItemDetailView({
  itemId,
  onBack,
  showToast,
}: {
  itemId: string;
  onBack: () => void;
  showToast: (msg: string) => void;
}) {
  const [item, setItem] = useState<VaultItem | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [showCvv, setShowCvv] = useState(false);
  const [totpCode, setTotpCode] = useState<string | null>(null);
  const [totpCountdown, setTotpCountdown] = useState(30);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      const response = (await browser.runtime.sendMessage({
        type: "GET_ITEM",
        payload: { id: itemId },
      })) as MessageResponse;
      if (response.error) {
        setLoadError(response.error);
      } else if (response.item) {
        setItem(response.item);
      }
    }
    load();
  }, [itemId]);

  // TOTP timer
  useEffect(() => {
    if (!item?.totp_secret) return;

    async function generateTotp() {
      try {
        const response = (await browser.runtime.sendMessage({
          type: "GENERATE_TOTP",
          payload: { secret: item!.totp_secret },
        })) as MessageResponse;
        if (response.code) {
          setTotpCode(response.code);
        }
      } catch {
        // ignore
      }
    }

    generateTotp();
    const interval = setInterval(() => {
      const now = Math.floor(Date.now() / 1000);
      const remaining = 30 - (now % 30);
      setTotpCountdown(remaining);
      if (remaining === 30) {
        generateTotp();
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [item?.totp_secret]);

  const copyField = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      showToast(`${label} copied!`);
      setTimeout(() => {
        navigator.clipboard.writeText("").catch(() => {});
      }, 30000);
    } catch {
      showToast("Failed to copy");
    }
  };

  if (loadError) {
    return (
      <div style={{ padding: "1rem" }}>
        <button onClick={onBack} style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem", marginBottom: "1rem" }}>
          ← Back
        </button>
        <p style={{ color: "#ff6b6b" }}>{loadError}</p>
      </div>
    );
  }

  if (!item) {
    return (
      <div style={{ padding: "2rem", textAlign: "center" }}>
        <p>Loading…</p>
      </div>
    );
  }

  const icon = kindIcons[item.kind] || "📦";

  return (
    <div style={{ padding: "1rem" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", marginBottom: "1rem" }}>
        <button onClick={onBack} style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}>
          ← Back
        </button>
        <span style={{ fontSize: "1.2rem" }}>{icon}</span>
        <h2 style={{ fontSize: "1rem", fontWeight: 600, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {item.name}
        </h2>
      </div>

      {/* Fields based on kind */}
      <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
        {item.kind === "login" && (
          <>
            {item.username && (
              <DetailField label="Username" value={item.username} onCopy={() => copyField(item.username!, "Username")} />
            )}
            {item.password && (
              <DetailField
                label="Password"
                value={showPassword ? item.password : "••••••••"}
                onCopy={() => copyField(item.password!, "Password")}
                onToggle={() => setShowPassword(!showPassword)}
                toggleLabel={showPassword ? "Hide" : "Show"}
              />
            )}
            {item.totp_secret && totpCode && (
              <DetailField
                label={`TOTP (${totpCountdown}s)`}
                value={totpCode}
                onCopy={() => copyField(totpCode!, "TOTP code")}
              />
            )}
            {item.uris && item.uris.length > 0 && (
              <div>
                <div style={{ fontSize: "0.75rem", color: "#aaa", marginBottom: "0.2rem" }}>URIs</div>
                {item.uris.map((u, i) => (
                  <div key={i} style={{ fontSize: "0.85rem", color: "#ccc", marginBottom: "0.2rem" }}>
                    {u.uri}
                  </div>
                ))}
              </div>
            )}
          </>
        )}

        {item.kind === "secure_note" && (
          <div>
            <div style={{ fontSize: "0.75rem", color: "#aaa", marginBottom: "0.2rem" }}>Note</div>
            <div style={{ fontSize: "0.85rem", color: "#ccc", whiteSpace: "pre-wrap", background: "#0d1b2a", padding: "0.5rem", borderRadius: "4px" }}>
              {item.note || "—"}
            </div>
          </div>
        )}

        {item.kind === "card" && (
          <>
            {item.cardholder_name && <DetailField label="Cardholder" value={item.cardholder_name} onCopy={() => copyField(item.cardholder_name!, "Cardholder")} />}
            {item.card_number && (
              <DetailField
                label="Card Number"
                value={showPassword ? item.card_number : "•••• •••• •••• " + (item.card_number.slice(-4) || "••••")}
                onCopy={() => copyField(item.card_number!, "Card number")}
                onToggle={() => setShowPassword(!showPassword)}
                toggleLabel={showPassword ? "Hide" : "Show"}
              />
            )}
            {item.expiry_date && <DetailField label="Expiry" value={item.expiry_date} onCopy={() => copyField(item.expiry_date!, "Expiry")} />}
            {item.cvv && (
              <DetailField
                label="CVV"
                value={showCvv ? item.cvv : "•••"}
                onCopy={() => copyField(item.cvv!, "CVV")}
                onToggle={() => setShowCvv(!showCvv)}
                toggleLabel={showCvv ? "Hide" : "Show"}
              />
            )}
          </>
        )}

        {item.kind === "identity" && (
          <>
            {item.first_name && <DetailField label="First Name" value={item.first_name} onCopy={() => copyField(item.first_name!, "First name")} />}
            {item.last_name && <DetailField label="Last Name" value={item.last_name} onCopy={() => copyField(item.last_name!, "Last name")} />}
            {item.email && <DetailField label="Email" value={item.email} onCopy={() => copyField(item.email!, "Email")} />}
            {item.phone && <DetailField label="Phone" value={item.phone} onCopy={() => copyField(item.phone!, "Phone")} />}
            {item.address && <DetailField label="Address" value={item.address} onCopy={() => copyField(item.address!, "Address")} />}
            {item.city && <DetailField label="City" value={item.city} onCopy={() => copyField(item.city!, "City")} />}
            {item.country && <DetailField label="Country" value={item.country} onCopy={() => copyField(item.country!, "Country")} />}
          </>
        )}
      </div>
    </div>
  );
}

function DetailField({
  label,
  value,
  onCopy,
  onToggle,
  toggleLabel,
}: {
  label: string;
  value: string;
  onCopy?: () => void;
  onToggle?: () => void;
  toggleLabel?: string;
}) {
  return (
    <div>
      <div style={{ fontSize: "0.75rem", color: "#aaa", marginBottom: "0.2rem" }}>{label}</div>
      <div style={{ display: "flex", alignItems: "center", gap: "0.4rem" }}>
        <span style={{ fontSize: "0.85rem", color: "#e0e0e0", flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontFamily: label === "Password" || label.startsWith("TOTP") ? "monospace" : "inherit" }}>
          {value}
        </span>
        {onToggle && (
          <button
            onClick={onToggle}
            style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#aaa", cursor: "pointer", fontSize: "0.7rem", flexShrink: 0 }}
          >
            {toggleLabel}
          </button>
        )}
        {onCopy && (
          <button
            onClick={onCopy}
            title="Copy"
            style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#aaa", cursor: "pointer", fontSize: "0.7rem", flexShrink: 0 }}
          >
            📋
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Devices View ───────────────────────────────────────────────────────────

function DevicesView({
  onBack,
  showToast,
}: {
  onBack: () => void;
  showToast: (msg: string) => void;
}) {
  const [devices, setDevices] = useState<DeviceEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAdmit, setShowAdmit] = useState(false);
  const [admitPubkey, setAdmitPubkey] = useState("");
  const [admitLabel, setAdmitLabel] = useState("");
  const [admitError, setAdmitError] = useState<string | null>(null);
  const [confirmRevoke, setConfirmRevoke] = useState<string | null>(null);

  useEffect(() => {
    loadDevices();
  }, []);

  async function loadDevices() {
    setLoading(true);
    const response = (await browser.runtime.sendMessage({ type: "LIST_DEVICES" })) as MessageResponse;
    if (response.devices) {
      setDevices(response.devices);
    }
    setLoading(false);
  }

  async function handleAdmit(e: React.FormEvent) {
    e.preventDefault();
    setAdmitError(null);

    if (!/^[0-9a-f]{64}$/i.test(admitPubkey.trim())) {
      setAdmitError("Public key must be 64 hex characters");
      return;
    }
    if (!admitLabel.trim()) {
      setAdmitError("Device label is required");
      return;
    }

    const response = (await browser.runtime.sendMessage({
      type: "ADMIT_DEVICE",
      payload: { pubkeyHex: admitPubkey.trim(), label: admitLabel.trim() },
    })) as MessageResponse;

    if (response.error) {
      setAdmitError(response.error);
    } else {
      showToast("Device admitted!");
      setShowAdmit(false);
      setAdmitPubkey("");
      setAdmitLabel("");
      await loadDevices();
    }
  }

  async function handleRevoke(deviceId: string) {
    const response = (await browser.runtime.sendMessage({
      type: "REVOKE_DEVICE",
      payload: { deviceId },
    })) as MessageResponse;

    if (response.error) {
      showToast("Error: " + response.error);
    } else {
      showToast("Device revoked");
      setConfirmRevoke(null);
      await loadDevices();
    }
  }

  const inputStyle: React.CSSProperties = {
    width: "100%",
    padding: "0.5rem",
    borderRadius: "4px",
    border: "1px solid #444",
    background: "#16213e",
    color: "#e0e0e0",
    fontSize: "0.85rem",
    marginBottom: "0.5rem",
  };

  return (
    <div style={{ padding: "1rem" }}>
      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: "0.5rem", marginBottom: "1rem" }}>
        <button onClick={onBack} style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}>
          ← Back
        </button>
        <h2 style={{ fontSize: "1rem", fontWeight: 600 }}>📱 Devices</h2>
      </div>

      {/* Admit button */}
      {!showAdmit && (
        <button
          onClick={() => setShowAdmit(true)}
          style={{ width: "100%", padding: "0.5rem", borderRadius: "4px", border: "none", background: "#0f3460", color: "#e0e0e0", fontSize: "0.85rem", cursor: "pointer", marginBottom: "1rem" }}
        >
          + Admit Device
        </button>
      )}

      {/* Admit form */}
      {showAdmit && (
        <form onSubmit={handleAdmit} style={{ marginBottom: "1rem", padding: "0.75rem", background: "#0d1b2a", borderRadius: "4px" }}>
          <p style={{ fontSize: "0.75rem", color: "#aaa", marginBottom: "0.5rem" }}>
            To sync vaults between devices, both devices must admit each other. Share your public key with the other device, and enter their public key below.
          </p>
          <input
            type="text"
            value={admitPubkey}
            onChange={(e) => setAdmitPubkey(e.target.value)}
            placeholder="Public key (64 hex characters)"
            style={inputStyle}
          />
          <input
            type="text"
            value={admitLabel}
            onChange={(e) => setAdmitLabel(e.target.value)}
            placeholder="Device label (e.g. Bob's Phone)"
            style={inputStyle}
          />
          {admitError && (
            <p style={{ color: "#ff6b6b", fontSize: "0.8rem", marginBottom: "0.5rem" }} role="alert">{admitError}</p>
          )}
          <div style={{ display: "flex", gap: "0.4rem" }}>
            <button type="submit" style={{ flex: 1, padding: "0.4rem", borderRadius: "4px", border: "none", background: "#0f3460", color: "#e0e0e0", fontSize: "0.85rem", cursor: "pointer" }}>
              Admit
            </button>
            <button type="button" onClick={() => { setShowAdmit(false); setAdmitError(null); }} style={{ flex: 1, padding: "0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", fontSize: "0.85rem", cursor: "pointer" }}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {/* Device list */}
      {loading ? (
        <p style={{ color: "#888", textAlign: "center" }}>Loading…</p>
      ) : devices.length === 0 ? (
        <p style={{ color: "#888", textAlign: "center", padding: "1rem 0" }}>No devices in trust group yet.</p>
      ) : (
        <ul style={{ listStyle: "none" }}>
          {devices.map((device) => (
            <li
              key={device.device_id}
              style={{ padding: "0.6rem", borderBottom: "1px solid #2a2a4a", display: "flex", justifyContent: "space-between", alignItems: "center" }}
            >
              <div style={{ minWidth: 0 }}>
                <div style={{ fontWeight: 500, fontSize: "0.9rem", display: "flex", alignItems: "center", gap: "0.3rem" }}>
                  {device.label}
                  {device.revoked && <span style={{ fontSize: "0.7rem", color: "#ff6b6b", background: "#3d1515", padding: "0.1rem 0.3rem", borderRadius: "3px" }}>revoked</span>}
                </div>
                <div style={{ fontSize: "0.75rem", color: "#888", fontFamily: "monospace" }}>
                  {device.nostr_pubkey.slice(0, 16)}…
                </div>
              </div>
              {!device.revoked && (
                confirmRevoke === device.device_id ? (
                  <div style={{ display: "flex", gap: "0.3rem" }}>
                    <button
                      onClick={() => handleRevoke(device.device_id)}
                      style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "none", background: "#ff6b6b", color: "#fff", cursor: "pointer", fontSize: "0.7rem" }}
                    >
                      Confirm
                    </button>
                    <button
                      onClick={() => setConfirmRevoke(null)}
                      style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.7rem" }}
                    >
                      Cancel
                    </button>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirmRevoke(device.device_id)}
                    style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#ff6b6b", cursor: "pointer", fontSize: "0.7rem" }}
                  >
                    Revoke
                  </button>
                )
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ─── Item Create View ───────────────────────────────────────────────────────

function ItemCreateView({
  onSave,
  onCancel,
}: {
  onSave: () => void;
  onCancel: () => void;
}) {
  const [kind, setKind] = useState<ItemKind>("login");
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [nameError, setNameError] = useState<string | null>(null);

  // Login fields
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [totpSecret, setTotpSecret] = useState("");
  const [uris, setUris] = useState<UriEntry[]>([{ uri: "", match_strategy: "Domain" }]);

  // Secure Note fields
  const [note, setNote] = useState("");

  // Card fields
  const [cardholderName, setCardholderName] = useState("");
  const [cardNumber, setCardNumber] = useState("");
  const [expiryDate, setExpiryDate] = useState("");
  const [cvv, setCvv] = useState("");

  // Identity fields
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [address, setAddress] = useState("");
  const [city, setCity] = useState("");
  const [country, setCountry] = useState("");
  const [phone, setPhone] = useState("");
  const [email, setEmail] = useState("");

  const [genError, setGenError] = useState<string | null>(null);

  useEffect(() => {
    async function autoFillUri() {
      try {
        const tabs = await browser.tabs.query({ active: true, currentWindow: true });
        const tab = tabs[0];
        if (tab?.url && tab.url.startsWith("https://")) {
          setUris([{ uri: tab.url, match_strategy: "Domain" }]);
        }
      } catch {
        // ignore
      }
    }
    autoFillUri();
  }, []);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") onCancel();
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onCancel]);

  async function handleGeneratePassword() {
    setGenError(null);
    try {
      const response = (await browser.runtime.sendMessage({ type: "GENERATE_PASSWORD" })) as MessageResponse;
      if (response.error) {
        setGenError(response.error);
      } else if (response.password) {
        setPassword(response.password);
      }
    } catch (err) {
      setGenError(String(err));
    }
  }

  function addUri() {
    setUris([...uris, { uri: "", match_strategy: "Domain" }]);
  }

  function removeUri(index: number) {
    if (uris.length <= 1) return;
    setUris(uris.filter((_, i) => i !== index));
  }

  function updateUri(index: number, field: keyof UriEntry, value: string) {
    const updated = [...uris];
    updated[index] = { ...updated[index], [field]: value };
    setUris(updated);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setNameError(null);
    setSaveError(null);

    const trimmedName = name.trim();
    if (!trimmedName) {
      setNameError("Item name is required");
      return;
    }

    setSaving(true);

    try {
      const payload: Record<string, unknown> = { kind, name: trimmedName };

      if (kind === "login") {
        if (username) payload.username = username;
        if (password) payload.password = password;
        if (totpSecret) payload.totp_secret = totpSecret;
        const validUris = uris.filter((u) => u.uri.trim());
        if (validUris.length > 0) {
          payload.uris = validUris.map((u) => ({ uri: u.uri.trim(), match_strategy: u.match_strategy }));
        }
      } else if (kind === "secure_note") {
        if (note) payload.note = note;
      } else if (kind === "card") {
        if (cardholderName) payload.cardholder_name = cardholderName;
        if (cardNumber) payload.card_number = cardNumber;
        if (expiryDate) payload.expiry_date = expiryDate;
        if (cvv) payload.cvv = cvv;
      } else if (kind === "identity") {
        if (firstName) payload.first_name = firstName;
        if (lastName) payload.last_name = lastName;
        if (address) payload.address = address;
        if (city) payload.city = city;
        if (country) payload.country = country;
        if (phone) payload.phone = phone;
        if (email) payload.email = email;
      }

      const response = (await browser.runtime.sendMessage({ type: "ADD_ITEM", payload })) as MessageResponse;
      if (response.error) {
        setSaveError(response.error);
      } else {
        onSave();
      }
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  }

  const inputStyle: React.CSSProperties = {
    width: "100%",
    padding: "0.5rem",
    borderRadius: "4px",
    border: "1px solid #444",
    background: "#16213e",
    color: "#e0e0e0",
    fontSize: "0.9rem",
    marginBottom: "0.6rem",
  };

  const labelStyle: React.CSSProperties = {
    display: "block",
    marginBottom: "0.2rem",
    fontSize: "0.8rem",
    color: "#aaa",
  };

  return (
    <div style={{ padding: "1rem", height: "100%", display: "flex", flexDirection: "column" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.75rem", flexShrink: 0 }}>
        <h1 style={{ fontSize: "1.1rem" }}>Add Item</h1>
        <button onClick={onCancel} style={{ padding: "0.3rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem" }}>
          Cancel
        </button>
      </div>

      <form onSubmit={handleSubmit} style={{ flex: 1, overflowY: "auto", display: "flex", flexDirection: "column" }}>
        {saveError && (
          <div role="alert" style={{ padding: "0.5rem", marginBottom: "0.75rem", background: "#3d1515", borderRadius: "4px", color: "#ff6b6b", fontSize: "0.85rem" }}>
            {saveError}
          </div>
        )}

        <label htmlFor="item-type" style={labelStyle}>Type</label>
        <select id="item-type" value={kind} onChange={(e) => setKind(e.target.value as ItemKind)} style={{ ...inputStyle, cursor: "pointer" }}>
          <option value="login">Login</option>
          <option value="secure_note">Secure Note</option>
          <option value="card">Card</option>
          <option value="identity">Identity</option>
        </select>

        <label htmlFor="item-name" style={labelStyle}>Name *</label>
        <input
          id="item-name"
          type="text"
          value={name}
          onChange={(e) => { setName(e.target.value); if (nameError) setNameError(null); }}
          placeholder="Item name"
          autoFocus
          style={{ ...inputStyle, border: nameError ? "1px solid #ff6b6b" : "1px solid #444" }}
        />
        {nameError && <p role="alert" style={{ color: "#ff6b6b", fontSize: "0.8rem", marginTop: "-0.4rem", marginBottom: "0.5rem" }}>{nameError}</p>}

        {kind === "login" && (
          <>
            <label htmlFor="login-username" style={labelStyle}>Username</label>
            <input id="login-username" type="text" value={username} onChange={(e) => setUsername(e.target.value)} placeholder="Username or email" style={inputStyle} />

            <label htmlFor="login-password" style={labelStyle}>Password</label>
            <div style={{ display: "flex", gap: "0.4rem", marginBottom: "0.6rem" }}>
              <input id="login-password" type="text" value={password} onChange={(e) => setPassword(e.target.value)} placeholder="Password" style={{ ...inputStyle, marginBottom: 0, flex: 1 }} />
              <button type="button" onClick={handleGeneratePassword} title="Generate password" style={{ padding: "0.5rem 0.6rem", borderRadius: "4px", border: "1px solid #444", background: "#0f3460", color: "#e0e0e0", cursor: "pointer", fontSize: "0.8rem", whiteSpace: "nowrap" }}>
                Generate
              </button>
            </div>
            {genError && <p role="alert" style={{ color: "#ff6b6b", fontSize: "0.8rem", marginTop: "-0.4rem", marginBottom: "0.5rem" }}>{genError}</p>}

            <label htmlFor="login-totp" style={labelStyle}>TOTP Secret</label>
            <input id="login-totp" type="text" value={totpSecret} onChange={(e) => setTotpSecret(e.target.value)} placeholder="Base32 TOTP secret (optional)" style={inputStyle} />

            <div style={{ marginBottom: "0.5rem" }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "0.3rem" }}>
                <span style={{ fontSize: "0.8rem", color: "#aaa" }}>URIs</span>
                <button type="button" onClick={addUri} style={{ padding: "0.2rem 0.4rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#e0e0e0", cursor: "pointer", fontSize: "0.75rem" }}>
                  + Add URI
                </button>
              </div>
              {uris.map((entry, index) => (
                <div key={index} style={{ display: "flex", gap: "0.3rem", marginBottom: "0.4rem", alignItems: "center" }}>
                  <input type="text" value={entry.uri} onChange={(e) => updateUri(index, "uri", e.target.value)} placeholder="https://example.com" style={{ ...inputStyle, marginBottom: 0, flex: 1 }} aria-label={`URI ${index + 1}`} />
                  <select value={entry.match_strategy} onChange={(e) => updateUri(index, "match_strategy", e.target.value)} style={{ ...inputStyle, marginBottom: 0, width: "auto", minWidth: "5rem", cursor: "pointer" }} aria-label={`Match strategy for URI ${index + 1}`}>
                    <option value="Domain">Domain</option>
                    <option value="Host">Host</option>
                    <option value="StartsWith">StartsWith</option>
                    <option value="Exact">Exact</option>
                    <option value="Regex">Regex</option>
                    <option value="Never">Never</option>
                  </select>
                  {uris.length > 1 && (
                    <button type="button" onClick={() => removeUri(index)} aria-label={`Remove URI ${index + 1}`} style={{ padding: "0.3rem 0.5rem", borderRadius: "4px", border: "1px solid #444", background: "transparent", color: "#ff6b6b", cursor: "pointer", fontSize: "0.75rem" }}>
                      ✕
                    </button>
                  )}
                </div>
              ))}
            </div>
          </>
        )}

        {kind === "secure_note" && (
          <>
            <label htmlFor="note-content" style={labelStyle}>Note</label>
            <textarea id="note-content" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Secure note content" rows={4} style={{ ...inputStyle, resize: "vertical", minHeight: "6rem" }} />
          </>
        )}

        {kind === "card" && (
          <>
            <label htmlFor="card-holder" style={labelStyle}>Cardholder Name</label>
            <input id="card-holder" type="text" value={cardholderName} onChange={(e) => setCardholderName(e.target.value)} placeholder="Name on card" style={inputStyle} />
            <label htmlFor="card-number" style={labelStyle}>Card Number</label>
            <input id="card-number" type="text" value={cardNumber} onChange={(e) => setCardNumber(e.target.value)} placeholder="Card number" style={inputStyle} />
            <label htmlFor="card-expiry" style={labelStyle}>Expiry Date</label>
            <input id="card-expiry" type="text" value={expiryDate} onChange={(e) => setExpiryDate(e.target.value)} placeholder="MM/YY" style={inputStyle} />
            <label htmlFor="card-cvv" style={labelStyle}>CVV</label>
            <input id="card-cvv" type="password" value={cvv} onChange={(e) => setCvv(e.target.value)} placeholder="CVV" style={inputStyle} />
          </>
        )}

        {kind === "identity" && (
          <>
            <label htmlFor="identity-first-name" style={labelStyle}>First Name</label>
            <input id="identity-first-name" type="text" value={firstName} onChange={(e) => setFirstName(e.target.value)} placeholder="First name" style={inputStyle} />
            <label htmlFor="identity-last-name" style={labelStyle}>Last Name</label>
            <input id="identity-last-name" type="text" value={lastName} onChange={(e) => setLastName(e.target.value)} placeholder="Last name" style={inputStyle} />
            <label htmlFor="identity-address" style={labelStyle}>Address</label>
            <input id="identity-address" type="text" value={address} onChange={(e) => setAddress(e.target.value)} placeholder="Street address" style={inputStyle} />
            <label htmlFor="identity-city" style={labelStyle}>City</label>
            <input id="identity-city" type="text" value={city} onChange={(e) => setCity(e.target.value)} placeholder="City" style={inputStyle} />
            <label htmlFor="identity-country" style={labelStyle}>Country</label>
            <input id="identity-country" type="text" value={country} onChange={(e) => setCountry(e.target.value)} placeholder="Country" style={inputStyle} />
            <label htmlFor="identity-phone" style={labelStyle}>Phone</label>
            <input id="identity-phone" type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} placeholder="Phone number" style={inputStyle} />
            <label htmlFor="identity-email" style={labelStyle}>Email</label>
            <input id="identity-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="Email address" style={inputStyle} />
          </>
        )}

        <button type="submit" disabled={saving} style={{ width: "100%", padding: "0.6rem", borderRadius: "4px", border: "none", background: "#0f3460", color: "#e0e0e0", fontSize: "0.9rem", cursor: saving ? "wait" : "pointer", marginTop: "0.75rem", flexShrink: 0 }}>
          {saving ? "Saving…" : "Save Item"}
        </button>
      </form>
    </div>
  );
}
