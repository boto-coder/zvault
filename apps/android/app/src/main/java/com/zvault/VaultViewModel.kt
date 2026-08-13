package com.zvault

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File

/**
 * Sealed interface representing the vault UI state.
 */
sealed interface VaultUiState {
    /** No vault is open; show unlock/create screen. */
    data object Locked : VaultUiState

    /** Vault is being unlocked (Argon2id derivation in progress). */
    data object Unlocking : VaultUiState

    /** Vault is unlocked; display items. */
    data object Unlocked : VaultUiState

    /** An error occurred. */
    data class Error(val message: String) : VaultUiState
}

/**
 * ViewModel bridging the Jetpack Compose UI to zvault-core via UniFFI bindings.
 *
 * Follows the MVVM pattern:
 * - UI observes [uiState], [items], [devices], and other StateFlows
 * - User actions call ViewModel methods
 * - ViewModel delegates to [VaultRepository] on background coroutines
 * - StateFlows are updated from results, triggering recomposition
 *
 * All vault operations (create, open, save, CRUD) are performed on
 * [kotlinx.coroutines.Dispatchers.IO] to avoid blocking the main thread
 * during Argon2id key derivation.
 */
class VaultViewModel(application: Application) : AndroidViewModel(application) {

    private val repository = VaultRepository()

    private val _uiState = MutableStateFlow<VaultUiState>(VaultUiState.Locked)
    val uiState: StateFlow<VaultUiState> = _uiState.asStateFlow()

    private val _items = MutableStateFlow<List<VaultItem>>(emptyList())
    val items: StateFlow<List<VaultItem>> = _items.asStateFlow()

    private val _selectedItem = MutableStateFlow<VaultItem?>(null)
    val selectedItem: StateFlow<VaultItem?> = _selectedItem.asStateFlow()

    private val _devices = MutableStateFlow<List<DeviceInfo>>(emptyList())
    val devices: StateFlow<List<DeviceInfo>> = _devices.asStateFlow()

    private val _biometricEnabled = MutableStateFlow(false)
    val biometricEnabled: StateFlow<Boolean> = _biometricEnabled.asStateFlow()

    private val _biometricAvailable = MutableStateFlow(false)
    val biometricAvailable: StateFlow<Boolean> = _biometricAvailable.asStateFlow()

    /** Default vault file path in app-internal storage. */
    private val vaultPath: String
        get() {
            val dir = getApplication<Application>().filesDir
            return File(dir, "vault.zvault").absolutePath
        }

    init {
        checkBiometricAvailability()
    }

    // --- Vault lifecycle ---

    /**
     * Create a new vault with [password].
     */
    fun createVault(password: String) {
        _uiState.value = VaultUiState.Unlocking
        viewModelScope.launch {
            try {
                val items = repository.createVault(password, vaultPath)
                _items.value = items
                _uiState.value = VaultUiState.Unlocked
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to create vault")
            }
        }
    }

    /**
     * Open an existing vault with [password].
     */
    fun openVault(password: String) {
        _uiState.value = VaultUiState.Unlocking
        viewModelScope.launch {
            try {
                val items = repository.openVault(password, vaultPath)
                _items.value = items
                _uiState.value = VaultUiState.Unlocked
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to open vault")
            }
        }
    }

    /**
     * Unlock vault using biometric-wrapped key.
     */
    fun biometricUnlock() {
        _uiState.value = VaultUiState.Unlocking
        viewModelScope.launch {
            try {
                val items = repository.openVaultWithBiometric(vaultPath)
                _items.value = items
                _uiState.value = VaultUiState.Unlocked
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Biometric unlock failed")
            }
        }
    }

    /**
     * Lock the vault and clear all in-memory state.
     */
    fun lockVault() {
        viewModelScope.launch {
            repository.lockVault()
            _items.value = emptyList()
            _selectedItem.value = null
            _devices.value = emptyList()
            _uiState.value = VaultUiState.Locked
        }
    }

