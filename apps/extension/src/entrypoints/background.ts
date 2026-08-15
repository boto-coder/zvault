/**
 * ZVault background service worker.
 *
 * Responsibilities:
 * - Holds the decrypted vault JSON in memory (session key).
 * - Handles messages from popup and content scripts.
 * - Manages auto-lock timer.
 * - Never persists plaintext to storage.
 */

export default defineBackground(() => {
  // ─── Session state (in-memory only; lost on service worker restart) ─────

  /** Decrypted vault JSON. Null when locked. */
  let sessionVaultJson: string | null = null;

  /** Password kept in memory for re-encryption on save. Cleared on lock. */
  let sessionPassword: string | null = null;

  /** Auto-lock timeout in minutes (default: 15). */
  const AUTO_LOCK_MINUTES = 15;

  // ─── Message handler ───────────────────────────────────────────────────

  browser.runtime.onMessage.addListener(
    (
      message: { type: string; payload?: unknown },
      _sender: browser.Runtime.MessageSender,
      sendResponse: (response: unknown) => void
    ) => {
      handleMessage(message)
        .then(sendResponse)
        .catch((err) => sendResponse({ error: String(err) }));
      return true; // keep the message channel open for async response
    }
  );

  async function handleMessage(message: {
    type: string;
    payload?: unknown;
  }): Promise<unknown> {
    switch (message.type) {
      case "UNLOCK": {
        const { password, data } = message.payload as {
          password: string;
          data: number[];
        };
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const vaultJson = wasm.open_vault(password, new Uint8Array(data));
        sessionVaultJson = vaultJson;
        sessionPassword = password;
        resetAutoLock();
        return { success: true };
      }

      case "CREATE": {
        const { password } = message.payload as { password: string };
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const data = wasm.create_vault(password);
        sessionVaultJson = wasm.open_vault(password, data);
        sessionPassword = password;
        resetAutoLock();
        return { success: true, data: Array.from(data) };
      }

      case "LOCK": {
        lock();
        return { success: true };
      }

      case "GET_STATUS": {
        return { unlocked: sessionVaultJson !== null };
      }

      case "LIST_ITEMS": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const items = wasm.list_items(sessionVaultJson);
        return { items };
      }

      case "ADD_ITEM": {
        if (!sessionVaultJson || !sessionPassword)
          return { error: "Vault is locked" };
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const itemJson = JSON.stringify(message.payload);
        sessionVaultJson = wasm.add_item(sessionVaultJson, itemJson);
        // Re-encrypt and persist to storage
        const encrypted = wasm.encrypt_vault(
          sessionPassword,
          sessionVaultJson
        );
        await browser.storage.local.set({
          vault: Array.from(encrypted),
        });
        // Fire-and-forget Nostr sync — never block the response
        triggerNostrSync().catch((err) =>
          console.warn("[ZVault] Nostr sync failed:", err)
        );
        return { success: true };
      }

      case "GENERATE_PASSWORD": {
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const reqLength = (message.payload as { length?: number } | undefined)
          ?.length;
        try {
          const pw = wasm.generate_password(reqLength);
          return { password: pw };
        } catch (err) {
          return { error: String(err) };
        }
      }

      case "GENERATE_TOTP": {
        const { secret } = message.payload as { secret: string };
        const { initWasm } = await import("../lib/wasm");
        const wasm = await initWasm();
        const code = wasm.generate_totp(secret);
        const now = Math.floor(Date.now() / 1000);
        const remainingSeconds = 30 - (now % 30);
        return { code, remainingSeconds };
      }

      default:
        return { error: `Unknown message type: ${message.type}` };
    }
  }

  // ─── Auto-lock ─────────────────────────────────────────────────────────

  function resetAutoLock() {
    browser.alarms.clear("auto-lock");
    browser.alarms.create("auto-lock", {
      delayInMinutes: AUTO_LOCK_MINUTES,
    });
  }

  function lock() {
    sessionVaultJson = null;
    sessionPassword = null;
    browser.alarms.clear("auto-lock");
  }

  browser.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === "auto-lock") {
      lock();
    }
  });

  // ─── Nostr sync (fire-and-forget) ─────────────────────────────────────

  /**
   * Trigger a Nostr sync to propagate the latest vault state to connected devices.
   * This is best-effort — sync failures are logged but never surfaced to the user
   * during an add-item operation.
   */
  async function triggerNostrSync(): Promise<void> {
    // TODO: Implement full NIP-44/NIP-59 sync when relay configuration is available.
    // For now this is a no-op placeholder that will be wired up when the extension
    // gains relay settings and device identity management.
    //
    // The implementation will:
    // 1. Build a full sync message from the current vault state
    // 2. NIP-44 encrypt it for each admitted device's public key
    // 3. NIP-59 gift-wrap the sealed message
    // 4. Publish to configured relays via WebSocket
    console.debug("[ZVault] Nostr sync triggered (not yet wired to relays)");
  }
});
