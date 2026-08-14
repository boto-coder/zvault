import { defineConfig } from "wxt";
import react from "@vitejs/plugin-react";

export default defineConfig({
  srcDir: "src",
  publicDir: "../public",
  modules: [],
  vite: () => ({
    plugins: [react()],
    build: {
      target: "esnext",
    },
    optimizeDeps: {
      exclude: ["zvault-wasm"],
    },
  }),
  manifest: {
    name: "ZVault",
    description:
      "Local-first, end-to-end encrypted password manager with Nostr sync",
    permissions: ["storage", "activeTab", "clipboardWrite", "alarms"],
    host_permissions: ["https://*/*"],
    content_security_policy: {
      extension_pages:
        "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'",
    },
    web_accessible_resources: [
      {
        resources: ["wasm/*"],
        matches: ["<all_urls>"],
      },
    ],
  },
});
