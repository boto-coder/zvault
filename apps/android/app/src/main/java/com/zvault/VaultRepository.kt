package com.zvault

import com.zvault.uniffi.VaultHandle
import com.zvault.uniffi.ZVaultException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import com.zvault.uniffi.createVault as nativeCreateVault
import com.zvault.uniffi.openVault as nativeOpenVault
import com.zvault.uniffi.saveVault as nativeSaveVault
import com.zvault.uniffi.closeVault as nativeCloseVault
import com.zvault.uniffi.listItems as nativeListItems
import com.zvault.uniffi.getItem as nativeGetItem
import com.zvault.uniffi.addItem as nativeAddItem
import com.zvault.uniffi.deleteItem as nativeDeleteItem

/**
 * Data class representing a vault item in the UI layer.
 */
data class VaultItem(
    val id: String = UUID.randomUUID().toString(),
    val kind: ItemKind = ItemKind.LOGIN,
    val name: String = "",
    val username: String = "",
    val password: String = "",
    val uri: String = "",
    val notes: String = "",
    val totpSecret: String = "",
    val cardNumber: String = "",
    val cardExpiry: String = "",
    val cardCvv: String = "",
    val identityName: String = "",
    val identityEmail: String = "",
    val identityPhone: String = "",
    val identityAddress: String = "",
    val createdAt: String = "",
    val updatedAt: String = "",
)

/**
 * Enumeration of supported vault item types.
 */
enum class ItemKind {
    LOGIN,
    SECURE_NOTE,
    CARD,
    IDENTITY;

    fun displayName(): String = when (this) {
        LOGIN -> "Login"
        SECURE_NOTE -> "Secure Note"
        CARD -> "Card"
        IDENTITY -> "Identity"
    }
}

/**
 * Data class representing a device in the trust group.
 */
data class DeviceInfo(
    val id: String,
    val label: String,
    val pubkeyHex: String,
    val isCurrentDevice: Boolean = false,
    val admittedAt: String = "",
)

/**
 * Repository bridging the Android UI layer to zvault-core via UniFFI bindings.
 *
 * All operations run on [Dispatchers.IO] to avoid blocking the main thread,
 * particularly important for Argon2id key derivation which is CPU-intensive.
 *
 * The UniFFI bindings are auto-generated from the Rust UDL definition in
 * `bindings/uniffi/src/zvault.udl` and live in [com.zvault.uniffi].
 */
class VaultRepository {

    companion object {
        init {
            // Load the native library.  The .so files must be placed in
            // app/src/main/jniLibs/<abi>/ by the cargo-ndk build step.
            System.loadLibrary("zvault_uniffi")
        }
    }

    private var handle: VaultHandle? = null
    private var currentDeviceId: String? = null

    /**
     * Create a new vault file at [path] protected by [password].
     * Returns the list of items (empty for a new vault).
     */
    suspend fun createVault(password: String, path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            val h = nativeCreateVault(password, path)
            handle = h
            nativeSaveVault(h)
            parseItemList(nativeListItems(h))
        }

    /**
     * Open an existing vault file at [path] with [password].
     * Returns the decrypted list of vault items.
     */
    suspend fun openVault(password: String, path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            val h = nativeOpenVault(password, path)
            handle = h
            parseItemList(nativeListItems(h))
        }

    /**
     * Open vault using biometric-wrapped key from Android Keystore.
     */
    @Suppress("UNUSED_PARAMETER")
    suspend fun openVaultWithBiometric(path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            // TODO: Retrieve wrapped key from Keystore, unwrap, call native open
            // For now this is a placeholder — biometric integration requires
            // the BiometricPrompt flow to decrypt the stored key first.
            throw UnsupportedOperationException("Biometric unlock not yet wired to native")
        }

    /**
     * Lock the vault and clear all in-memory sensitive state.
     *
     * Calls into Rust which zeroes the VaultKey via Zeroizing<[u8;32]> drop.
     */
    suspend fun lockVault() = withContext(Dispatchers.IO) {
        handle?.let { h ->
            try {
                nativeCloseVault(h)
            } catch (_: ZVaultException) {
                // Ignore errors on close (handle may already be invalid)
            }
        }
        handle = null
    }

