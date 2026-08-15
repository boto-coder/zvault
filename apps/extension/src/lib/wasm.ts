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
