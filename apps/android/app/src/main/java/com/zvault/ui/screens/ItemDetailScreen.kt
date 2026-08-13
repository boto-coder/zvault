package com.zvault.ui.screens

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
import androidx.compose.material3.ButtonDefaults
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.zvault.ItemKind
import com.zvault.VaultItem
import com.zvault.ui.components.PasswordField

/**
 * Item detail screen for viewing and editing a vault item.
 *
 * Supports:
 * - Viewing item fields with password visibility toggle
 * - Editing mode for updating fields
 * - Copy-to-clipboard for credentials
 * - Delete with confirmation dialog
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ItemDetailScreen(
    item: VaultItem,
    onSave: (VaultItem) -> Unit,
    onDelete: (String) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var isEditing by remember { mutableStateOf(false) }
    var showDeleteDialog by remember { mutableStateOf(false) }

    // Editable state
    var name by remember(item) { mutableStateOf(item.name) }
    var username by remember(item) { mutableStateOf(item.username) }
    var password by remember(item) { mutableStateOf(item.password) }
    var uri by remember(item) { mutableStateOf(item.uri) }
    var notes by remember(item) { mutableStateOf(item.notes) }
    var totpSecret by remember(item) { mutableStateOf(item.totpSecret) }
    var cardNumber by remember(item) { mutableStateOf(item.cardNumber) }
    var cardExpiry by remember(item) { mutableStateOf(item.cardExpiry) }
    var cardCvv by remember(item) { mutableStateOf(item.cardCvv) }
    var identityName by remember(item) { mutableStateOf(item.identityName) }
    var identityEmail by remember(item) { mutableStateOf(item.identityEmail) }
    var identityPhone by remember(item) { mutableStateOf(item.identityPhone) }
    var identityAddress by remember(item) { mutableStateOf(item.identityAddress) }

    if (showDeleteDialog) {
        AlertDialog(
            onDismissRequest = { showDeleteDialog = false },
            title = { Text("Delete Item") },
            text = { Text("Are you sure you want to delete \"${item.name}\"? This cannot be undone.") },
            confirmButton = {
                TextButton(
                    onClick = {
                        showDeleteDialog = false
                        onDelete(item.id)
                    }
                ) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showDeleteDialog = false }) {
                    Text("Cancel")
                }
            },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (isEditing) "Edit Item" else "Item Details") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Text("←")
                    }
                },
                actions = {
                    if (!isEditing) {
                        IconButton(onClick = { isEditing = true }) {
                            Text("✏️")
                        }
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
                .padding(horizontal = 16.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            Spacer(modifier = Modifier.height(8.dp))

            // Item kind badge
            Text(
                text = item.kind.displayName(),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.primary,
            )

            Spacer(modifier = Modifier.height(16.dp))

            // Name field (always shown)
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text("Name") },
                modifier = Modifier.fillMaxWidth(),
                enabled = isEditing,
                singleLine = true,
            )

            Spacer(modifier = Modifier.height(12.dp))

            // Kind-specific fields
            when (item.kind) {
                ItemKind.LOGIN -> {
                    OutlinedTextField(
                        value = username,
                        onValueChange = { username = it },
                        label = { Text("Username") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    PasswordField(
                        value = password,
                        onValueChange = { password = it },
                        label = "Password",
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = uri,
                        onValueChange = { uri = it },
                        label = { Text("URI") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = totpSecret,
                        onValueChange = { totpSecret = it },
                        label = { Text("TOTP Secret") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                }

                ItemKind.SECURE_NOTE -> {
                    // Notes only — shown below
                }

                ItemKind.CARD -> {
                    OutlinedTextField(
                        value = cardNumber,
                        onValueChange = { cardNumber = it },
                        label = { Text("Card Number") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    Row(modifier = Modifier.fillMaxWidth()) {
                        OutlinedTextField(
                            value = cardExpiry,
                            onValueChange = { cardExpiry = it },
                            label = { Text("Expiry") },
                            modifier = Modifier.weight(1f),
                            enabled = isEditing,
                            singleLine = true,
                        )
                        Spacer(modifier = Modifier.width(12.dp))
                        PasswordField(
                            value = cardCvv,
                            onValueChange = { cardCvv = it },
                            label = "CVV",
                            modifier = Modifier.weight(1f),
                            enabled = isEditing,
                        )
                    }
                }

                ItemKind.IDENTITY -> {
                    OutlinedTextField(
                        value = identityName,
                        onValueChange = { identityName = it },
                        label = { Text("Full Name") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = identityEmail,
                        onValueChange = { identityEmail = it },
                        label = { Text("Email") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = identityPhone,
                        onValueChange = { identityPhone = it },
                        label = { Text("Phone") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        singleLine = true,
                    )
                    Spacer(modifier = Modifier.height(12.dp))
                    OutlinedTextField(
                        value = identityAddress,
                        onValueChange = { identityAddress = it },
                        label = { Text("Address") },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = isEditing,
                        minLines = 2,
                    )
                }
            }

            // Notes (all kinds)
            Spacer(modifier = Modifier.height(12.dp))
            OutlinedTextField(
                value = notes,
                onValueChange = { notes = it },
                label = { Text("Notes") },
                modifier = Modifier.fillMaxWidth(),
                enabled = isEditing,
                minLines = 3,
                maxLines = 8,
            )

            Spacer(modifier = Modifier.height(24.dp))

            // Action buttons
            if (isEditing) {
                Button(
                    onClick = {
                        onSave(
                            item.copy(
                                name = name,
                                username = username,
                                password = password,
                                uri = uri,
                                notes = notes,
                                totpSecret = totpSecret,
                                cardNumber = cardNumber,
                                cardExpiry = cardExpiry,
                                cardCvv = cardCvv,
                                identityName = identityName,
                                identityEmail = identityEmail,
                                identityPhone = identityPhone,
                                identityAddress = identityAddress,
                            )
                        )
                        isEditing = false
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Save")
                }
                Spacer(modifier = Modifier.height(8.dp))
                OutlinedButton(
                    onClick = {
                        // Reset fields
                        name = item.name
                        username = item.username
                        password = item.password
                        uri = item.uri
                        notes = item.notes
                        totpSecret = item.totpSecret
                        isEditing = false
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Text("Cancel")
                }
            }

            Spacer(modifier = Modifier.height(16.dp))

            // Delete button (always available)
            OutlinedButton(
                onClick = { showDeleteDialog = true },
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.outlinedButtonColors(
                    contentColor = MaterialTheme.colorScheme.error,
                ),
            ) {
                Text("Delete Item")
            }

            Spacer(modifier = Modifier.height(32.dp))
        }
    }
}
