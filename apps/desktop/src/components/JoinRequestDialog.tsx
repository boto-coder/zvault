import React, { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { QRCodeDisplay } from "./QRCodeDisplay";

interface JoinRequestDialogProps {
  onClose: () => void;
  onDeviceAdmitted: () => void;
}

/**
 * 3-step wizard dialog for requesting to join a vault:
 * 1. Generate join-request code (show as QR + text)
 * 2. Wait for the admin to share a response code
 * 3. Import the response and complete pairing
 */
export const JoinRequestDialog: React.FC<JoinRequestDialogProps> = ({
  onClose,
  onDeviceAdmitted,
}) => {
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [requestCode, setRequestCode] = useState<string | null>(null);
  const [responseCode, setResponseCode] = useState("");
  const [responseInfo, setResponseInfo] = useState<{
    pubkeyHex: string;
    label: string;
    pairingType: string;
    vaultId?: string;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Auto-generate request code on open
  useEffect(() => {
    generateRequest();
  }, []);

  const generateRequest = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<{ code: string }>("create_join_request_code");
      setRequestCode(result.code);
      setStep(2);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  // Step 2: Import admin's response
  const importResponse = async () => {
    setError(null);
    const trimmed = responseCode.trim();
    if (!trimmed.startsWith("zvault:")) {
      setError("Code must start with 'zvault:'");
      return;
    }
    try {
      const info = await invoke<{
        pairingType: string;
        pubkeyHex: string;
        label: string;
        vaultId?: string;
        timestamp: number;
      }>("import_pairing_code", { code: trimmed });
      setResponseInfo({
        pubkeyHex: info.pubkeyHex,
        label: info.label,
        pairingType: info.pairingType,
        vaultId: info.vaultId ?? undefined,
      });
      setStep(3);
    } catch (err) {
      setError(String(err));
    }
  };

  // Step 3: Confirm
  const confirmPairing = async () => {
    if (!responseInfo) return;
    setLoading(true);
    setError(null);
    try {
      await invoke("confirm_pairing", {
        pubkeyHex: responseInfo.pubkeyHex,
        label: responseInfo.label,
        pairingType: responseInfo.pairingType,
      });
      onDeviceAdmitted();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-white dark:bg-gray-900 rounded-lg shadow-xl p-6 max-w-md w-full mx-4">
        <h2 className="text-lg font-semibold mb-4">Join Request</h2>

        {step === 1 && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-gray-600 dark:text-gray-300">
              Generating your join-request code...
            </p>
            {loading && <p className="text-sm text-gray-500">Please wait...</p>}
          </div>
        )}

        {step === 2 && requestCode && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-gray-600 dark:text-gray-300">
              Share this code with the vault admin. Then paste their response below.
            </p>
            <QRCodeDisplay value={requestCode} label="Join Request Code" />
            <div className="bg-gray-100 dark:bg-gray-800 rounded p-3">
              <code className="text-xs break-all select-all">{requestCode}</code>
            </div>
            <hr className="border-gray-200 dark:border-gray-700" />
            <p className="text-sm font-medium">Paste admin's response code:</p>
            <input
              type="text"
              value={responseCode}
              onChange={(e) => setResponseCode(e.target.value)}
              placeholder="zvault:..."
              className="px-3 py-2 rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm font-mono"
            />
            <button
              onClick={importResponse}
              disabled={!responseCode.trim()}
              className="w-full py-2 bg-blue-600 text-white rounded font-medium hover:bg-blue-700 disabled:opacity-50"
            >
              Import Response
            </button>
          </div>
        )}

        {step === 3 && responseInfo && (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-gray-600 dark:text-gray-300">
              The admin has approved your request. Confirm to complete pairing:
            </p>
            <div className="bg-gray-100 dark:bg-gray-800 rounded p-3 space-y-1">
              <p className="text-sm"><strong>Admin device:</strong> {responseInfo.label}</p>
              <p className="text-xs text-gray-500 font-mono break-all">
                {responseInfo.pubkeyHex}
              </p>
              {responseInfo.vaultId && (
                <p className="text-xs text-gray-500">Vault: {responseInfo.vaultId}</p>
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
                onClick={confirmPairing}
                disabled={loading}
                className="flex-1 py-2 bg-green-600 text-white rounded font-medium hover:bg-green-700 disabled:opacity-50"
              >
                {loading ? "Completing..." : "Complete Pairing"}
              </button>
            </div>
          </div>
        )}

        {error && <p className="text-red-500 text-sm mt-3">{error}</p>}

        {step !== 3 && (
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
