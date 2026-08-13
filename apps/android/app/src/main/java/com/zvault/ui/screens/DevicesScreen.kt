package com.zvault.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zvault.DeviceInfo

/**
 * Devices screen — manages the trust group of devices that can sync the vault.
 *
 * Shows the list of admitted devices with their labels, public keys, and
 * admission timestamps. Supports admitting new devices (via pubkey) and
 * revoking existing ones.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DevicesScreen(
    devices: List<DeviceInfo>,
    onAdmitDevice: (pubkeyHex: String, label: String) -> Unit,
    onRevokeDevice: (deviceId: String) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var showAdmitDialog by remember { mutableStateOf(false) }
    var showRevokeDialog by remember { mutableStateOf<DeviceInfo?>(null) }

    if (showAdmitDialog) {
        AdmitDeviceDialog(
            onConfirm = { pubkey, label ->
                onAdmitDevice(pubkey, label)
                showAdmitDialog = false
            },
            onDismiss = { showAdmitDialog = false },
        )
    }

    showRevokeDialog?.let { device ->
        AlertDialog(
            onDismissRequest = { showRevokeDialog = null },
            title = { Text("Revoke Device") },
            text = {
                Text(
                    "Revoke \"${device.label}\"? This device will no longer " +
                        "receive vault updates and its future sync messages will be rejected."
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        onRevokeDevice(device.id)
                        showRevokeDialog = null
                    }
                ) {
                    Text("Revoke", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showRevokeDialog = null }) {
                    Text("Cancel")
                }
            },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Devices") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Text("←")
                    }
                },
                actions = {
                    TextButton(onClick = { showAdmitDialog = true }) {
                        Text("Admit")
                    }
                },
            )
        },
        modifier = modifier,
    ) { padding ->
        if (devices.isEmpty()) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding)
                    .padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.Center,
            ) {
                Text("📱", style = MaterialTheme.typography.displayMedium)
                Spacer(modifier = Modifier.height(16.dp))
                Text(
                    text = "No devices in trust group",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(16.dp),
                modifier = Modifier
                    .fillMaxSize()
                    .padding(padding),
            ) {
                items(devices, key = { it.id }) { device ->
                    DeviceCard(
                        device = device,
                        onRevoke = { showRevokeDialog = device },
                    )
                    Spacer(modifier = Modifier.height(8.dp))
                }
            }
        }
    }
}

@Composable
private fun DeviceCard(
    device: DeviceInfo,
    onRevoke: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier.fillMaxWidth(),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = device.label,
                        style = MaterialTheme.typography.bodyLarge,
                    )
                    if (device.isCurrentDevice) {
                        Spacer(modifier = Modifier.width(8.dp))
                        Text(
                            text = "(this device)",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
                Spacer(modifier = Modifier.height(4.dp))
                Text(
                    text = device.pubkeyHex.take(16) + "…",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (device.admittedAt.isNotBlank()) {
                    Text(
                        text = "Admitted: ${device.admittedAt}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            if (!device.isCurrentDevice) {
                OutlinedButton(onClick = onRevoke) {
                    Text("Revoke", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}

@Composable
private fun AdmitDeviceDialog(
    onConfirm: (pubkeyHex: String, label: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var pubkeyHex by remember { mutableStateOf("") }
    var label by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Admit Device") },
        text = {
            Column {
                Text(
                    text = "Enter the public key and label of the device to admit to this vault's trust group.",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Spacer(modifier = Modifier.height(16.dp))
                OutlinedTextField(
                    value = pubkeyHex,
                    onValueChange = { pubkeyHex = it },
                    label = { Text("Public Key (hex)") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                )
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it },
                    label = { Text("Device Label") },
                    modifier = Modifier.fillMaxWidth(),
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(pubkeyHex, label) },
                enabled = pubkeyHex.isNotBlank() && label.isNotBlank(),
            ) {
                Text("Admit")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        },
    )
}
