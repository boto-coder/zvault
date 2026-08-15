import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DevicePubkeyInfo {
  deviceId: string;
  label: string;
  pubkeyHex: string;
  npub: string;
}

interface Props {
  onExportKey: () => void;
}

function DevicePubkeyCard({ onExportKey }: Props) {
  const [info, setInfo] = useState<DevicePubkeyInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copiedField, setCopiedField] = useState<string | null>(null);

  useEffect(() => {
    invoke<DevicePubkeyInfo>("get_device_pubkey")
      .then(setInfo)
      .catch((err) => setError(String(err)));
  }, []);

  const copyToClipboard = async (value: string, field: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 2000);
    } catch {
      // ignore clipboard errors
    }
  };

  if (error) {
    return (
      <div className="mb-6 p-4 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg border border-yellow-200 dark:border-yellow-800">
        <p className="text-sm text-yellow-700 dark:text-yellow-300">
          {error}
        </p>
      </div>
    );
  }

  if (!info) {
    return null;
  }

  const truncatedHex =
    info.pubkeyHex.substring(0, 8) + "…" + info.pubkeyHex.substring(56);

  return (
    <div className="mb-6 p-4 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700">
      <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-3">
        This Device
      </h3>
      <dl className="space-y-2 text-sm">
        <div className="flex items-start justify-between">
          <dt className="text-gray-500 dark:text-gray-400 w-24 flex-shrink-0">
            Label
          </dt>
          <dd className="text-gray-900 dark:text-gray-100 text-right">
            {info.label}
          </dd>
        </div>
        <div className="flex items-start justify-between">
          <dt className="text-gray-500 dark:text-gray-400 w-24 flex-shrink-0">
            Device ID
          </dt>
          <dd className="text-gray-900 dark:text-gray-100 font-mono text-xs text-right break-all">
            {info.deviceId}
          </dd>
        </div>
        <div className="flex items-start justify-between gap-2">
          <dt className="text-gray-500 dark:text-gray-400 w-24 flex-shrink-0">
            Pubkey
          </dt>
          <dd className="flex items-center gap-1 text-right">
            <span className="text-gray-900 dark:text-gray-100 font-mono text-xs">
              {truncatedHex}
            </span>
            <button
              type="button"
              onClick={() => copyToClipboard(info.pubkeyHex, "hex")}
              className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400 flex-shrink-0"
              title="Copy full hex public key"
            >
              {copiedField === "hex" ? "Copied!" : "Copy"}
            </button>
          </dd>
        </div>
        <div className="flex items-start justify-between gap-2">
          <dt className="text-gray-500 dark:text-gray-400 w-24 flex-shrink-0">
            npub
          </dt>
          <dd className="flex items-center gap-1 text-right min-w-0">
            <span className="text-gray-900 dark:text-gray-100 font-mono text-xs truncate">
              {info.npub}
            </span>
            <button
              type="button"
              onClick={() => copyToClipboard(info.npub, "npub")}
              className="text-xs text-zvault-600 hover:text-zvault-700 dark:text-zvault-400 flex-shrink-0"
              title="Copy npub"
            >
              {copiedField === "npub" ? "Copied!" : "Copy"}
            </button>
          </dd>
        </div>
      </dl>
      <div className="mt-3 pt-3 border-t border-gray-200 dark:border-gray-700">
        <button
          type="button"
          onClick={onExportKey}
          className="text-xs text-red-600 hover:text-red-700 dark:text-red-400 dark:hover:text-red-300 font-medium"
        >
          Export Secret Key…
        </button>
      </div>
    </div>
  );
}

export default DevicePubkeyCard;
