/**
 * WASM bridge for zvault-core.
 *
 * Loads the zvault-wasm module and exposes typed wrappers for vault operations.
 * The WASM binary is loaded lazily on first use from the extension's bundled assets.
 *
 * Supports both:
 * - Chrome MV3 service workers (dynamic import)
 * - Firefox MV2 background scripts (fetch + WebAssembly.instantiate fallback)
 */

export interface ZVaultWasm {
  create_vault(password: string): Uint8Array;
  open_vault(password: string, data: Uint8Array): string;
  encrypt_vault(password: string, vault_json: string): Uint8Array;
  add_item(vault_json: string, item_json: string): string;
  list_items(vault_json: string): unknown[];
  generate_totp(secret: string): string;
  generate_password(length?: number): string;
  validate_relay_url(url: string): string;
  add_relay_to_vault(vault_json: string, url: string): string;
  remove_relay_from_vault(vault_json: string, url: string): string;
  toggle_relay_in_vault(vault_json: string, url: string, enabled: boolean): string;
  reset_relays_in_vault(vault_json: string): string;
  get_enabled_relays(vault_json: string): string[];
  encode_npub_from_hex(pubkey_hex: string): string;
  create_invite_code(pubkey_hex: string, label: string, vault_id: string): string;
  create_join_request_code(pubkey_hex: string, label: string): string;
  decode_pairing_code(code: string): unknown;
  create_response_code(response_type: string, pubkey_hex: string, label: string, vault_id?: string): string;
  admit_device_from_pairing(vault_json: string, remote_pubkey: string, label: string): string;
  // Sync / NIP-44 / NIP-59
  build_full_sync_message(vault_json: string, device_id: string, secret_key_hex: string, recipient_pubkey_hex: string): string;
  apply_sync_message(vault_json: string, sync_msg_json: string, secret_key_hex: string, sender_pubkey_hex: string): string;
  nip44_encrypt(sender_sk_hex: string, recipient_pk_hex: string, plaintext: string): string;
  nip44_decrypt(receiver_sk_hex: string, sender_pk_hex: string, ciphertext_b64: string): string;
  gift_wrap(sender_sk_hex: string, recipient_pk_hex: string, content: string, kind: number, tags_json: string): string;
  unwrap_gift_wrap(receiver_sk_hex: string, event_json: string): string;
  sign_event(sk_hex: string, event_json: string): string;
  verify_event(event_json: string): boolean;
}

let wasmInstance: ZVaultWasm | null = null;

/**
 * Initialise the WASM module. Must be called before any vault operations.
 * In MV3 extensions, call this in the service worker on startup.
 */
export async function initWasm(): Promise<ZVaultWasm> {
  if (wasmInstance) return wasmInstance;

  const wasmUrl = browser.runtime.getURL("/wasm/zvault_wasm_bg.wasm");
  const glueUrl = browser.runtime.getURL("/wasm/zvault_wasm.js");

  let glueModule: Record<string, unknown>;

  try {
    // Preferred path: dynamic import (works in MV3 service workers and
    // module-based contexts).
    glueModule = await import(/* @vite-ignore */ glueUrl);
  } catch {
    // Fallback for Firefox MV2 background scripts where dynamic import of
    // extension URLs may fail. Fetch the glue JS as text and evaluate it.
    const resp = await fetch(glueUrl);
    const glueText = await resp.text();

    // The wasm-bindgen --target web glue exports an `init` default and named
    // exports.  We create a blob module URL so the browser can parse it as ESM.
    const blob = new Blob([glueText], { type: "application/javascript" });
    const blobUrl = URL.createObjectURL(blob);
    try {
      glueModule = await import(/* @vite-ignore */ blobUrl);
    } finally {
      URL.revokeObjectURL(blobUrl);
    }
  }

  // The default export is the `init` function from wasm-bindgen --target web.
  const init = glueModule.default as (
    input?: string | URL | Request | Response | BufferSource | WebAssembly.Module
  ) => Promise<unknown>;

  // Fetch the WASM binary and instantiate. Using fetch + arrayBuffer ensures
  // compatibility with contexts where streaming instantiation is unavailable.
  const wasmResponse = await fetch(wasmUrl);
  const wasmBytes = await wasmResponse.arrayBuffer();
  await init(wasmBytes);

  wasmInstance = {
    create_vault: glueModule.create_vault as ZVaultWasm["create_vault"],
    open_vault: glueModule.open_vault as ZVaultWasm["open_vault"],
    encrypt_vault: glueModule.encrypt_vault as ZVaultWasm["encrypt_vault"],
    add_item: glueModule.add_item as ZVaultWasm["add_item"],
    list_items: glueModule.list_items as ZVaultWasm["list_items"],
    generate_totp: glueModule.generate_totp as ZVaultWasm["generate_totp"],
    generate_password: glueModule.generate_password as ZVaultWasm["generate_password"],
    validate_relay_url: glueModule.validate_relay_url as ZVaultWasm["validate_relay_url"],
    add_relay_to_vault: glueModule.add_relay_to_vault as ZVaultWasm["add_relay_to_vault"],
    remove_relay_from_vault: glueModule.remove_relay_from_vault as ZVaultWasm["remove_relay_from_vault"],
    toggle_relay_in_vault: glueModule.toggle_relay_in_vault as ZVaultWasm["toggle_relay_in_vault"],
    reset_relays_in_vault: glueModule.reset_relays_in_vault as ZVaultWasm["reset_relays_in_vault"],
    get_enabled_relays: glueModule.get_enabled_relays as ZVaultWasm["get_enabled_relays"],
    encode_npub_from_hex: glueModule.encode_npub_from_hex as ZVaultWasm["encode_npub_from_hex"],
    create_invite_code: glueModule.create_invite_code as ZVaultWasm["create_invite_code"],
    create_join_request_code: glueModule.create_join_request_code as ZVaultWasm["create_join_request_code"],
    decode_pairing_code: glueModule.decode_pairing_code as ZVaultWasm["decode_pairing_code"],
    create_response_code: glueModule.create_response_code as ZVaultWasm["create_response_code"],
    admit_device_from_pairing: glueModule.admit_device_from_pairing as ZVaultWasm["admit_device_from_pairing"],
    // Sync / NIP-44 / NIP-59
    build_full_sync_message: glueModule.build_full_sync_message as ZVaultWasm["build_full_sync_message"],
    apply_sync_message: glueModule.apply_sync_message as ZVaultWasm["apply_sync_message"],
    nip44_encrypt: glueModule.nip44_encrypt as ZVaultWasm["nip44_encrypt"],
    nip44_decrypt: glueModule.nip44_decrypt as ZVaultWasm["nip44_decrypt"],
    gift_wrap: glueModule.gift_wrap as ZVaultWasm["gift_wrap"],
    unwrap_gift_wrap: glueModule.unwrap_gift_wrap as ZVaultWasm["unwrap_gift_wrap"],
    sign_event: glueModule.sign_event as ZVaultWasm["sign_event"],
    verify_event: glueModule.verify_event as ZVaultWasm["verify_event"],
  };

  return wasmInstance;
}

/**
 * Get the initialised WASM instance (throws if not yet initialised).
 */
export function getWasm(): ZVaultWasm {
  if (!wasmInstance) {
    throw new Error("WASM not initialised — call initWasm() first");
  }
  return wasmInstance;
}
