package com.zvault

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview

/**
 * Main entry point for the ZVault Android application.
 *
 * Uses Jetpack Compose for the UI layer. The core vault operations are
 * delegated to zvault-core via UniFFI-generated Kotlin bindings.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    ZVaultApp()
                }
            }
        }
    }
}

@Composable
fun ZVaultApp() {
    // TODO(M10): Implement navigation and vault UI screens
    Text(text = "ZVault")
}

@Preview(showBackground = true)
@Composable
fun ZVaultAppPreview() {
    MaterialTheme {
        ZVaultApp()
    }
}
