import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface ItemDetailData {
  id: string;
  kind: string;
  name: string;
  username: string | null;
  password: string | null;
  totpSecret: string | null;
  uris: { uri: string; match: string }[];
  note: string | null;
  cardNumber: string | null;
  expiry: string | null;
  cvv: string | null;
  cardholder: string | null;
  favourite: boolean;
  createdAt: string;
  updatedAt: string;
}

interface Props {
  itemId: string;
  onBack: () => void;
}

function ItemDetail({ itemId, onBack }: Props) {
  const [item, setItem] = useState<ItemDetailData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [showPassword, setShowPassword] = useState(false);
  const [showCvv, setShowCvv] = useState(false);

  // Edit form state
  const [editName, setEditName] = useState("");
  const [editUsername, setEditUsername] = useState("");
  const [editPassword, setEditPassword] = useState("");
  const [editNote, setEditNote] = useState("");
  const [editCardNumber, setEditCardNumber] = useState("");
  const [editExpiry, setEditExpiry] = useState("");
  const [editCvv, setEditCvv] = useState("");
  const [editCardholder, setEditCardholder] = useState("");

  const loadItem = async () => {
    try {
      const result = await invoke<ItemDetailData>("get_item", { id: itemId });
      setItem(result);
      // Populate edit form
      setEditName(result.name);
      setEditUsername(result.username || "");
      setEditPassword(result.password || "");
      setEditNote(result.note || "");
      setEditCardNumber(result.cardNumber || "");
      setEditExpiry(result.expiry || "");
      setEditCvv(result.cvv || "");
      setEditCardholder(result.cardholder || "");
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    loadItem();
  }, [itemId]);

  const handleSave = async () => {
    if (!item) return;
    try {
      const itemJson = JSON.stringify({
        id: item.id,
        kind: item.kind,
        name: editName,
        username: editUsername || null,
        password: editPassword || null,
        note: editNote || null,
        cardNumber: editCardNumber || null,
        expiry: editExpiry || null,
        cvv: editCvv || null,
        cardholder: editCardholder || null,
        favourite: item.favourite,
      });
      await invoke("update_item", { itemJson });
      setEditing(false);
      loadItem();
    } catch (err) {
      setError(String(err));
    }
  };

  if (!item) {
    return (
      <div className="min-h-screen bg-gray-50 dark:bg-gray-900 flex items-center justify-center">
        {error ? (
          <div className="text-red-600 dark:text-red-400">{error}</div>
        ) : (
          <div className="text-gray-500">Loading…</div>
        )}
      </div>
    );
  }

  const kindLabel = {
    login: "Login",
    secure_note: "Secure Note",
    card: "Card",
    identity: "Identity",
  }[item.kind] || item.kind;

  return (
    <div className="min-h-screen bg-gray-50 dark:bg-gray-900">
      {/* Header */}
      <header className="bg-white dark:bg-gray-800 shadow-sm border-b border-gray-200 dark:border-gray-700">
        <div className="max-w-4xl mx-auto px-4 py-3 flex items-center justify-between">
          <button
            type="button"
            onClick={onBack}
            className="text-sm text-zvault-600 hover:text-zvault-700 dark:text-zvault-300 font-medium"
          >
            ← Back
          </button>
          <div className="flex items-center gap-3">
            {!editing ? (
              <button
                type="button"
                onClick={() => setEditing(true)}
                className="px-3 py-1.5 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
              >
                Edit
              </button>
            ) : (
              <>
                <button
                  type="button"
                  onClick={() => setEditing(false)}
                  className="px-3 py-1.5 text-sm bg-gray-200 hover:bg-gray-300 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={handleSave}
                  className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-700 text-white rounded-lg transition-colors"
                >
                  Save
                </button>
              </>
            )}
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6">
        {error && (
          <div
            className="mb-4 p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg"
            role="alert"
          >
            {error}
          </div>
        )}

        <div className="bg-white dark:bg-gray-800 rounded-xl shadow-sm p-6">
          {/* Item name + kind badge */}
          <div className="flex items-center gap-3 mb-6">
            {editing ? (
              <input
                type="text"
                value={editName}
                onChange={(e) => setEditName(e.target.value)}
                className="text-xl font-semibold px-2 py-1 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 flex-1"
              />
            ) : (
              <h2 className="text-xl font-semibold text-gray-900 dark:text-gray-100">
                {item.name}
              </h2>
            )}
            <span className="px-2 py-0.5 text-xs font-medium bg-zvault-100 text-zvault-700 dark:bg-zvault-900 dark:text-zvault-300 rounded">
              {kindLabel}
            </span>
          </div>

          {/* Fields */}
          <div className="space-y-4">
            {/* Login fields */}
            {item.kind === "login" && (
              <>
                <Field
                  label="Username"
                  value={editing ? editUsername : item.username}
                  editing={editing}
                  onChange={setEditUsername}
                />
                <Field
                  label="Password"
                  value={editing ? editPassword : item.password}
                  editing={editing}
                  onChange={setEditPassword}
                  secret={!editing}
                  showSecret={showPassword}
                  onToggleSecret={() => setShowPassword(!showPassword)}
                />
                {item.uris.length > 0 && !editing && (
                  <div>
                    <label className="block text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
                      URIs
                    </label>
                    <ul className="space-y-1">
                      {item.uris.map((u, i) => (
                        <li
                          key={i}
                          className="text-sm text-gray-700 dark:text-gray-300"
                        >
                          {u.uri}{" "}
                          <span className="text-gray-400">({u.match})</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
              </>
            )}

            {/* Secure Note fields */}
            {item.kind === "secure_note" && (
              <div>
                <label className="block text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
                  Note
                </label>
                {editing ? (
                  <textarea
                    value={editNote}
                    onChange={(e) => setEditNote(e.target.value)}
                    rows={6}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500"
                  />
                ) : (
                  <p className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap">
                    {item.note || "—"}
                  </p>
                )}
              </div>
            )}

            {/* Card fields */}
            {item.kind === "card" && (
              <>
                <Field
                  label="Cardholder"
                  value={editing ? editCardholder : item.cardholder}
                  editing={editing}
                  onChange={setEditCardholder}
                />
                <Field
                  label="Card Number"
                  value={editing ? editCardNumber : item.cardNumber}
                  editing={editing}
                  onChange={setEditCardNumber}
                  secret={!editing}
                  showSecret={showPassword}
                  onToggleSecret={() => setShowPassword(!showPassword)}
                />
                <Field
                  label="Expiry"
                  value={editing ? editExpiry : item.expiry}
                  editing={editing}
                  onChange={setEditExpiry}
                />
                <Field
                  label="CVV"
                  value={editing ? editCvv : item.cvv}
                  editing={editing}
                  onChange={setEditCvv}
                  secret={!editing}
                  showSecret={showCvv}
                  onToggleSecret={() => setShowCvv(!showCvv)}
                />
              </>
            )}

            {/* Metadata */}
            <div className="pt-4 border-t border-gray-200 dark:border-gray-700">
              <div className="grid grid-cols-2 gap-4 text-sm text-gray-500 dark:text-gray-400">
                <div>
                  <span className="font-medium">Created:</span>{" "}
                  {new Date(item.createdAt).toLocaleString()}
                </div>
                <div>
                  <span className="font-medium">Updated:</span>{" "}
                  {new Date(item.updatedAt).toLocaleString()}
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}

// ─── Field component ─────────────────────────────────────────────────────────

interface FieldProps {
  label: string;
  value: string | null;
  editing: boolean;
  onChange: (v: string) => void;
  secret?: boolean;
  showSecret?: boolean;
  onToggleSecret?: () => void;
}

function Field({
  label,
  value,
  editing,
  onChange,
  secret,
  showSecret,
  onToggleSecret,
}: FieldProps) {
  return (
    <div>
      <label className="block text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
        {label}
      </label>
      {editing ? (
        <input
          type="text"
          value={value || ""}
          onChange={(e) => onChange(e.target.value)}
          className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500"
        />
      ) : (
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-700 dark:text-gray-300">
            {secret && !showSecret
              ? "••••••••"
              : value || "—"}
          </span>
          {secret && value && (
            <button
              type="button"
              onClick={onToggleSecret}
              className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400"
            >
              {showSecret ? "Hide" : "Show"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

export default ItemDetail;
