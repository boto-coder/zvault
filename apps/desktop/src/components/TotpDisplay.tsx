import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface TotpResponse {
  code: string;
  remainingSeconds: number;
}

interface TotpDisplayProps {
  secret: string;
}

function TotpDisplay({ secret }: TotpDisplayProps) {
  const [code, setCode] = useState("------");
  const [remaining, setRemaining] = useState(30);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    const refresh = async () => {
      try {
        const result = await invoke<TotpResponse>("generate_totp", { secret });
        if (active) {
          setCode(result.code);
          setRemaining(result.remainingSeconds);
          setError(null);
        }
      } catch (err) {
        if (active) {
          setError(String(err));
          setCode("------");
        }
      }
    };

    refresh();
    const interval = setInterval(refresh, 1000);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [secret]);

  const handleCopy = async () => {
    if (code === "------" || error) return;
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
      // Clear clipboard after 30s
      setTimeout(() => {
        navigator.clipboard.writeText("").catch(() => {});
      }, 30000);
    } catch {
      // Clipboard write failed — silently ignore
    }
  };

  if (error) {
    return (
      <div className="flex items-center gap-3 p-3 bg-red-50 dark:bg-red-900/20 rounded-lg">
        <div className="text-sm text-red-600 dark:text-red-400">
          TOTP Error: {error}
        </div>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-3 p-3 bg-gray-100 dark:bg-gray-700 rounded-lg">
      <div className="flex-1">
        <label className="block text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
          TOTP Code
        </label>
        <div
          className="text-2xl font-mono font-bold text-gray-900 dark:text-gray-100 tracking-wider"
          aria-live="polite"
          aria-label={`TOTP code: ${code}`}
        >
          {code.slice(0, 3)} {code.slice(3)}
        </div>
      </div>
      <div className="text-sm text-gray-500 dark:text-gray-400 text-center min-w-[3rem]">
        <div className="text-lg font-mono">{remaining}s</div>
        <div className="text-xs">remaining</div>
      </div>
      <button
        type="button"
        onClick={handleCopy}
        className="px-3 py-1.5 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
        aria-label="Copy TOTP code"
      >
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  );
}

export default TotpDisplay;
