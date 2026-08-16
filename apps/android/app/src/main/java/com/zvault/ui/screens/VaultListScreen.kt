package com.zvault.ui.screens

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.zvault.VaultItem
import com.zvault.ui.components.ItemCard

/**
 * Main vault list screen showing all items with search and filtering.
 *
 * Features:
 * - Search bar to filter items by name, username, or URI
 * - LazyColumn for performant scrolling of large vaults
 * - FAB to add new items
 * - Top app bar with sync, lock, settings, and devices navigation
 * - Pull-to-refresh triggers force sync
 * - Snackbar shows sync result
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VaultListScreen(
    items: List<VaultItem>,
    isSyncing: Boolean,
    syncResultMessage: String?,
    onSyncResultShown: () -> Unit,
    onItemClick: (VaultItem) -> Unit,
    onAddClick: () -> Unit,
    onLockClick: () -> Unit,
    onSettingsClick: () -> Unit,
    onDevicesClick: () -> Unit,
    onSyncClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var searchQuery by remember { mutableStateOf("") }
    val snackbarHostState = remember { SnackbarHostState() }

    val filteredItems = remember(items, searchQuery) {
        if (searchQuery.isBlank()) items
        else items.filter { item ->
            item.name.contains(searchQuery, ignoreCase = true) ||
                item.username.contains(searchQuery, ignoreCase = true) ||
                item.uri.contains(searchQuery, ignoreCase = true)
        }
    }

    // Show snackbar when sync result arrives
    LaunchedEffect(syncResultMessage) {
        if (syncResultMessage != null) {
            snackbarHostState.showSnackbar(syncResultMessage)
            onSyncResultShown()
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("ZVault") },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                ),
                actions = {
                    IconButton(onClick = onSyncClick, enabled = !isSyncing) {
                        if (isSyncing) {
                            CircularProgressIndicator(
                                modifier = Modifier.padding(4.dp),
                                strokeWidth = 2.dp,
                            )
                        } else {
                            Text("🔄")
                        }
                    }
                    IconButton(onClick = onDevicesClick) {
                        Text("📱")
                    }
                    IconButton(onClick = onSettingsClick) {
                        Text("⚙️")
                    }
                    IconButton(onClick = onLockClick) {
                        Text("🔒")
                    }
                },
            )
        },
        floatingActionButton = {
            FloatingActionButton(
                onClick = onAddClick,
                containerColor = MaterialTheme.colorScheme.primary,
            ) {
                Text("＋", style = MaterialTheme.typography.headlineSmall)
            }
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        modifier = modifier,
    ) { padding ->
        PullToRefreshBox(
            isRefreshing = isSyncing,
            onRefresh = onSyncClick,
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            Column(
                modifier = Modifier.fillMaxSize(),
            ) {
                // Search bar
                TextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 8.dp),
                    placeholder = { Text("Search vault…") },
                    singleLine = true,
                    colors = TextFieldDefaults.colors(
                        focusedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                        unfocusedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
                )

                if (filteredItems.isEmpty()) {
                    // Empty state
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .padding(32.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Text(
                                text = if (searchQuery.isBlank()) "🔐" else "🔍",
                                style = MaterialTheme.typography.displayMedium,
                            )
                            Spacer(modifier = Modifier.height(16.dp))
                            Text(
                                text = if (searchQuery.isBlank()) {
                                    "Your vault is empty.\nTap + to add your first item."
                                } else {
                                    "No items match \"$searchQuery\""
                                },
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                } else {
                    LazyColumn(
                        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                    ) {
                        items(
                            items = filteredItems,
                            key = { it.id },
                        ) { item ->
                            ItemCard(
                                item = item,
                                onClick = { onItemClick(item) },
                            )
                            Spacer(modifier = Modifier.height(8.dp))
                        }
                    }
                }
            }
        }
    }
}
