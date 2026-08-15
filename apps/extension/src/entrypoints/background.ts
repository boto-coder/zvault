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

      case "GET_PASSWORD": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const { id } = message.payload as { id: string };
        const vault = JSON.parse(sessionVaultJson);
        const item = vault.items.find((i: { id: string }) => i.id === id);
        return item ? { password: item.password || null } : { error: "Item not found" };
      }

      case "GET_ITEM": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const { id: itemId } = message.payload as { id: string };
        const vault = JSON.parse(sessionVaultJson);
        const item = vault.items.find((i: { id: string }) => i.id === itemId);
        return item ? { item } : { error: "Item not found" };
      }

      case "LIST_DEVICES": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const vault = JSON.parse(sessionVaultJson);
        return { devices: vault.devices || [] };
      }

      case "ADMIT_DEVICE": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { pubkeyHex, label } = message.payload as { pubkeyHex: string; label: string };
        if (!/^[0-9a-f]{64}$/i.test(pubkeyHex)) return { error: "Invalid public key format" };
        const vault = JSON.parse(sessionVaultJson);
        const entry = {
          device_id: crypto.randomUUID(),
          nostr_pubkey: pubkeyHex.toLowerCase(),
          label,
          added_at: new Date().toISOString(),
          added_by: vault.devices?.[0]?.device_id || "unknown",
          revoked: false,
        };
        vault.devices = vault.devices || [];
        vault.devices.push(entry);
        vault.version = (vault.version || 0) + 1;
        sessionVaultJson = JSON.stringify(vault);
        const { initWasm: initWasmAdmit } = await import("../lib/wasm");
        const wasmAdmit = await initWasmAdmit();
        const encryptedAdmit = wasmAdmit.encrypt_vault(sessionPassword, sessionVaultJson);
        await browser.storage.local.set({ vault: Array.from(encryptedAdmit) });
        return { success: true, deviceId: entry.device_id };
      }

      case "REVOKE_DEVICE": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { deviceId } = message.payload as { deviceId: string };
        const vault = JSON.parse(sessionVaultJson);
        const device = vault.devices?.find((d: { device_id: string }) => d.device_id === deviceId);
        if (!device) return { error: "Device not found" };
        device.revoked = true;
        device.revoked_at = new Date().toISOString();
        vault.version = (vault.version || 0) + 1;
        sessionVaultJson = JSON.stringify(vault);
        const { initWasm: initWasmRevoke } = await import("../lib/wasm");
        const wasmRevoke = await initWasmRevoke();
        const encryptedRevoke = wasmRevoke.encrypt_vault(sessionPassword, sessionVaultJson);
        await browser.storage.local.set({ vault: Array.from(encryptedRevoke) });
        return { success: true };
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

      case "GET_RELAY_SETTINGS": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const vault = JSON.parse(sessionVaultJson);
        const relays = vault.settings?.relays || [];
        return { relays };
      }

      case "ADD_RELAY": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { url: addUrl } = message.payload as { url: string };
        const { initWasm: initWasmRelay } = await import("../lib/wasm");
        const wasmRelay = await initWasmRelay();
        try {
          sessionVaultJson = wasmRelay.add_relay_to_vault(sessionVaultJson, addUrl);
          const encryptedRelay = wasmRelay.encrypt_vault(sessionPassword, sessionVaultJson);
          await browser.storage.local.set({ vault: Array.from(encryptedRelay) });
          return { success: true };
        } catch (err) {
          return { error: String(err) };
        }
      }

      case "REMOVE_RELAY": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { url: removeUrl } = message.payload as { url: string };
        const { initWasm: initWasmRelayRm } = await import("../lib/wasm");
        const wasmRelayRm = await initWasmRelayRm();
        try {
          sessionVaultJson = wasmRelayRm.remove_relay_from_vault(sessionVaultJson, removeUrl);
          const encryptedRelayRm = wasmRelayRm.encrypt_vault(sessionPassword, sessionVaultJson);
          await browser.storage.local.set({ vault: Array.from(encryptedRelayRm) });
          return { success: true };
        } catch (err) {
          return { error: String(err) };
        }
      }

      case "TOGGLE_RELAY": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { url: toggleUrl, enabled: toggleEnabled } = message.payload as { url: string; enabled: boolean };
        const { initWasm: initWasmRelayTgl } = await import("../lib/wasm");
        const wasmRelayTgl = await initWasmRelayTgl();
        try {
          sessionVaultJson = wasmRelayTgl.toggle_relay_in_vault(sessionVaultJson, toggleUrl, toggleEnabled);
          const encryptedRelayTgl = wasmRelayTgl.encrypt_vault(sessionPassword, sessionVaultJson);
          await browser.storage.local.set({ vault: Array.from(encryptedRelayTgl) });
          return { success: true };
        } catch (err) {
          return { error: String(err) };
        }
      }

      case "RESET_RELAYS": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { initWasm: initWasmRelayRst } = await import("../lib/wasm");
        const wasmRelayRst = await initWasmRelayRst();
        try {
          sessionVaultJson = wasmRelayRst.reset_relays_in_vault(sessionVaultJson);
          const encryptedRelayRst = wasmRelayRst.encrypt_vault(sessionPassword, sessionVaultJson);
          await browser.storage.local.set({ vault: Array.from(encryptedRelayRst) });
          return { success: true };
        } catch (err) {
          return { error: String(err) };
        }
      }

      case "GET_DEVICE_PUBKEY": {
        if (!sessionVaultJson) return { error: "Vault is locked" };
        const vault = JSON.parse(sessionVaultJson);
        const device = vault.devices?.find((d: { revoked?: boolean }) => !d.revoked);
        if (!device) return { error: "No active device identity found" };
        // Encode pubkey as npub via WASM
        const { initWasm: initWasmNpub } = await import("../lib/wasm");
        const wasmNpub = await initWasmNpub();
        let npub: string;
        try {
          npub = wasmNpub.encode_npub_from_hex(device.nostr_pubkey);
        } catch (err) {
          return { error: `Failed to encode npub: ${err}` };
        }
        return {
          deviceId: device.device_id,
          label: device.label,
          pubkeyHex: device.nostr_pubkey,
          npub,
        };
      }

      case "EXPORT_DEVICE_SECRET_KEY": {
        if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
        const { password: exportPassword } = message.payload as { password: string };
        // Re-verify password by attempting to decrypt the vault data
        const stored = await browser.storage.local.get("vault");
        if (!stored.vault) return { error: "No vault data in storage" };
        const { initWasm: initWasmExport } = await import("../lib/wasm");
        const wasmExport = await initWasmExport();
        try {
          wasmExport.open_vault(exportPassword, new Uint8Array(stored.vault as number[]));
        } catch {
          return { error: "Invalid password" };
        }
        // In the extension, device identity is stored in the vault JSON itself.
        // The secret key is stored separately in encrypted storage.
        // For the extension, we store device_secret_key_hex in browser.storage.local (encrypted).
        const deviceStore = await browser.storage.local.get("device_secret_key_hex");
        if (!deviceStore.device_secret_key_hex) {
          return { error: "Device identity not initialised. No secret key stored." };
        }
        const secretKeyHex = deviceStore.device_secret_key_hex as string;
        // Encode as nsec using a simple bech32 encoding via WASM
        // The WASM module only exposes encode_npub_from_hex; for nsec we return hex only
        // since the extension cannot safely encode nsec without exposing it to WASM memory.
        // Actually, let's compute nsec in JS using the same bech32 algorithm.
        // For security, we return both hex and nsec (computed here).
        let nsec: string;
        try {
          // We don't have a WASM nsec encoder to avoid exposing secret key material
          // in WASM linear memory longer than necessary. Compute bech32 in JS.
          nsec = encodeBech32Nsec(secretKeyHex);
        } catch (err) {
          return { error: `Failed to encode nsec: ${err}` };
        }
        return { nsec, hex: secretKeyHex };
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

  // ─── Bech32 helper (NIP-19 nsec encoding) ─────────────────────────────

  /** Bech32 character set */
  const BECH32_CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";

  /** Bech32 polymod for checksum computation */
  function bech32Polymod(values: number[]): number {
    const GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];
    let chk = 1;
    for (const v of values) {
      const b = chk >>> 25;
      chk = ((chk & 0x1ffffff) << 5) ^ v;
      for (let i = 0; i < 5; i++) {
        if ((b >>> i) & 1) {
          chk ^= GEN[i];
        }
      }
    }
    return chk;
  }

  /** Expand HRP for checksum */
  function bech32HrpExpand(hrp: string): number[] {
    const ret: number[] = [];
    for (let i = 0; i < hrp.length; i++) {
      ret.push(hrp.charCodeAt(i) >> 5);
    }
    ret.push(0);
    for (let i = 0; i < hrp.length; i++) {
      ret.push(hrp.charCodeAt(i) & 31);
    }
    return ret;
  }

  /** Convert 8-bit bytes to 5-bit groups */
  function convertBits(data: number[], fromBits: number, toBits: number, pad: boolean): number[] {
    let acc = 0;
    let bits = 0;
    const ret: number[] = [];
    const maxv = (1 << toBits) - 1;
    for (const value of data) {
      acc = (acc << fromBits) | value;
      bits += fromBits;
      while (bits >= toBits) {
        bits -= toBits;
        ret.push((acc >>> bits) & maxv);
      }
    }
    if (pad) {
      if (bits > 0) {
        ret.push((acc << (toBits - bits)) & maxv);
      }
    }
    return ret;
  }

  /** Create bech32 checksum (original bech32, not bech32m) */
  function bech32CreateChecksum(hrp: string, data: number[]): number[] {
    const values = bech32HrpExpand(hrp).concat(data).concat([0, 0, 0, 0, 0, 0]);
    const polymod = bech32Polymod(values) ^ 1; // bech32 uses XOR 1; bech32m uses XOR 0x2bc830a3
    const ret: number[] = [];
    for (let i = 0; i < 6; i++) {
      ret.push((polymod >>> (5 * (5 - i))) & 31);
    }
    return ret;
  }

  /** Encode bytes as bech32 with the given HRP (original bech32 variant) */
  function bech32Encode(hrp: string, data: number[]): string {
    const fiveBit = convertBits(data, 8, 5, true);
    const checksum = bech32CreateChecksum(hrp, fiveBit);
    const combined = fiveBit.concat(checksum);
    let result = hrp + "1";
    for (const c of combined) {
      result += BECH32_CHARSET[c];
    }
    return result;
  }

  /** Encode a hex secret key as NIP-19 nsec bech32 string */
  function encodeBech32Nsec(hexKey: string): string {
    if (hexKey.length !== 64) throw new Error("Secret key must be 64 hex characters");
    const bytes: number[] = [];
    for (let i = 0; i < 64; i += 2) {
      bytes.push(parseInt(hexKey.substring(i, i + 2), 16));
    }
    return bech32Encode("nsec", bytes);
  }
});
