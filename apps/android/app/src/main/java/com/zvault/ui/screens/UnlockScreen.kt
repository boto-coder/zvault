package com.zvault.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.zvault.ui.components.PasswordField

/**
 * Unlock screen — first screen the user sees when the vault is locked.
 *
 * Provides:
 * - Password input to unlock an existing vault
 * - Button to create a new vault
 * - Optional biometric unlock trigger
 */
@Composable
fun UnlockScreen(
    isUnlocking: Boolean,
    errorMessage: String?,
    biometricAvailable: Boolean,
    onUnlock: (password: String) -> Unit,
    onCreateVault: (password: String) -> Unit,
    onBiometricUnlock: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var password by remember { mutableStateOf("") }
    var confirmPassword by remember { mutableStateOf("") }
    var isCreateMode by remember { mutableStateOf(false) }
    val snackbarHostState = remember { SnackbarHostState() }

    LaunchedEffect(errorMessage) {
        errorMessage?.let { snackbarHostState.showSnackbar(it) }
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbarHostState) },
        modifier = modifier,
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            // App branding
            Text(
                text = "🔐",
                style = MaterialTheme.typography.displayLarge,
            )
            Spacer(modifier = Modifier.height(8.dp))
            Text(
                text = "ZVault",
                style = MaterialTheme.typography.headlineLarge,
            )
            Text(
                text = "Local-first encrypted password manager",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )

            Spacer(modifier = Modifier.height(48.dp))

            // Password field
            PasswordField(
                value = password,
                onValueChange = { password = it },
                label = if (isCreateMode) "Master Password" else "Password",
                modifier = Modifier.fillMaxWidth(),
                enabled = !isUnlocking,
            )

            // Confirm password in create mode
            if (isCreateMode) {
                Spacer(modifier = Modifier.height(12.dp))
                PasswordField(
                    value = confirmPassword,
                    onValueChange = { confirmPassword = it },
                    label = "Confirm Password",
                    modifier = Modifier.fillMaxWidth(),
                    enabled = !isUnlocking,
                    isError = confirmPassword.isNotEmpty() && password != confirmPassword,
                    supportingText = if (confirmPassword.isNotEmpty() && password != confirmPassword) {
                        { Text("Passwords do not match") }
                    } else null,
                )
            }

            Spacer(modifier = Modifier.height(24.dp))

            if (isUnlocking) {
                CircularProgressIndicator(modifier = Modifier.size(48.dp))
                Spacer(modifier = Modifier.height(8.dp))
                Text(
                    text = "Deriving key…",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } else {
                if (isCreateMode) {
                    // Create mode buttons
                    Button(
                        onClick = {
                            if (password == confirmPassword && password.isNotBlank()) {
                                onCreateVault(password)
                            }
                        },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = password.isNotBlank() && password == confirmPassword,
                    ) {
                        Text("Create Vault")
                    }
                    Spacer(modifier = Modifier.height(8.dp))
                    TextButton(onClick = { isCreateMode = false }) {
                        Text("Already have a vault? Unlock")
                    }
                } else {
                    // Unlock mode buttons
                    Button(
                        onClick = {
                            if (password.isNotBlank()) onUnlock(password)
                        },
                        modifier = Modifier.fillMaxWidth(),
                        enabled = password.isNotBlank(),
                    ) {
                        Text("Unlock")
                    }

                    Spacer(modifier = Modifier.height(8.dp))

                    if (biometricAvailable) {
                        OutlinedButton(
                            onClick = onBiometricUnlock,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text("🔏  Unlock with Biometrics")
                        }
                        Spacer(modifier = Modifier.height(8.dp))
                    }

                    TextButton(onClick = { isCreateMode = true }) {
                        Text("Create new vault")
                    }
                }
            }
        }
    }
}
