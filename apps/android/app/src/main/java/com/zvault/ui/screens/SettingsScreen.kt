package com.zvault.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Divider
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
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
import androidx.compose.ui.unit.dp
import com.zvault.ui.components.PasswordField

/**
 * Settings screen — biometric toggle, export/import, re-key password,
 * and other vault configuration options.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    biometricEnabled: Boolean,
    biometricAvailable: Boolean,
    onBiometricToggle: (Boolean) -> Unit,
    onExportVault: () -> Unit,
    onImportVault: () -> Unit,
    onRekeyVault: (oldPassword: String, newPassword: String) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var showRekeyDialog by remember { mutableStateOf(false) }

    if (showRekeyDialog) {
        RekeyDialog(
            onConfirm = { oldPw, newPw ->
                onRekeyVault(oldPw, newPw)
                showRekeyDialog = false
            },
            onDismiss = { showRekeyDialog = false },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Text("←")
                    }
                },
            )
        },
        modifier = modifier,
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState()),
        ) {
            // Security section
            SettingsSectionHeader("Security")

            // Biometric unlock
            SettingsRow(
                title = "Biometric Unlock",
                subtitle = if (biometricAvailable) {
                    "Use fingerprint or face to unlock vault"
                } else {
                    "Biometric hardware not available"
                },
                trailing = {
                    Switch(
                        checked = biometricEnabled,
                        onCheckedChange = onBiometricToggle,
                        enabled = biometricAvailable,
                    )
                },
            )

            Divider(modifier = Modifier.padding(horizontal = 16.dp))

            // Change password
            SettingsRow(
                title = "Change Master Password",
                subtitle = "Re-key the vault with a new password",
                onClick = { showRekeyDialog = true },
            )

            Divider(modifier = Modifier.padding(horizontal = 16.dp))

            Spacer(modifier = Modifier.height(24.dp))

            // Data section
            SettingsSectionHeader("Data")

            SettingsRow(
                title = "Export Vault",
                subtitle = "Export as encrypted .zvault-export backup",
                onClick = onExportVault,
            )

            Divider(modifier = Modifier.padding(horizontal = 16.dp))

            SettingsRow(
                title = "Import",
                subtitle = "Import from Bitwarden, 1Password, LastPass, CSV, or KDBX",
                onClick = onImportVault,
            )

            Divider(modifier = Modifier.padding(horizontal = 16.dp))

            Spacer(modifier = Modifier.height(24.dp))

            // About section
            SettingsSectionHeader("About")

            SettingsRow(
                title = "ZVault",
                subtitle = "v1.0.0 — Local-first encrypted password manager",
            )

            Divider(modifier = Modifier.padding(horizontal = 16.dp))

            SettingsRow(
                title = "Encryption",
                subtitle = "Argon2id (KDF) + AES-256-GCM (at rest)\nNIP-44 XChaCha20-Poly1305 (sync)",
            )

            Spacer(modifier = Modifier.height(32.dp))
        }
    }
}

@Composable
private fun SettingsSectionHeader(title: String) {
    Text(
        text = title,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
    )
}

@Composable
private fun SettingsRow(
    title: String,
    subtitle: String,
    onClick: (() -> Unit)? = null,
    trailing: @Composable (() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
            )
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (trailing != null) {
            Spacer(modifier = Modifier.width(12.dp))
            trailing()
        }
    }
}

@Composable
private fun RekeyDialog(
    onConfirm: (oldPassword: String, newPassword: String) -> Unit,
    onDismiss: () -> Unit,
) {
    var oldPassword by remember { mutableStateOf("") }
    var newPassword by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Change Master Password") },
        text = {
            Column {
                Text(
                    text = "This will re-encrypt your vault with a new password. " +
                        "The operation cannot be undone.",
                    style = MaterialTheme.typography.bodyMedium,
                )
                Spacer(modifier = Modifier.height(16.dp))
                PasswordField(
                    value = oldPassword,
                    onValueChange = { oldPassword = it },
                    label = "Current Password",
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(modifier = Modifier.height(8.dp))
                PasswordField(
                    value = newPassword,
                    onValueChange = { newPassword = it },
                    label = "New Password",
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(modifier = Modifier.height(8.dp))
                PasswordField(
                    value = confirmPassword,
                    onValueChange = { confirmPassword = it },
                    label = "Confirm New Password",
                    modifier = Modifier.fillMaxWidth(),
                    isError = confirmPassword.isNotEmpty() && newPassword != confirmPassword,
                    supportingText = if (confirmPassword.isNotEmpty() && newPassword != confirmPassword) {
                        { Text("Passwords do not match") }
                    } else null,
                )
            }
        },
        confirmButton = {
            Button(
                onClick = { onConfirm(oldPassword, newPassword) },
                enabled = oldPassword.isNotBlank() &&
                    newPassword.isNotBlank() &&
                    newPassword == confirmPassword,
            ) {
                Text("Change Password")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Cancel")
            }
        },
    )
}
