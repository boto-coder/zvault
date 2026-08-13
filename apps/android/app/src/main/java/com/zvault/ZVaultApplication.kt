package com.zvault

import android.app.Application
import android.util.Log

/**
 * Application class for ZVault.
 *
 * Initialises the UniFFI native library on startup, ensuring zvault-core
 * is available before any UI or background work starts.
 */
class ZVaultApplication : Application() {

    companion object {
        private const val TAG = "ZVaultApplication"

        /** Whether the native library was loaded successfully. */
        var nativeLibraryLoaded: Boolean = false
            private set
    }

    override fun onCreate() {
        super.onCreate()
        loadNativeLibrary()
    }

    private fun loadNativeLibrary() {
        try {
            System.loadLibrary("zvault_core")
            nativeLibraryLoaded = true
            Log.i(TAG, "zvault-core native library loaded successfully")
        } catch (e: UnsatisfiedLinkError) {
            nativeLibraryLoaded = false
            Log.e(TAG, "Failed to load zvault-core native library: ${e.message}")
        }
    }
}
