import React, { useState } from "react";

interface ImportCodeInputProps {
  onSubmit: (code: string) => void;
  placeholder?: string;
  disabled?: boolean;
}

/**
 * Text input for pasting zvault: pairing codes.
 * Validates the prefix before submission.
 */
export const ImportCodeInput: React.FC<ImportCodeInputProps> = ({
  onSubmit,
  placeholder = "Paste zvault: code here...",
  disabled = false,
}) => {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = value.trim();
    if (!trimmed.startsWith("zvault:")) {
      setError("Code must start with 'zvault:'");
      return;
    }
    if (trimmed.length < 10) {
      setError("Code is too short");
      return;
    }
    setError(null);
    onSubmit(trimmed);
  };

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-2">
      <div className="flex gap-2">
        <input
          type="text"
          value={value}
          onChange={(e) => {
            setValue(e.target.value);
            setError(null);
          }}
          placeholder={placeholder}
          disabled={disabled}
          className="flex-1 px-3 py-2 rounded border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <button
          type="submit"
          disabled={disabled || !value.trim()}
          className="px-4 py-2 bg-blue-600 text-white rounded text-sm font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          Import
        </button>
      </div>
      {error && <p className="text-red-500 text-xs">{error}</p>}
    </form>
  );
};