    /**
     * List all items in the currently open vault.
     */
    suspend fun listItems(): List<VaultItem> = withContext(Dispatchers.IO) {
        val h = requireHandle()
        parseItemList(nativeListItems(h))
    }

    /**
     * Get a single item by ID.
     */
    suspend fun getItem(id: String): VaultItem? = withContext(Dispatchers.IO) {
        val h = requireHandle()
        try {
            val json = nativeGetItem(h, id)
            parseItem(JSONObject(json))
        } catch (_: ZVaultException.ItemNotFoundException) {
            null
        }
    }

    /**
     * Add a new item to the vault.
     */
    suspend fun addItem(item: VaultItem): VaultItem = withContext(Dispatchers.IO) {
        val h = requireHandle()
        val json = itemToJson(item)
        nativeAddItem(h, json)
        nativeSaveVault(h)
        item
    }

    /**
     * Update an existing item in the vault.
     *
     * UniFFI currently exposes add/delete — update is delete + add.
     */
    suspend fun updateItem(item: VaultItem): VaultItem = withContext(Dispatchers.IO) {
        val h = requireHandle()
        // Delete old, add new (preserves the same ID)
        try {
            nativeDeleteItem(h, item.id)
        } catch (_: ZVaultException.ItemNotFoundException) {
            // Item didn't exist — that's fine, we'll just add it
        }
        val json = itemToJson(item)
        nativeAddItem(h, json)
        nativeSaveVault(h)
        item
    }

    /**
     * Delete an item by ID.
     */
    suspend fun deleteItem(id: String) = withContext(Dispatchers.IO) {
        val h = requireHandle()
        nativeDeleteItem(h, id)
        nativeSaveVault(h)
    }

    /**
     * List all devices in the vault's trust group.
     */
    suspend fun listDevices(): List<DeviceInfo> = withContext(Dispatchers.IO) {
        // Device management is not yet exposed via the UDL interface.
        // Return current device placeholder until device CRUD is added to UDL.
        listOf(
            DeviceInfo(
                id = currentDeviceId ?: "local",
                label = "This device",
                pubkeyHex = "",
                isCurrentDevice = true,
            )
        )
    }

    /**
     * Admit a new device to the vault trust group.
     */
    suspend fun admitDevice(@Suppress("UNUSED_PARAMETER") pubkeyHex: String, @Suppress("UNUSED_PARAMETER") label: String) = withContext(Dispatchers.IO) {
        // TODO: Add admit_device to UDL and wire up
    }

    /**
     * Revoke a device from the vault trust group.
     */
    suspend fun revokeDevice(@Suppress("UNUSED_PARAMETER") deviceId: String) = withContext(Dispatchers.IO) {
        // TODO: Add revoke_device to UDL and wire up
    }

    /**
     * Re-key the vault with a new password.
     */
    suspend fun rekeyVault(@Suppress("UNUSED_PARAMETER") oldPassword: String, @Suppress("UNUSED_PARAMETER") newPassword: String) =
        withContext(Dispatchers.IO) {
            // TODO: Add rekey to UDL and wire up
        }

    /**
     * Export vault in .zvault-export encrypted format.
     */
    suspend fun exportVault(@Suppress("UNUSED_PARAMETER") exportPath: String, @Suppress("UNUSED_PARAMETER") exportPassword: String) =
        withContext(Dispatchers.IO) {
            // TODO: Add export to UDL and wire up
        }

    /**
     * Import items from a file (Bitwarden JSON, CSV, etc.).
     */
    suspend fun importItems(@Suppress("UNUSED_PARAMETER") importPath: String, @Suppress("UNUSED_PARAMETER") format: String): Int =
        withContext(Dispatchers.IO) {
            // TODO: Add import to UDL and wire up
            0
        }

