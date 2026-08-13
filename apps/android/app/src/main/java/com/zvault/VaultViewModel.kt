package com.zvault

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * Sealed interface representing the vault UI state.
 */
sealed interface VaultUiState {
    /** No vault is open; show unlock/create screen. */
    data object Locked : VaultUiState

    /** Vault is being unlocked (Argon2id derivation in progress). */
    data object Unlocking : VaultUiState

    /** Vault is unlocked; display items. */
    data class Unlocked(val itemsJson: String = "[]") : VaultUiState

    /** An error occurred. */
    data class Error(val message: String) : VaultUiState
}

/**
 * ViewModel bridging the Jetpack Compose UI to zvault-core via UniFFI bindings.
 *
 * All vault operations (create, open, save, CRUD) are performed on a background
 * coroutine to avoid blocking the main thread during Argon2id key derivation.
 */
class VaultViewModel : ViewModel() {

    private val _uiState = MutableStateFlow<VaultUiState>(VaultUiState.Locked)

    /** Observable UI state for the Compose layer. */
    val uiState: StateFlow<VaultUiState> = _uiState.asStateFlow()

    /**
     * Attempt to create a new vault at [path] with [password].
     */
    fun createVault(password: String, path: String) {
        _uiState.value = VaultUiState.Unlocking
        viewModelScope.launch {
            try {
                // TODO(M10): Call UniFFI binding — zvault.createVault(password, path)
                _uiState.value = VaultUiState.Unlocked()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Unknown error")
            }
        }
    }

    /**
     * Attempt to open an existing vault at [path] with [password].
     */
    fun openVault(password: String, path: String) {
        _uiState.value = VaultUiState.Unlocking
        viewModelScope.launch {
            try {
                // TODO(M10): Call UniFFI binding — zvault.openVault(password, path)
                // val handle = zvault.openVault(password, path)
                // val items = zvault.listItems(handle)
                _uiState.value = VaultUiState.Unlocked()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Unknown error")
            }
        }
    }

    /**
     * Lock the vault, clearing all in-memory state.
     */
    fun lockVault() {
        // TODO(M10): Drop the vault handle to trigger Rust-side zeroization
        _uiState.value = VaultUiState.Locked
    }

    /**
     * Add an item to the vault.
     */
    fun addItem(itemJson: String) {
        viewModelScope.launch {
            try {
                // TODO(M10): Call UniFFI binding — zvault.addItem(handle, itemJson)
                refreshItems()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Unknown error")
            }
        }
    }

    /**
     * Delete an item from the vault by UUID.
     */
    fun deleteItem(itemId: String) {
        viewModelScope.launch {
            try {
                // TODO(M10): Call UniFFI binding — zvault.deleteItem(handle, itemId)
                refreshItems()
            } catch (e: Exception) {
                _uiState.value = VaultUiState.Error(e.message ?: "Unknown error")
            }
        }
    }

    private fun refreshItems() {
        // TODO(M10): Call UniFFI binding — zvault.listItems(handle)
        _uiState.value = VaultUiState.Unlocked(itemsJson = "[]")
    }
}
