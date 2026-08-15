import React from "react";
import { QRCodeSVG } from "qrcode.react";

interface QRCodeDisplayProps {
  value: string;
  size?: number;
  label?: string;
}

/**
 * Displays a QR code for the given value (pairing code).
 * Uses qrcode.react for SVG rendering.
 */
export const QRCodeDisplay: React.FC<QRCodeDisplayProps> = ({
  value,
  size = 200,
  label,
}) => {
  return (
    <div className="flex flex-col items-center gap-3">
      {label && (
        <p className="text-sm text-gray-500 dark:text-gray-400">{label}</p>
      )}
      <QRCodeSVG value={value} size={size} />
    </div>
  );
};
