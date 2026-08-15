import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DeviceSecretKeyInfo {
  nsec: string;
  hex: string;
}

interface Props {
  vaultPath: string;
  open: boolean;
  onClose: () => void;
}

function ExportSecretKeyDialog({ vaultPath, open, onClose }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [secretKey, setSecretKey] = useState<DeviceSecretKeyInfo | null>(null);
  const [countdown, setCountdown] = useState(30);
  const [copiedField, setCopiedField] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Auto-hide countdown
  useEffect(() => {
    if (secretKey) {
      setCountdown(30);
      timerRef.current = setInterval(() => {
        setCountdown((prev) => {
          if (prev <= 1) {
            handleDone();
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    }
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [secretKey]);

  // Reset state when dialog opens/closes
  useEffect(() => {
    if (!open) {
      setPassword("");
      setError(null);
      setSecretKey(null);
      setLoading(false);
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    }
  }, [open]);

  const handleDone = () => {
    setSecretKey(null);
    setPassword("");
    setError(null);
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    onClose();
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setLoading(true);

    try {
      const result = await invoke<DeviceSecretKeyInfo>("export_device_secret_key", {
        password,
        path: vaultPath,
      });
      setSecretKey(result);
      setPassword("");
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const copyToClipboard = async (value: string, field: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 2000);
    } catch {
      // ignore
    }
  };

  if (!open) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-lg">
        {!secretKey ? (
          <>
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
              Export Device Secret Key
            </h2>

            {/* Warning */}
            <div className="mb-4 p-3 bg-red-50 dark:bg-red-900/20 rounded-lg border border-red-200 dark:border-red-800">
              <p className="text-sm text-red-700 dark:text-red-300 font-medium mb-1">
                ⚠️ Security Warning
              </p>
              <p className="text-xs text-red-600 dark:text-red-400">
                Your device secret key grants full control over this device's identity.
                Anyone with this key can impersonate your device and receive synced vault data.
                Only export this key for backup purposes. Never share it.
              </p>
            </div>

            <form onSubmit={handleSubmit} className="space-y-4">
              <div>
                <label
                  htmlFor="export-password"
                  className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
                >
                  Enter vault password to confirm
                </label>
                <input
                  id="export-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
                  autoFocus
                  autoComplete="current-password"
                />
              </div>
              {error && (
                <p className="text-sm text-red-600 dark:text-red-400" role="alert">
                  {error}
                </p>
              )}
              <div className="flex gap-3 justify-end">
                <button
                  type="button"
                  onClick={handleDone}
                  className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={loading || !password}
                  className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors disabled:opacity-50"
                >
                  {loading ? "Verifying…" : "Export Key"}
                </button>
              </div>
            </form>
          </>
        ) : (
          <>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100">
                Device Secret Key
              </h2>
              <span className="text-xs text-red-600 dark:text-red-400 font-mono">
                Auto-hide in {countdown}s
              </span>
            </div>

            <div className="space-y-3">
              <div>
                <div className="flex items-center justify-between mb-1">
                  <label className="text-xs font-medium text-gray-500 dark:text-gray-400">
                    nsec (bech32)
                  </label>
                  <button
                    type="button"
                    onClick={() => copyToClipboard(secretKey.nsec, "nsec")}
                    className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400"
                  >
                    {copiedField === "nsec" ? "Copied!" : "Copy"}
                  </button>
                </div>
                <div className="p-2 bg-gray-100 dark:bg-gray-900 rounded border font-mono text-xs break-all text-gray-900 dark:text-gray-100">
                  {secretKey.nsec}
                </div>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1">
                  <label className="text-xs font-medium text-gray-500 dark:text-gray-400">
                    Hex
                  </label>
                  <button
                    type="button"
                    onClick={() => copyToClipboard(secretKey.hex, "hex")}
                    className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400"
                  >
                    {copiedField === "hex" ? "Copied!" : "Copy"}
                  </button>
                </div>
                <div className="p-2 bg-gray-100 dark:bg-gray-900 rounded border font-mono text-xs break-all text-gray-900 dark:text-gray-100">
                  {secretKey.hex}
                </div>
              </div>
            </div>

            <div className="mt-4 flex justify-end">
              <button
                type="button"
                onClick={handleDone}
                className="px-4 py-2 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
              >
                Done
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

export default ExportSecretKeyDialog;
