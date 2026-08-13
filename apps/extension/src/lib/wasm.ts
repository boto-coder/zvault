/**
 * WASM bridge for zvault-core.
 *
 * Loads the zvault-wasm module and exposes typed wrappers for vault operations.
 * The WASM binary is loaded lazily on first use from the extension's bundled assets.
 */

export interface ZVaultWasm {
  create_vault(password: string): Uint8Array;
  open_vault(password: string, data: Uint8Array): string;
  encrypt_vault(password: string, vault_json: string): Uint8Array;
  add_item(vault_json: string, item_json: string): string;
  list_items(vault_json: string): unknown[];
  generate_totp(secret: string): string;
}

let wasmInstance: ZVaultWasm | null = null;

/**
 * Initialise the WASM module. Must be called before any vault operations.
 * In MV3 extensions, call this in the service worker on startup.
 */
export async function initWasm(): Promise<ZVaultWasm> {
  if (wasmInstance) return wasmInstance;

  // Fetch the wasm-bindgen JS glue from the extension bundle.
  // In a bundled extension, these files live under /wasm/ in the output dir.
  const wasmUrl = browser.runtime.getURL("/wasm/zvault_wasm_bg.wasm");

  // Dynamic import of the wasm-bindgen glue.
  // The glue JS is included in the extension bundle at build time.
  const glueUrl = browser.runtime.getURL("/wasm/zvault_wasm.js");
  const glueModule = await import(/* @vite-ignore */ glueUrl);

  // Initialise with the WASM binary URL.
  await glueModule.default(wasmUrl);

  wasmInstance = {
    create_vault: glueModule.create_vault,
    open_vault: glueModule.open_vault,
    encrypt_vault: glueModule.encrypt_vault,
    add_item: glueModule.add_item,
    list_items: glueModule.list_items,
    generate_totp: glueModule.generate_totp,
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
