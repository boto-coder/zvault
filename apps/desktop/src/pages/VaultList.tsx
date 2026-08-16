import { useEffect, useState, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import ItemCard from "../components/ItemCard";
import SyncButton from "../components/SyncButton";

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
  onDevices: () => void;
}

type ItemKind = "login" | "secure_note" | "card" | "identity";

interface UriEntry {
  uri: string;
  match: string;
}

function VaultList({ onSelectItem, onLocked, onDevices }: Props) {
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [deleteConfirm, setDeleteConfirm] = useState<ItemSummary | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

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

  // Listen for keyboard shortcut events
  useEffect(() => {
    const handleOpenAdd = () => setShowAdd(true);
    const handleFocusSearch = () => searchRef.current?.focus();
    const handleEscape = () => {
      if (deleteConfirm) {
        setDeleteConfirm(null);
      } else if (showAdd) {
        setShowAdd(false);
      }
    };

    window.addEventListener("zvault:open-add", handleOpenAdd);
    window.addEventListener("zvault:focus-search", handleFocusSearch);
    window.addEventListener("zvault:escape", handleEscape);
    return () => {
      window.removeEventListener("zvault:open-add", handleOpenAdd);
      window.removeEventListener("zvault:focus-search", handleFocusSearch);
      window.removeEventListener("zvault:escape", handleEscape);
    };
  }, [showAdd, deleteConfirm]);

  const handleLock = async () => {
    onLocked();
  };

  const handleDeleteItem = async (id: string) => {
    try {
      await invoke("delete_item", { id });
      setDeleteConfirm(null);
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
              title="Add Item (Ctrl+N)"
            >
              + Add Item
            </button>
            <SyncButton onSyncComplete={loadItems} />
            <button
              type="button"
              onClick={onDevices}
              className="px-3 py-1.5 text-sm bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg transition-colors"
              title="Manage Devices"
            >
              Devices
            </button>
            <button
              type="button"
              onClick={handleLock}
              className="px-3 py-1.5 text-sm bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg transition-colors"
              title="Lock Vault (Ctrl+L)"
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
            ref={searchRef}
            type="search"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Search items… (Ctrl+F)"
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
                onDelete={() => setDeleteConfirm(item)}
              />
            ))}
          </div>
        )}
      </main>

      {/* Add Item Modal */}
      {showAdd && (
        <AddItemModal
          onSaved={() => {
            setShowAdd(false);
            loadItems();
          }}
          onClose={() => setShowAdd(false)}
        />
      )}

      {/* Delete Confirmation Modal */}
      {deleteConfirm && (
        <DeleteConfirmModal
          itemName={deleteConfirm.name}
          onConfirm={() => handleDeleteItem(deleteConfirm.id)}
          onCancel={() => setDeleteConfirm(null)}
        />
      )}
    </div>
  );
}

// ─── Delete Confirmation Modal ───────────────────────────────────────────────

interface DeleteConfirmModalProps {
  itemName: string;
  onConfirm: () => void;
  onCancel: () => void;
}

function DeleteConfirmModal({ itemName, onConfirm, onCancel }: DeleteConfirmModalProps) {
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-sm">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
          Delete Item
        </h2>
        <p className="text-sm text-gray-600 dark:text-gray-400 mb-6">
          Delete <strong>{itemName}</strong>? This action cannot be undone.
        </p>
        <div className="flex gap-3 justify-end">
          <button
            type="button"
            onClick={onCancel}
            className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}

// ─── Full Add Item Modal ─────────────────────────────────────────────────────

interface AddItemModalProps {
  onSaved: () => void;
  onClose: () => void;
}

