import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ImportCodeInput } from "./ImportCodeInput";

interface ConfirmPairingDialogProps {
  onClose: () => void;
  onDeviceAdmitted: () => void;
}

/**
 * Dialog for importing a pairing code (paste) and confirming the pairing.
 * Handles all pairing types: invite, join-request, invite-response, join-response.
 */
export const ConfirmPairingDialog: React.FC<ConfirmPairingDialogProps> = ({
  onClose,
  onDeviceAdmitted,
}) => {
  const [pairingInfo, setPairingInfo] = useState<{
    pairingType: string;
    pubkeyHex: string;
    label: string;
    vaultId?: string;
    timestamp: number;
  } | null>(null);
  const [responseCode, setResponseCode] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const handleImport = async (code: string) => {
    setError(null);
    try {
      const info = await invoke<{
        pairingType: string;
        pubkeyHex: string;
        label: string;
        vaultId?: string;
        timestamp: number;
      }>("import_pairing_code", { code });
      setPairingInfo(info);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleConfirm = async () => {
    if (!pairingInfo) return;
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<{ code: string } | null>("confirm_pairing", {
        pubkeyHex: pairingInfo.pubkeyHex,
        label: pairingInfo.label,
        pairingType: pairingInfo.pairingType,
      });
      if (result && result.code) {
        setResponseCode(result.code);
      } else {
        onDeviceAdmitted();
        onClose();
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const pairingTypeLabel = (t: string): string => {
    switch (t) {
      case "invite": return "Invite (device wants to add you)";
      case "join_request": return "Join Request (device wants to join your vault)";
      case "invite_response": return "Invite Response";
      case "join_response": return "Join Response";
      default: return t;
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-900 rounded-lg shadow-xl p-6 max-w-md w-full mx-4">
        <h2 className="text-lg font-semibold mb-4">Import Pairing Code</h2>

        {!pairingInfo && !responseCode && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-gray-600 dark:text-gray-300">
              Paste a pairing code received from another device.
            </p>
            <ImportCodeInput onSubmit={handleImport} />
          </div>
        )}

        {pairingInfo && !responseCode && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-gray-600 dark:text-gray-300">
              {pairingTypeLabel(pairingInfo.pairingType)}
            </p>
            <div className="bg-gray-100 dark:bg-gray-800 rounded p-3 space-y-1">
              <p className="text-sm"><strong>Device:</strong> {pairingInfo.label}</p>
              <p className="text-xs text-gray-500 font-mono break-all">
                {pairingInfo.pubkeyHex}
              </p>
              {pairingInfo.vaultId && (
                <p className="text-xs text-gray-500">Vault: {pairingInfo.vaultId}</p>
              )}
            </div>
            <div className="flex gap-2">
              <button
                onClick={onClose}
                className="flex-1 py-2 border border-gray-300 dark:border-gray-600 rounded font-medium hover:bg-gray-100 dark:hover:bg-gray-800"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirm}
                disabled={loading}
                className="flex-1 py-2 bg-green-600 text-white rounded font-medium hover:bg-green-700 disabled:opacity-50"
              >
                {loading ? "Admitting..." : "Confirm & Admit"}
              </button>
            </div>
          </div>
        )}

        {responseCode && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-green-600 dark:text-green-400 font-medium">
              ✓ Device admitted successfully!
            </p>
            <p className="text-sm text-gray-600 dark:text-gray-300">
              Send this response code back to the other device:
            </p>
            <div className="bg-gray-100 dark:bg-gray-800 rounded p-3">
              <code className="text-xs break-all select-all">{responseCode}</code>
            </div>
            <button
              onClick={() => {
                onDeviceAdmitted();
                onClose();
              }}
              className="w-full py-2 bg-blue-600 text-white rounded font-medium hover:bg-blue-700"
            >
              Done
            </button>
          </div>
        )}

        {error && <p className="text-red-500 text-sm mt-3">{error}</p>}

        {!pairingInfo && !responseCode && (
          <button
            onClick={onClose}
            className="mt-4 w-full py-2 border border-gray-300 dark:border-gray-600 rounded font-medium hover:bg-gray-100 dark:hover:bg-gray-800"
          >
            Cancel
          </button>
        )}
      </div>
    </div>
  );
};
