import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import DevicePubkeyCard from "../components/DevicePubkeyCard";
import ExportSecretKeyDialog from "../components/ExportSecretKeyDialog";
import { InviteDeviceDialog } from "../components/InviteDeviceDialog";
import { JoinRequestDialog } from "../components/JoinRequestDialog";
import { ConfirmPairingDialog } from "../components/ConfirmPairingDialog";

interface DeviceSummary {
  device_id: string;
  label: string;
  nostr_pubkey: string;
  added_at: string;
  revoked: boolean;
}

interface Props {
  onBack: () => void;
  vaultPath?: string;
}

function Devices({ onBack, vaultPath }: Props) {
  const [devices, setDevices] = useState<DeviceSummary[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [showAdmit, setShowAdmit] = useState(false);
  const [revokeConfirm, setRevokeConfirm] = useState<DeviceSummary | null>(null);
  const [showExportKey, setShowExportKey] = useState(false);
  const [showInvite, setShowInvite] = useState(false);
  const [showJoinRequest, setShowJoinRequest] = useState(false);
  const [showPasteCode, setShowPasteCode] = useState(false);

  // Admit form
  const [admitPubkey, setAdmitPubkey] = useState("");
  const [admitLabel, setAdmitLabel] = useState("");
  const [admitError, setAdmitError] = useState<string | null>(null);
  const [admitting, setAdmitting] = useState(false);

  // Copied feedback
  const [copied, setCopied] = useState(false);

  const loadDevices = async () => {
    try {
      const result = await invoke<DeviceSummary[]>("list_devices");
      setDevices(result);
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    loadDevices();
  }, []);

  const handleAdmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setAdmitError(null);

    const key = admitPubkey.trim();
    if (key.length !== 64 || !/^[0-9a-f]{64}$/i.test(key)) {
      setAdmitError("Public key must be 64 hex characters");
      return;
    }
    if (!admitLabel.trim()) {
      setAdmitError("Device label is required");
      return;
    }

    setAdmitting(true);
    try {
      await invoke("admit_device", { pubkeyHex: key, label: admitLabel.trim() });
      setShowAdmit(false);
      setAdmitPubkey("");
      setAdmitLabel("");
      await loadDevices();
    } catch (err) {
      setAdmitError(String(err));
    } finally {
      setAdmitting(false);
    }
  };

  const handleRevoke = async (deviceId: string) => {
    try {
      await invoke("revoke_device", { deviceId });
      setRevokeConfirm(null);
      await loadDevices();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleCopyPubkey = async (key: string) => {
    try {
      await navigator.clipboard.writeText(key);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

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
          <h1 className="text-xl font-bold text-zvault-700 dark:text-zvault-300">
            Devices
          </h1>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setShowInvite(true)}
              className="px-3 py-1.5 text-sm bg-green-600 hover:bg-green-700 text-white rounded-lg transition-colors"
            >
              Invite
            </button>
            <button
              type="button"
              onClick={() => setShowJoinRequest(true)}
              className="px-3 py-1.5 text-sm bg-purple-600 hover:bg-purple-700 text-white rounded-lg transition-colors"
            >
              Join Request
            </button>
            <button
              type="button"
              onClick={() => setShowPasteCode(true)}
              className="px-3 py-1.5 text-sm bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors"
            >
              Paste Code
            </button>
            <button
              type="button"
              onClick={() => setShowAdmit(true)}
              className="px-3 py-1.5 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
              title="Advanced: manual admit by public key"
            >
              + Admit (Advanced)
            </button>
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6">
        {error && (
          <div className="mb-4 p-3 text-sm text-red-700 bg-red-100 dark:text-red-300 dark:bg-red-900/30 rounded-lg" role="alert">
            {error}
          </div>
        )}

        {/* Instructional text */}
        <div className="mb-6 p-4 bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800">
          <p className="text-sm text-blue-700 dark:text-blue-300">
            To sync vaults between devices, both devices must admit each other. Share your public key with the other device, and enter their public key using "Admit Device".
          </p>
        </div>

        {/* This device's public key card */}
        <DevicePubkeyCard onExportKey={() => setShowExportKey(true)} />

        {/* Device list */}
        {devices.length === 0 ? (
          <div className="text-center py-12 text-gray-500 dark:text-gray-400">
            No devices in the trust group yet. Admit a device to start syncing.
          </div>
        ) : (
          <div className="space-y-3">
            {devices.map((device) => (
              <div
                key={device.device_id}
                className="flex items-center justify-between p-4 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700"
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <h3 className="text-sm font-medium text-gray-900 dark:text-gray-100">
                      {device.label}
                    </h3>
                    {device.revoked && (
                      <span className="px-2 py-0.5 text-xs font-medium bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300 rounded">
                        Revoked
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 mt-1">
                    <p className="text-xs text-gray-500 dark:text-gray-400 font-mono truncate">
                      {device.nostr_pubkey}
                    </p>
                    <button
                      type="button"
                      onClick={() => handleCopyPubkey(device.nostr_pubkey)}
                      className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400 flex-shrink-0"
                      title="Copy public key"
                    >
                      {copied ? "Copied!" : "Copy"}
                    </button>
                  </div>
                  <p className="text-xs text-gray-400 mt-1">
                    Added: {new Date(device.added_at).toLocaleDateString()}
                  </p>
                </div>
                {!device.revoked && (
                  <button
                    type="button"
                    onClick={() => setRevokeConfirm(device)}
                    className="ml-4 px-3 py-1.5 text-xs text-red-600 hover:text-red-700 border border-red-300 hover:border-red-400 dark:border-red-700 dark:hover:border-red-600 rounded-lg transition-colors flex-shrink-0"
                  >
                    Revoke
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </main>

      {/* Admit Device Modal */}
      {showAdmit && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">
              Admit Device
            </h2>
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
              Enter the public key and label of the device you want to admit to your trust group. Both devices must admit each other for sync to work.
            </p>
            <form onSubmit={handleAdmit} className="space-y-4">
              <div>
                <label htmlFor="admit-pubkey" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Public Key (hex)
                </label>
                <input
                  id="admit-pubkey"
                  type="text"
                  value={admitPubkey}
                  onChange={(e) => setAdmitPubkey(e.target.value)}
                  placeholder="64 hex characters"
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent font-mono text-sm"
                  autoFocus
                />
              </div>
              <div>
                <label htmlFor="admit-label" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Device Label
                </label>
                <input
                  id="admit-label"
                  type="text"
                  value={admitLabel}
                  onChange={(e) => setAdmitLabel(e.target.value)}
                  placeholder="e.g. Bob's Phone"
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 focus:ring-2 focus:ring-zvault-500 focus:border-transparent"
                />
              </div>
              {admitError && (
                <p className="text-sm text-red-600 dark:text-red-400" role="alert">{admitError}</p>
              )}
              <div className="flex gap-3 justify-end">
                <button
                  type="button"
                  onClick={() => { setShowAdmit(false); setAdmitError(null); }}
                  className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={admitting}
                  className="px-4 py-2 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors disabled:opacity-50"
                >
                  {admitting ? "Admitting…" : "Admit"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Revoke Confirmation Modal */}
      {revokeConfirm && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-sm">
            <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-2">
              Revoke Device
            </h2>
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-6">
              Revoke <strong>{revokeConfirm.label}</strong>? This device will no longer receive vault updates and its messages will be rejected.
            </p>
            <div className="flex gap-3 justify-end">
              <button
                type="button"
                onClick={() => setRevokeConfirm(null)}
                className="px-4 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => handleRevoke(revokeConfirm.device_id)}
                className="px-4 py-2 text-sm bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
              >
                Revoke
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Export Secret Key Dialog */}
      <ExportSecretKeyDialog
        vaultPath={vaultPath || ""}
        open={showExportKey}
        onClose={() => setShowExportKey(false)}
      />

      {/* Invite Device Dialog */}
      {showInvite && (
        <InviteDeviceDialog
          onClose={() => setShowInvite(false)}
          onDeviceAdmitted={loadDevices}
        />
      )}

      {/* Join Request Dialog */}
      {showJoinRequest && (
        <JoinRequestDialog
          onClose={() => setShowJoinRequest(false)}
          onDeviceAdmitted={loadDevices}
        />
      )}

      {/* Paste Code (Confirm Pairing) Dialog */}
      {showPasteCode && (
        <ConfirmPairingDialog
          onClose={() => setShowPasteCode(false)}
          onDeviceAdmitted={loadDevices}
        />
      )}
    </div>
  );
}

export default Devices;