function AddItemModal({ onSaved, onClose }: AddItemModalProps) {
  const [kind, setKind] = useState<ItemKind>("login");
  const [name, setName] = useState("");
  const [nameError, setNameError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Login fields
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [totpSecret, setTotpSecret] = useState("");
  const [uris, setUris] = useState<UriEntry[]>([{ uri: "", match: "domain" }]);

  // Secure Note
  const [note, setNote] = useState("");

  // Card
  const [cardholder, setCardholder] = useState("");
  const [cardNumber, setCardNumber] = useState("");
  const [expiry, setExpiry] = useState("");
  const [cvv, setCvv] = useState("");

  // Identity
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [email, setEmail] = useState("");
  const [phone, setPhone] = useState("");
  const [address, setAddress] = useState("");
  const [city, setCity] = useState("");
  const [country, setCountry] = useState("");

  async function handleGeneratePassword() {
    try {
      const pw = await invoke<string>("generate_password", {});
      setPassword(pw);
    } catch (err) {
      setSaveError(String(err));
    }
  }

  function addUri() {
    setUris([...uris, { uri: "", match: "domain" }]);
  }

  function removeUri(index: number) {
    if (uris.length <= 1) return;
    setUris(uris.filter((_, i) => i !== index));
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
        if (totpSecret) payload.totpSecret = totpSecret;
        const validUris = uris.filter((u) => u.uri.trim());
        if (validUris.length > 0) {
          payload.uris = validUris.map((u) => ({ uri: u.uri.trim(), match: u.match }));
        }
      } else if (kind === "secure_note") {
        if (note) payload.note = note;
      } else if (kind === "card") {
        if (cardholder) payload.cardholder = cardholder;
        if (cardNumber) payload.cardNumber = cardNumber;
        if (expiry) payload.expiry = expiry;
        if (cvv) payload.cvv = cvv;
      } else if (kind === "identity") {
        // Identity fields go through the existing mechanism too
        if (firstName) payload.firstName = firstName;
        if (lastName) payload.lastName = lastName;
        if (email) payload.email = email;
        if (phone) payload.phone = phone;
        if (address) payload.address = address;
        if (city) payload.city = city;
        if (country) payload.country = country;
      }

      const itemJson = JSON.stringify(payload);
      await invoke("add_item", { itemJson });
      onSaved();
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  }

  const inputClasses =
    "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent";
  const labelClasses =
    "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1";

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 overflow-y-auto p-4">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-lg my-auto">
        <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
          Add New Item
        </h2>

        <form id="add-item-form" onSubmit={handleSubmit} className="space-y-4 max-h-[70vh] overflow-y-auto pr-2">
          {saveError && (
            <div className="p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg" role="alert">
              {saveError}
            </div>
          )}

          {/* Type */}
          <div>
            <label htmlFor="add-kind" className={labelClasses}>Type</label>
            <select
              id="add-kind"
              value={kind}
              onChange={(e) => setKind(e.target.value as ItemKind)}
              className={inputClasses}
            >
              <option value="login">Login</option>
              <option value="secure_note">Secure Note</option>
              <option value="card">Card</option>
              <option value="identity">Identity</option>
            </select>
          </div>

          {/* Name */}
          <div>
            <label htmlFor="add-name" className={labelClasses}>Name *</label>
            <input
              id="add-name"
              type="text"
              value={name}
              onChange={(e) => { setName(e.target.value); if (nameError) setNameError(null); }}
              placeholder="e.g. GitHub"
              className={`${inputClasses} ${nameError ? "border-red-500" : ""}`}
              autoFocus
            />
            {nameError && <p className="text-sm text-red-600 mt-1">{nameError}</p>}
          </div>

          {/* Login fields */}
          {kind === "login" && (
            <>
              <div>
                <label htmlFor="add-username" className={labelClasses}>Username</label>
                <input
                  id="add-username"
                  type="text"
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  placeholder="Username or email"
                  className={inputClasses}
                />
              </div>
              <div>
                <label htmlFor="add-password" className={labelClasses}>Password</label>
                <div className="flex gap-2">
                  <input
                    id="add-password"
                    type="text"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="Password"
                    className={`${inputClasses} flex-1`}
                  />
                  <button
                    type="button"
                    onClick={handleGeneratePassword}
                    className="px-3 py-2 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors whitespace-nowrap"
                  >
                    Generate
                  </button>
                </div>
              </div>
              <div>
                <label htmlFor="add-totp" className={labelClasses}>TOTP Secret</label>
                <input
                  id="add-totp"
                  type="text"
                  value={totpSecret}
                  onChange={(e) => setTotpSecret(e.target.value)}
                  placeholder="Base32 TOTP secret (optional)"
                  className={inputClasses}
                />
              </div>
              <div>
                <div className="flex items-center justify-between mb-1">
                  <span className={labelClasses.replace(" mb-1", "")}>URIs</span>
                  <button
                    type="button"
                    onClick={addUri}
                    className="text-xs text-zvault-600 hover:text-zvault-700"
                  >
                    + Add URI
                  </button>
                </div>
                {uris.map((entry, index) => (
                  <div key={index} className="flex gap-2 mb-2">
                    <input
                      type="text"
                      value={entry.uri}
                      onChange={(e) => {
                        const updated = [...uris];
                        updated[index] = { ...updated[index], uri: e.target.value };
                        setUris(updated);
                      }}
                      placeholder="https://example.com"
                      className={`${inputClasses} flex-1`}
                      aria-label={`URI ${index + 1}`}
                    />
                    <select
                      value={entry.match}
                      onChange={(e) => {
                        const updated = [...uris];
                        updated[index] = { ...updated[index], match: e.target.value };
                        setUris(updated);
                      }}
                      className={`${inputClasses} w-auto`}
                      aria-label={`Match strategy ${index + 1}`}
                    >
                      <option value="domain">Domain</option>
                      <option value="host">Host</option>
                      <option value="startswith">StartsWith</option>
                      <option value="exact">Exact</option>
                      <option value="regex">Regex</option>
                      <option value="never">Never</option>
                    </select>
                    {uris.length > 1 && (
                      <button
                        type="button"
                        onClick={() => removeUri(index)}
                        className="px-2 text-red-500 hover:text-red-700"
                        aria-label={`Remove URI ${index + 1}`}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                ))}
              </div>
            </>
          )}

          {/* Secure Note fields */}
          {kind === "secure_note" && (
            <div>
              <label htmlFor="add-note" className={labelClasses}>Note</label>
              <textarea
                id="add-note"
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="Secure note content"
                rows={5}
                className={`${inputClasses} resize-y`}
              />
            </div>
          )}

          {/* Card fields */}
          {kind === "card" && (
            <>
              <div>
                <label htmlFor="add-cardholder" className={labelClasses}>Cardholder Name</label>
                <input id="add-cardholder" type="text" value={cardholder} onChange={(e) => setCardholder(e.target.value)} placeholder="Name on card" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-cardnumber" className={labelClasses}>Card Number</label>
                <input id="add-cardnumber" type="text" value={cardNumber} onChange={(e) => setCardNumber(e.target.value)} placeholder="Card number" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-expiry" className={labelClasses}>Expiry Date</label>
                <input id="add-expiry" type="text" value={expiry} onChange={(e) => setExpiry(e.target.value)} placeholder="MM/YY" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-cvv" className={labelClasses}>CVV</label>
                <input id="add-cvv" type="password" value={cvv} onChange={(e) => setCvv(e.target.value)} placeholder="CVV" className={inputClasses} />
              </div>
            </>
          )}

          {/* Identity fields */}
          {kind === "identity" && (
            <>
              <div>
                <label htmlFor="add-firstname" className={labelClasses}>First Name</label>
                <input id="add-firstname" type="text" value={firstName} onChange={(e) => setFirstName(e.target.value)} placeholder="First name" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-lastname" className={labelClasses}>Last Name</label>
                <input id="add-lastname" type="text" value={lastName} onChange={(e) => setLastName(e.target.value)} placeholder="Last name" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-email" className={labelClasses}>Email</label>
                <input id="add-email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="Email address" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-phone" className={labelClasses}>Phone</label>
                <input id="add-phone" type="tel" value={phone} onChange={(e) => setPhone(e.target.value)} placeholder="Phone number" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-address" className={labelClasses}>Address</label>
                <input id="add-address" type="text" value={address} onChange={(e) => setAddress(e.target.value)} placeholder="Street address" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-city" className={labelClasses}>City</label>
                <input id="add-city" type="text" value={city} onChange={(e) => setCity(e.target.value)} placeholder="City" className={inputClasses} />
              </div>
              <div>
                <label htmlFor="add-country" className={labelClasses}>Country</label>
                <input id="add-country" type="text" value={country} onChange={(e) => setCountry(e.target.value)} placeholder="Country" className={inputClasses} />
              </div>
            </>
          )}
        </form>

        {/* Buttons outside the scrollable area */}
        <div className="flex gap-3 justify-end mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            type="submit"
            form="add-item-form"
            disabled={saving}
            className="px-4 py-2 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors disabled:opacity-50"
          >
            {saving ? "Saving…" : "Add Item"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default VaultList;
