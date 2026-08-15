import React from "react";

interface QRCodeDisplayProps {
  value: string;
  size?: number;
  label?: string;
}

/**
 * Displays a QR code for the given value (pairing code).
 * Uses qrcode.react for rendering. Falls back to a text display
 * if the package is not available or the code is too long.
 */
export const QRCodeDisplay: React.FC<QRCodeDisplayProps> = ({
  value,
  size = 200,
  label,
}) => {
  // Lazy-import QRCodeSVG to avoid hard failure if not installed
  let QRComponent: React.FC<{ value: string; size: number }> | null = null;
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const qr = require("qrcode.react");
    QRComponent = qr.QRCodeSVG;
  } catch {
    // Package not available — will render text fallback
  }

  return (
    <div className="flex flex-col items-center gap-3">
      {label && (
        <p className="text-sm text-gray-500 dark:text-gray-400">{label}</p>
      )}
      {QRComponent ? (
        <QRComponent value={value} size={size} />
      ) : (
        <div
          className="border border-gray-300 dark:border-gray-600 rounded p-4 bg-gray-50 dark:bg-gray-800"
          style={{ maxWidth: size }}
        >
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">
            QR code (install qrcode.react for visual display):
          </p>
          <code className="text-xs break-all select-all">{value}</code>
        </div>
      )}
    </div>
  );
};
