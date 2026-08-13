package com.zvault

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import com.zvault.navigation.ZVaultNavGraph
import com.zvault.ui.theme.ZVaultTheme

/**
 * Main entry point for the ZVault Android application.
 *
 * Uses Jetpack Compose for the UI layer with Material3 theming. The core vault
 * operations are delegated to zvault-core via UniFFI-generated Kotlin bindings.
 *
 * Architecture:
 * - Activity → ZVaultTheme → NavGraph → Screens
 * - VaultViewModel manages all state via StateFlow
 * - VaultRepository bridges to UniFFI native methods
 * - Navigation is handled by Compose Navigation (NavHost)
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            ZVaultTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                ) {
                    ZVaultNavGraph()
                }
            }
        }
    }
}