    // --- Item CRUD ---

    /**
     * Add a new item to the vault.
     */
    fun addItem(item: VaultItem) {
        viewModelScope.launch {
            try {
                val created = repository.addItem(item)
                _items.value = _items.value + created
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to add item")
            }
        }
    }

    /**
     * Update an existing item.
     */
    fun updateItem(item: VaultItem) {
        viewModelScope.launch {
            try {
                val updated = repository.updateItem(item)
                _items.value = _items.value.map { if (it.id == updated.id) updated else it }
                _selectedItem.value = updated
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to update item")
            }
        }
    }

    /**
     * Delete an item by ID.
     */
    fun deleteItem(itemId: String) {
        viewModelScope.launch {
            try {
                repository.deleteItem(itemId)
                _items.value = _items.value.filter { it.id != itemId }
                if (_selectedItem.value?.id == itemId) {
                    _selectedItem.value = null
                }
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to delete item")
            }
        }
    }

    /**
     * Select an item for detail view.
     */
    fun selectItem(itemId: String) {
        _selectedItem.value = _items.value.find { it.id == itemId }
    }

    // --- Device management ---

    /**
     * Load the device list from the vault.
     */
    fun loadDevices() {
        viewModelScope.launch {
            try {
                _devices.value = repository.listDevices()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to load devices")
            }
        }
    }

    /**
     * Admit a new device to the trust group.
     */
    fun admitDevice(pubkeyHex: String, label: String) {
        viewModelScope.launch {
            try {
                repository.admitDevice(pubkeyHex, label)
                loadDevices()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to admit device")
            }
        }
    }

    /**
     * Revoke a device from the trust group.
     */
    fun revokeDevice(deviceId: String) {
        viewModelScope.launch {
            try {
                repository.revokeDevice(deviceId)
                _devices.value = _devices.value.filter { it.id != deviceId }
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to revoke device")
            }
        }
    }

    // --- Settings ---

    /**
     * Toggle biometric unlock enrollment.
     */
    fun setBiometricEnabled(enabled: Boolean) {
        viewModelScope.launch {
            // TODO: If enabling, wrap current VaultKey with Android Keystore biometric-bound key
            // If disabling, remove the wrapped key from Keystore
            _biometricEnabled.value = enabled
        }
    }

    /**
     * Re-key the vault with a new password.
     */
    fun rekeyVault(oldPassword: String, newPassword: String) {
        viewModelScope.launch {
            try {
                repository.rekeyVault(oldPassword, newPassword)
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Failed to change password")
            }
        }
    }

    /**
     * Export vault to .zvault-export format.
     */
    fun exportVault() {
        viewModelScope.launch {
            try {
                val exportDir = getApplication<Application>().getExternalFilesDir(null)
                val exportPath = File(exportDir, "vault.zvault-export").absolutePath
                repository.exportVault(exportPath, "") // TODO: prompt for export password
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Export failed")
            }
        }
    }

    /**
     * Import items from external format.
     */
    fun importVault() {
        viewModelScope.launch {
            try {
                // TODO: Launch file picker, then call repository.importItems(path, format)
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Import failed")
            }
        }
    }

    // --- Sync ---

    /**
     * Trigger a Nostr sync cycle.
     */
    fun syncNow() {
        viewModelScope.launch {
            try {
                repository.syncNow()
                // Refresh items after sync
                _items.value = repository.listItems()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Sync failed")
            }
        }
    }

    // --- Internal ---

    private fun checkBiometricAvailability() {
        // TODO: Use BiometricManager.canAuthenticate() to check hardware availability
        // val biometricManager = BiometricManager.from(getApplication())
        // _biometricAvailable.value = biometricManager.canAuthenticate(BIOMETRIC_STRONG) == BIOMETRIC_SUCCESS
        _biometricAvailable.value = true // Assume available; real check requires context
    }
}