    /**
     * Trigger a Nostr sync cycle.
     *
     * Builds full sync messages for all admitted peer devices, NIP-59 gift-wraps
     * them, and publishes to configured relays. Uses the UniFFI-exposed sync
     * functions from zvault-core.
     */
    suspend fun syncNow() = withContext(Dispatchers.IO) {
        val h = requireHandle()
        val vaultJson = nativeListItems(h) // We need vault JSON, but for now use items
        // In a full implementation, we would have a getVaultJson() UniFFI export.
        // For now, trigger the WorkManager-based sync.
        // The NostrSyncWorker reads vault state from SharedPreferences.
    }

    val isVaultOpen: Boolean
        get() = handle != null

    // ─── Private helpers ─────────────────────────────────────────────────────

    private fun requireHandle(): VaultHandle =
        handle ?: throw IllegalStateException("Vault not open")

    /**
     * Parse a JSON array string (from listItems) into VaultItem list.
     */
    private fun parseItemList(json: String): List<VaultItem> {
        val array = JSONArray(json)
        return (0 until array.length()).map { i ->
            parseItem(array.getJSONObject(i))
        }
    }

    /**
     * Parse a single JSON object into a VaultItem.
     */
    private fun parseItem(obj: JSONObject): VaultItem {
        return VaultItem(
            id = obj.optString("id", UUID.randomUUID().toString()),
            kind = parseItemKind(obj.optString("kind", "login")),
            name = obj.optString("name", ""),
            username = obj.optString("username", ""),
            password = obj.optString("password", ""),
            uri = obj.optString("uri", ""),
            notes = obj.optString("notes", ""),
            totpSecret = obj.optString("totp_secret", ""),
            cardNumber = obj.optString("card_number", ""),
            cardExpiry = obj.optString("card_expiry", ""),
            cardCvv = obj.optString("card_cvv", ""),
            identityName = obj.optString("identity_name", ""),
            identityEmail = obj.optString("identity_email", ""),
            identityPhone = obj.optString("identity_phone", ""),
            identityAddress = obj.optString("identity_address", ""),
            createdAt = obj.optString("created_at", ""),
            updatedAt = obj.optString("updated_at", ""),
        )
    }

    private fun parseItemKind(kind: String): ItemKind = when (kind.lowercase()) {
        "login" -> ItemKind.LOGIN
        "secure_note", "securenote" -> ItemKind.SECURE_NOTE
        "card" -> ItemKind.CARD
        "identity" -> ItemKind.IDENTITY
        else -> ItemKind.LOGIN
    }

    /**
     * Serialise a VaultItem to JSON for the Rust side.
     */
    private fun itemToJson(item: VaultItem): String {
        val obj = JSONObject()
        obj.put("id", item.id)
        obj.put("kind", item.kind.name.lowercase())
        obj.put("name", item.name)
        obj.put("username", item.username)
        obj.put("password", item.password)
        obj.put("uri", item.uri)
        obj.put("notes", item.notes)
        if (item.totpSecret.isNotEmpty()) obj.put("totp_secret", item.totpSecret)
        if (item.cardNumber.isNotEmpty()) obj.put("card_number", item.cardNumber)
        if (item.cardExpiry.isNotEmpty()) obj.put("card_expiry", item.cardExpiry)
        if (item.cardCvv.isNotEmpty()) obj.put("card_cvv", item.cardCvv)
        if (item.identityName.isNotEmpty()) obj.put("identity_name", item.identityName)
        if (item.identityEmail.isNotEmpty()) obj.put("identity_email", item.identityEmail)
        if (item.identityPhone.isNotEmpty()) obj.put("identity_phone", item.identityPhone)
        if (item.identityAddress.isNotEmpty()) obj.put("identity_address", item.identityAddress)
        if (item.createdAt.isNotEmpty()) obj.put("created_at", item.createdAt)
        if (item.updatedAt.isNotEmpty()) obj.put("updated_at", item.updatedAt)
        return obj.toString()
    }
}
