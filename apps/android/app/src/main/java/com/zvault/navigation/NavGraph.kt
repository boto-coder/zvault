package com.zvault.navigation

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import com.zvault.VaultUiState
import com.zvault.VaultViewModel
import com.zvault.ui.screens.AddItemScreen
import com.zvault.ui.screens.DevicesScreen
import com.zvault.ui.screens.ItemDetailScreen
import com.zvault.ui.screens.SettingsScreen
import com.zvault.ui.screens.UnlockScreen
import com.zvault.ui.screens.VaultListScreen

/**
 * Navigation route constants.
 */
object Routes {
    const val UNLOCK = "unlock"
    const val VAULT_LIST = "vault_list"
    const val ITEM_DETAIL = "item_detail/{itemId}"
    const val ADD_ITEM = "add_item"
    const val DEVICES = "devices"
    const val SETTINGS = "settings"

    fun itemDetail(itemId: String) = "item_detail/$itemId"
}

/**
 * Main navigation graph for the ZVault app.
 *
 * Routes between screens based on vault state:
 * - Locked → UnlockScreen
 * - Unlocked → VaultListScreen → ItemDetail / AddItem / Devices / Settings
 */
@Composable
fun ZVaultNavGraph(
    viewModel: VaultViewModel = viewModel(),
    navController: NavHostController = rememberNavController(),
    modifier: Modifier = Modifier,
) {
    val uiState by viewModel.uiState.collectAsState()
    val items by viewModel.items.collectAsState()
    val devices by viewModel.devices.collectAsState()
    val selectedItem by viewModel.selectedItem.collectAsState()
    val biometricEnabled by viewModel.biometricEnabled.collectAsState()
    val biometricAvailable by viewModel.biometricAvailable.collectAsState()

    // Navigate based on auth state
    val startDestination = when (uiState) {
        is VaultUiState.Locked, is VaultUiState.Unlocking, is VaultUiState.Error -> Routes.UNLOCK
        is VaultUiState.Unlocked -> Routes.VAULT_LIST
    }

    NavHost(
        navController = navController,
        startDestination = startDestination,
        modifier = modifier,
    ) {
        composable(Routes.UNLOCK) {
            UnlockScreen(
                isUnlocking = uiState is VaultUiState.Unlocking,
                errorMessage = (uiState as? VaultUiState.Error)?.message,
                biometricAvailable = biometricAvailable,
                onUnlock = { password ->
                    viewModel.openVault(password)
                },
                onCreateVault = { password ->
                    viewModel.createVault(password)
                },
                onBiometricUnlock = {
                    viewModel.biometricUnlock()
                },
            )
        }

        composable(Routes.VAULT_LIST) {
            VaultListScreen(
                items = items,
                onItemClick = { item ->
                    viewModel.selectItem(item.id)
                    navController.navigate(Routes.itemDetail(item.id))
                },
                onAddClick = {
                    navController.navigate(Routes.ADD_ITEM)
                },
                onLockClick = {
                    viewModel.lockVault()
                    navController.navigate(Routes.UNLOCK) {
                        popUpTo(0) { inclusive = true }
                    }
                },
                onSettingsClick = {
                    navController.navigate(Routes.SETTINGS)
                },
                onDevicesClick = {
                    viewModel.loadDevices()
                    navController.navigate(Routes.DEVICES)
                },
                onSyncClick = {
                    viewModel.syncNow()
                },
            )
        }

        composable(Routes.ITEM_DETAIL) {
            val item = selectedItem
            if (item != null) {
                ItemDetailScreen(
                    item = item,
                    onSave = { updatedItem ->
                        viewModel.updateItem(updatedItem)
                    },
                    onDelete = { itemId ->
                        viewModel.deleteItem(itemId)
                        navController.popBackStack()
                    },
                    onBack = {
                        navController.popBackStack()
                    },
                )
            }
        }

        composable(Routes.ADD_ITEM) {
            AddItemScreen(
                onSave = { item ->
                    viewModel.addItem(item)
                    navController.popBackStack()
                },
                onCancel = {
                    navController.popBackStack()
                },
            )
        }

        composable(Routes.DEVICES) {
            DevicesScreen(
                devices = devices,
                onAdmitDevice = { pubkey, label ->
                    viewModel.admitDevice(pubkey, label)
                },
                onRevokeDevice = { deviceId ->
                    viewModel.revokeDevice(deviceId)
                },
                onBack = {
                    navController.popBackStack()
                },
            )
        }

        composable(Routes.SETTINGS) {
            SettingsScreen(
                biometricEnabled = biometricEnabled,
                biometricAvailable = biometricAvailable,
                onBiometricToggle = { enabled ->
                    viewModel.setBiometricEnabled(enabled)
                },
                onExportVault = {
                    viewModel.exportVault()
                },
                onImportVault = {
                    viewModel.importVault()
                },
                onRekeyVault = { oldPw, newPw ->
                    viewModel.rekeyVault(oldPw, newPw)
                },
                onBack = {
                    navController.popBackStack()
                },
            )
        }
    }
}
