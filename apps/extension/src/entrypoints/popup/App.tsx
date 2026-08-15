import React, { useEffect, useState } from "react";

// ─── Types ──────────────────────────────────────────────────────────────────

interface VaultItem {
  id: string;
  kind: string;
  name: string;
  username?: string;
  uris?: { uri: string }[];
}

type View = "loading" | "create" | "unlock" | "items" | "create-item";

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
  data?: number[];
  password?: string;
  code?: string;
}

// ─── App ────────────────────────────────────────────────────────────────────

export function App() {
  const [view, setView] = useState<View>("loading");
  const [items, setItems] = useState<VaultItem[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    checkStatus();
  }, []);

  async function checkStatus() {
    const response = (await browser.runtime.sendMessage({ type: "GET_STATUS" })) as MessageResponse;
    if (response.unlocked) {
      await loadItems();
    } else {
      // Check if a vault exists in storage
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
      // No vault — shouldn't happen from unlock view, but redirect to create
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
      <h1 style={{ fontSize: "1.25rem", marginBottom: "0.5rem" }}>
        🔐 ZVault
      </h1>
      <h2 style={{ fontSize: "1rem", marginBottom: "1rem", color: "#aaa" }}>
        Create New Vault
      </h2>
      <form onSubmit={handleSubmit}>
        <label
          htmlFor="create-password"
          style={{ display: "block", marginBottom: "0.25rem", fontSize: "0.9rem" }}
        >
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
        <label
          htmlFor="confirm-password"
          style={{ display: "block", marginBottom: "0.25rem", fontSize: "0.9rem" }}
        >
          Confirm Password
        </label>
        <input
          id="confirm-password"
          type="password"
          value={confirmPassword}
          onChange={(e) => setConfirmPassword(e.target.value)}
          placeholder="Re-enter master password"
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
        {displayError && (
          <p
            style={{ color: "#ff6b6b", fontSize: "0.85rem", marginBottom: "0.5rem" }}
            role="alert"
          >
            {displayError}
          </p>
        )}
        <button
          type="submit"
          disabled={loading || !password.trim() || !confirmPassword.trim()}
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
  onAdd,
}: {
  items: VaultItem[];
  onLock: () => void;
  onCopyPassword: (id: string) => void;
  onAdd: () => void;
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
        <div style={{ display: "flex", gap: "0.4rem" }}>
          <button
            onClick={onAdd}
            aria-label="Add item"
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
            +
          </button>
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
      </div>

      {items.length === 0 ? (
        <p style={{ color: "#888", textAlign: "center", padding: "2rem 0" }}>
          No items yet. Click + to add credentials.
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
  const [uris, setUris] = useState<UriEntry[]>([
    { uri: "", match_strategy: "Domain" },
  ]);

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

  // Password generation error
  const [genError, setGenError] = useState<string | null>(null);

  // Auto-fill first URI from current tab on mount
  useEffect(() => {
    async function autoFillUri() {
      try {
        const tabs = await browser.tabs.query({
          active: true,
          currentWindow: true,
        });
        const tab = tabs[0];
        if (tab?.url && tab.url.startsWith("https://")) {
          setUris([{ uri: tab.url, match_strategy: "Domain" }]);
        }
      } catch {
        // Silently ignore — tab query may fail in some contexts
      }
    }
    autoFillUri();
  }, []);

  // Handle Escape key to cancel
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onCancel]);

  async function handleGeneratePassword() {
    setGenError(null);
    try {
      const response = (await browser.runtime.sendMessage({
        type: "GENERATE_PASSWORD",
      })) as MessageResponse;
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
      // Build the payload based on kind
      const payload: Record<string, unknown> = {
        kind,
        name: trimmedName,
      };

      if (kind === "login") {
        if (username) payload.username = username;
        if (password) payload.password = password;
        if (totpSecret) payload.totp_secret = totpSecret;
        const validUris = uris.filter((u) => u.uri.trim());
        if (validUris.length > 0) {
          payload.uris = validUris.map((u) => ({
            uri: u.uri.trim(),
            match_strategy: u.match_strategy,
          }));
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

      const response = (await browser.runtime.sendMessage({
        type: "ADD_ITEM",
        payload,
      })) as MessageResponse;

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
    <div
      style={{
        padding: "1rem",
        height: "100%",
        display: "flex",
        flexDirection: "column",
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: "0.75rem",
          flexShrink: 0,
        }}
      >
        <h1 style={{ fontSize: "1.1rem" }}>Add Item</h1>
        <button
          onClick={onCancel}
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
          Cancel
        </button>
      </div>

      {/* Scrollable form */}
      <form
        onSubmit={handleSubmit}
        style={{
          flex: 1,
          overflowY: "auto",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Error banner */}
        {saveError && (
          <div
            role="alert"
            style={{
              padding: "0.5rem",
              marginBottom: "0.75rem",
              background: "#3d1515",
              borderRadius: "4px",
              color: "#ff6b6b",
              fontSize: "0.85rem",
            }}
          >
            {saveError}
          </div>
        )}

        {/* Item type selector */}
        <label htmlFor="item-type" style={labelStyle}>
          Type
        </label>
        <select
          id="item-type"
          value={kind}
          onChange={(e) => setKind(e.target.value as ItemKind)}
          style={{
            ...inputStyle,
            cursor: "pointer",
          }}
        >
          <option value="login">Login</option>
          <option value="secure_note">Secure Note</option>
          <option value="card">Card</option>
          <option value="identity">Identity</option>
        </select>

        {/* Name field (shared across all types) */}
        <label htmlFor="item-name" style={labelStyle}>
          Name *
        </label>
        <input
          id="item-name"
          type="text"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (nameError) setNameError(null);
          }}
          placeholder="Item name"
          autoFocus
          style={{
            ...inputStyle,
            border: nameError ? "1px solid #ff6b6b" : "1px solid #444",
          }}
        />
        {nameError && (
          <p
            role="alert"
            style={{
              color: "#ff6b6b",
              fontSize: "0.8rem",
              marginTop: "-0.4rem",
              marginBottom: "0.5rem",
            }}
          >
            {nameError}
          </p>
        )}

        {/* Type-specific fields */}
        {kind === "login" && (
          <LoginFields
            username={username}
            setUsername={setUsername}
            password={password}
            setPassword={setPassword}
            totpSecret={totpSecret}
            setTotpSecret={setTotpSecret}
            uris={uris}
            onAddUri={addUri}
            onRemoveUri={removeUri}
            onUpdateUri={updateUri}
            onGeneratePassword={handleGeneratePassword}
            genError={genError}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        )}

        {kind === "secure_note" && (
          <SecureNoteFields
            note={note}
            setNote={setNote}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        )}

        {kind === "card" && (
          <CardFields
            cardholderName={cardholderName}
            setCardholderName={setCardholderName}
            cardNumber={cardNumber}
            setCardNumber={setCardNumber}
            expiryDate={expiryDate}
            setExpiryDate={setExpiryDate}
            cvv={cvv}
            setCvv={setCvv}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        )}

        {kind === "identity" && (
          <IdentityFields
            firstName={firstName}
            setFirstName={setFirstName}
            lastName={lastName}
            setLastName={setLastName}
            address={address}
            setAddress={setAddress}
            city={city}
            setCity={setCity}
            country={country}
            setCountry={setCountry}
            phone={phone}
            setPhone={setPhone}
            email={email}
            setEmail={setEmail}
            inputStyle={inputStyle}
            labelStyle={labelStyle}
          />
        )}

        {/* Save button */}
        <button
          type="submit"
          disabled={saving}
          style={{
            width: "100%",
            padding: "0.6rem",
            borderRadius: "4px",
            border: "none",
            background: "#0f3460",
            color: "#e0e0e0",
            fontSize: "0.9rem",
            cursor: saving ? "wait" : "pointer",
            marginTop: "0.75rem",
            flexShrink: 0,
          }}
        >
          {saving ? "Saving…" : "Save Item"}
        </button>
      </form>
    </div>
  );
}

// ─── Login Fields ───────────────────────────────────────────────────────────

function LoginFields({
  username,
  setUsername,
  password,
  setPassword,
  totpSecret,
  setTotpSecret,
  uris,
  onAddUri,
  onRemoveUri,
  onUpdateUri,
  onGeneratePassword,
  genError,
  inputStyle,
  labelStyle,
}: {
  username: string;
  setUsername: (v: string) => void;
  password: string;
  setPassword: (v: string) => void;
  totpSecret: string;
  setTotpSecret: (v: string) => void;
  uris: UriEntry[];
  onAddUri: () => void;
  onRemoveUri: (index: number) => void;
  onUpdateUri: (index: number, field: keyof UriEntry, value: string) => void;
  onGeneratePassword: () => void;
  genError: string | null;
  inputStyle: React.CSSProperties;
  labelStyle: React.CSSProperties;
}) {
  return (
    <>
      <label htmlFor="login-username" style={labelStyle}>
        Username
      </label>
      <input
        id="login-username"
        type="text"
        value={username}
        onChange={(e) => setUsername(e.target.value)}
        placeholder="Username or email"
        style={inputStyle}
      />

      <label htmlFor="login-password" style={labelStyle}>
        Password
      </label>
      <div style={{ display: "flex", gap: "0.4rem", marginBottom: "0.6rem" }}>
        <input
          id="login-password"
          type="text"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="Password"
          style={{ ...inputStyle, marginBottom: 0, flex: 1 }}
        />
        <button
          type="button"
          onClick={onGeneratePassword}
          aria-label="Generate password"
          title="Generate password"
          style={{
            padding: "0.5rem 0.6rem",
            borderRadius: "4px",
            border: "1px solid #444",
            background: "#0f3460",
            color: "#e0e0e0",
            cursor: "pointer",
            fontSize: "0.8rem",
            whiteSpace: "nowrap",
          }}
        >
          Generate
        </button>
      </div>
      {genError && (
        <p
          role="alert"
          style={{
            color: "#ff6b6b",
            fontSize: "0.8rem",
            marginTop: "-0.4rem",
            marginBottom: "0.5rem",
          }}
        >
          {genError}
        </p>
      )}

      <label htmlFor="login-totp" style={labelStyle}>
        TOTP Secret
      </label>
      <input
        id="login-totp"
        type="text"
        value={totpSecret}
        onChange={(e) => setTotpSecret(e.target.value)}
        placeholder="Base32 TOTP secret (optional)"
        style={inputStyle}
      />

      {/* URIs */}
      <div style={{ marginBottom: "0.5rem" }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            marginBottom: "0.3rem",
          }}
        >
          <span style={{ fontSize: "0.8rem", color: "#aaa" }}>URIs</span>
          <button
            type="button"
            onClick={onAddUri}
            aria-label="Add URI"
            style={{
              padding: "0.2rem 0.4rem",
              borderRadius: "4px",
              border: "1px solid #444",
              background: "transparent",
              color: "#e0e0e0",
              cursor: "pointer",
              fontSize: "0.75rem",
            }}
          >
            + Add URI
          </button>
        </div>
        {uris.map((entry, index) => (
          <div
            key={index}
            style={{
              display: "flex",
              gap: "0.3rem",
              marginBottom: "0.4rem",
              alignItems: "center",
            }}
          >
            <label htmlFor={`uri-${index}`} style={{ display: "none" }}>
              URI {index + 1}
            </label>
            <input
              id={`uri-${index}`}
              type="text"
              value={entry.uri}
              onChange={(e) => onUpdateUri(index, "uri", e.target.value)}
              placeholder="https://example.com"
              style={{ ...inputStyle, marginBottom: 0, flex: 1 }}
            />
            <label htmlFor={`uri-match-${index}`} style={{ display: "none" }}>
              Match strategy for URI {index + 1}
            </label>
            <select
              id={`uri-match-${index}`}
              value={entry.match_strategy}
              onChange={(e) =>
                onUpdateUri(index, "match_strategy", e.target.value)
              }
              style={{
                ...inputStyle,
                marginBottom: 0,
                width: "auto",
                minWidth: "5rem",
                cursor: "pointer",
              }}
            >
              <option value="Domain">Domain</option>
              <option value="Host">Host</option>
              <option value="StartsWith">StartsWith</option>
              <option value="Exact">Exact</option>
              <option value="Regex">Regex</option>
              <option value="Never">Never</option>
            </select>
            {uris.length > 1 && (
              <button
                type="button"
                onClick={() => onRemoveUri(index)}
                aria-label={`Remove URI ${index + 1}`}
                style={{
                  padding: "0.3rem 0.5rem",
                  borderRadius: "4px",
                  border: "1px solid #444",
                  background: "transparent",
                  color: "#ff6b6b",
                  cursor: "pointer",
                  fontSize: "0.75rem",
                }}
              >
                ✕
              </button>
            )}
          </div>
        ))}
      </div>
    </>
  );
}

// ─── Secure Note Fields ─────────────────────────────────────────────────────

function SecureNoteFields({
  note,
  setNote,
  inputStyle,
  labelStyle,
}: {
  note: string;
  setNote: (v: string) => void;
  inputStyle: React.CSSProperties;
  labelStyle: React.CSSProperties;
}) {
  return (
    <>
      <label htmlFor="note-content" style={labelStyle}>
        Note
      </label>
      <textarea
        id="note-content"
        value={note}
        onChange={(e) => setNote(e.target.value)}
        placeholder="Secure note content"
        rows={4}
        style={{
          ...inputStyle,
          resize: "vertical",
          minHeight: "6rem",
        }}
      />
    </>
  );
}

// ─── Card Fields ────────────────────────────────────────────────────────────

function CardFields({
  cardholderName,
  setCardholderName,
  cardNumber,
  setCardNumber,
  expiryDate,
  setExpiryDate,
  cvv,
  setCvv,
  inputStyle,
  labelStyle,
}: {
  cardholderName: string;
  setCardholderName: (v: string) => void;
  cardNumber: string;
  setCardNumber: (v: string) => void;
  expiryDate: string;
  setExpiryDate: (v: string) => void;
  cvv: string;
  setCvv: (v: string) => void;
  inputStyle: React.CSSProperties;
  labelStyle: React.CSSProperties;
}) {
  return (
    <>
      <label htmlFor="card-holder" style={labelStyle}>
        Cardholder Name
      </label>
      <input
        id="card-holder"
        type="text"
        value={cardholderName}
        onChange={(e) => setCardholderName(e.target.value)}
        placeholder="Name on card"
        style={inputStyle}
      />

      <label htmlFor="card-number" style={labelStyle}>
        Card Number
      </label>
      <input
        id="card-number"
        type="text"
        value={cardNumber}
        onChange={(e) => setCardNumber(e.target.value)}
        placeholder="Card number"
        style={inputStyle}
      />

      <label htmlFor="card-expiry" style={labelStyle}>
        Expiry Date
      </label>
      <input
        id="card-expiry"
        type="text"
        value={expiryDate}
        onChange={(e) => setExpiryDate(e.target.value)}
        placeholder="MM/YY"
        style={inputStyle}
      />

      <label htmlFor="card-cvv" style={labelStyle}>
        CVV
      </label>
      <input
        id="card-cvv"
        type="password"
        value={cvv}
        onChange={(e) => setCvv(e.target.value)}
        placeholder="CVV"
        style={inputStyle}
      />
    </>
  );
}

// ─── Identity Fields ────────────────────────────────────────────────────────

function IdentityFields({
  firstName,
  setFirstName,
  lastName,
  setLastName,
  address,
  setAddress,
  city,
  setCity,
  country,
  setCountry,
  phone,
  setPhone,
  email,
  setEmail,
  inputStyle,
  labelStyle,
}: {
  firstName: string;
  setFirstName: (v: string) => void;
  lastName: string;
  setLastName: (v: string) => void;
  address: string;
  setAddress: (v: string) => void;
  city: string;
  setCity: (v: string) => void;
  country: string;
  setCountry: (v: string) => void;
  phone: string;
  setPhone: (v: string) => void;
  email: string;
  setEmail: (v: string) => void;
  inputStyle: React.CSSProperties;
  labelStyle: React.CSSProperties;
}) {
  return (
    <>
      <label htmlFor="identity-first-name" style={labelStyle}>
        First Name
      </label>
      <input
        id="identity-first-name"
        type="text"
        value={firstName}
        onChange={(e) => setFirstName(e.target.value)}
        placeholder="First name"
        style={inputStyle}
      />

      <label htmlFor="identity-last-name" style={labelStyle}>
        Last Name
      </label>
      <input
        id="identity-last-name"
        type="text"
        value={lastName}
        onChange={(e) => setLastName(e.target.value)}
        placeholder="Last name"
        style={inputStyle}
      />

      <label htmlFor="identity-address" style={labelStyle}>
        Address
      </label>
      <input
        id="identity-address"
        type="text"
        value={address}
        onChange={(e) => setAddress(e.target.value)}
        placeholder="Street address"
        style={inputStyle}
      />

      <label htmlFor="identity-city" style={labelStyle}>
        City
      </label>
      <input
        id="identity-city"
        type="text"
        value={city}
        onChange={(e) => setCity(e.target.value)}
        placeholder="City"
        style={inputStyle}
      />

      <label htmlFor="identity-country" style={labelStyle}>
        Country
      </label>
      <input
        id="identity-country"
        type="text"
        value={country}
        onChange={(e) => setCountry(e.target.value)}
        placeholder="Country"
        style={inputStyle}
      />

      <label htmlFor="identity-phone" style={labelStyle}>
        Phone
      </label>
      <input
        id="identity-phone"
        type="tel"
        value={phone}
        onChange={(e) => setPhone(e.target.value)}
        placeholder="Phone number"
        style={inputStyle}
      />

      <label htmlFor="identity-email" style={labelStyle}>
        Email
      </label>
      <input
        id="identity-email"
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="Email address"
        style={inputStyle}
      />
    </>
  );
}
