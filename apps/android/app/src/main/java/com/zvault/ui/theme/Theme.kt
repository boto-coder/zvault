package com.zvault.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

// ZVault brand colours — deep blue-grey with teal accents
private val ZVaultPrimary = Color(0xFF00BFA5)
private val ZVaultOnPrimary = Color(0xFFFFFFFF)
private val ZVaultPrimaryContainer = Color(0xFF004D40)
private val ZVaultOnPrimaryContainer = Color(0xFFA7FFEB)
private val ZVaultSecondary = Color(0xFF80CBC4)
private val ZVaultOnSecondary = Color(0xFF003731)
private val ZVaultSecondaryContainer = Color(0xFF004D40)
private val ZVaultOnSecondaryContainer = Color(0xFFB2DFDB)
private val ZVaultTertiary = Color(0xFF64B5F6)
private val ZVaultError = Color(0xFFCF6679)
private val ZVaultOnError = Color(0xFF000000)

private val DarkColorScheme = darkColorScheme(
    primary = ZVaultPrimary,
    onPrimary = ZVaultOnPrimary,
    primaryContainer = ZVaultPrimaryContainer,
    onPrimaryContainer = ZVaultOnPrimaryContainer,
    secondary = ZVaultSecondary,
    onSecondary = ZVaultOnSecondary,
    secondaryContainer = ZVaultSecondaryContainer,
    onSecondaryContainer = ZVaultOnSecondaryContainer,
    tertiary = ZVaultTertiary,
    error = ZVaultError,
    onError = ZVaultOnError,
    background = Color(0xFF121212),
    onBackground = Color(0xFFE0E0E0),
    surface = Color(0xFF1E1E1E),
    onSurface = Color(0xFFE0E0E0),
    surfaceVariant = Color(0xFF2C2C2C),
    onSurfaceVariant = Color(0xFFBDBDBD),
)

private val LightColorScheme = lightColorScheme(
    primary = Color(0xFF00796B),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFB2DFDB),
    onPrimaryContainer = Color(0xFF00251A),
    secondary = Color(0xFF4DB6AC),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFB2DFDB),
    onSecondaryContainer = Color(0xFF00251A),
    tertiary = Color(0xFF1976D2),
    error = Color(0xFFB00020),
    onError = Color(0xFFFFFFFF),
    background = Color(0xFFFAFAFA),
    onBackground = Color(0xFF212121),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF212121),
    surfaceVariant = Color(0xFFF5F5F5),
    onSurfaceVariant = Color(0xFF616161),
)

/**
 * ZVault Material3 theme with dynamic colour support on Android 12+.
 *
 * Falls back to the custom ZVault colour scheme on older API levels.
 */
@Composable
fun ZVaultTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit
) {
    val colorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context)
            else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColorScheme
        else -> LightColorScheme
    }

    MaterialTheme(
        colorScheme = colorScheme,
        content = content
    )
}
