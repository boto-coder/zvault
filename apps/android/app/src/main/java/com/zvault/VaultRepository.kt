package com.zvault

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.UUID

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
 * In production, this class calls the UniFFI-generated Kotlin bindings.
 * The bindings are auto-generated from the Rust UDL definition in
 * `bindings/uniffi/src/zvault.udl`.
 */
class VaultRepository {

    private var vaultHandle: Long? = null
    private var currentDeviceId: String? = null

    /**
     * Create a new vault file at [path] protected by [password].
     * Returns the list of items (empty for a new vault).
     */
    suspend fun createVault(password: String, path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            // TODO: Call UniFFI binding
            // val handle = ZvaultCore.createVault(password, path)
            // vaultHandle = handle
            vaultHandle = 1L
            emptyList()
        }

    /**
     * Open an existing vault file at [path] with [password].
     * Returns the decrypted list of vault items.
     */
    suspend fun openVault(password: String, path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            // TODO: Call UniFFI binding
            // val handle = ZvaultCore.openVault(password, path)
            // vaultHandle = handle
            // return ZvaultCore.listItems(handle).map { it.toVaultItem() }
            vaultHandle = 1L
            emptyList()
        }

    /**
     * Open vault using biometric-wrapped key from Android Keystore.
     */
    suspend fun openVaultWithBiometric(path: String): List<VaultItem> =
        withContext(Dispatchers.IO) {
            // TODO: Retrieve wrapped key from Keystore, unwrap, open vault
            vaultHandle = 1L
            emptyList()
        }

    /**
     * Lock the vault and clear all in-memory sensitive state.
     */
    suspend fun lockVault() = withContext(Dispatchers.IO) {
        // TODO: Call UniFFI binding to drop vault handle (triggers Rust zeroization)
        // ZvaultCore.lockVault(vaultHandle)
        vaultHandle = null
    }

    /**
     * List all items in the currently open vault.
     */
    suspend fun listItems(): List<VaultItem> = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // return ZvaultCore.listItems(handle).map { it.toVaultItem() }
        emptyList()
    }

    /**
     * Get a single item by ID.
     */
    suspend fun getItem(id: String): VaultItem? = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // return ZvaultCore.getItem(handle, id)?.toVaultItem()
        null
    }

    /**
     * Add a new item to the vault.
     */
    suspend fun addItem(item: VaultItem): VaultItem = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // val id = ZvaultCore.addItem(handle, item.toFfi())
        // ZvaultCore.saveVault(handle)
        item.copy(id = UUID.randomUUID().toString())
    }

    /**
     * Update an existing item in the vault.
     */
    suspend fun updateItem(item: VaultItem): VaultItem = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // ZvaultCore.updateItem(handle, item.id, item.toFfi())
        // ZvaultCore.saveVault(handle)
        item
    }

    /**
     * Delete an item by ID.
     */
    suspend fun deleteItem(id: String) = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // ZvaultCore.deleteItem(handle, id)
        // ZvaultCore.saveVault(handle)
    }

    /**
     * List all devices in the vault's trust group.
     */
    suspend fun listDevices(): List<DeviceInfo> = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // return ZvaultCore.listDevices(handle).map { it.toDeviceInfo() }
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
    suspend fun admitDevice(pubkeyHex: String, label: String) = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // ZvaultCore.admitDevice(handle, pubkeyHex, label)
    }

    /**
     * Revoke a device from the vault trust group.
     */
    suspend fun revokeDevice(deviceId: String) = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // ZvaultCore.revokeDevice(handle, deviceId)
    }

    /**
     * Re-key the vault with a new password.
     */
    suspend fun rekeyVault(oldPassword: String, newPassword: String) =
        withContext(Dispatchers.IO) {
            val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
            // TODO: Call UniFFI binding
            // ZvaultCore.rekeyVault(handle, oldPassword, newPassword)
        }

    /**
     * Export vault in .zvault-export encrypted format.
     */
    suspend fun exportVault(exportPath: String, exportPassword: String) =
        withContext(Dispatchers.IO) {
            val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
            // TODO: Call UniFFI binding
            // ZvaultCore.exportVault(handle, exportPath, exportPassword)
        }

    /**
     * Import items from a file (Bitwarden JSON, CSV, etc.).
     */
    suspend fun importItems(importPath: String, format: String): Int =
        withContext(Dispatchers.IO) {
            val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
            // TODO: Call UniFFI binding
            // return ZvaultCore.importItems(handle, importPath, format)
            0
        }

    /**
     * Trigger a Nostr sync cycle.
     */
    suspend fun syncNow() = withContext(Dispatchers.IO) {
        val handle = vaultHandle ?: throw IllegalStateException("Vault not open")
        // TODO: Call UniFFI binding
        // ZvaultCore.syncNow(handle)
    }

    val isVaultOpen: Boolean
        get() = vaultHandle != null
}
