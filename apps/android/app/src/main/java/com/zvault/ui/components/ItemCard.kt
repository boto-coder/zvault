package com.zvault.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.zvault.ItemKind
import com.zvault.VaultItem

/**
 * A card composable showing an item summary in the vault list.
 *
 * Displays the item name, username/subtitle, and a kind icon.
 * Tapping the card navigates to the item detail screen.
 */
@Composable
fun ItemCard(
    item: VaultItem,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Card(
        modifier = modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
        elevation = CardDefaults.cardElevation(defaultElevation = 1.dp),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            // Kind icon
            Text(
                text = item.kind.icon(),
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.size(40.dp),
            )

            Spacer(modifier = Modifier.width(12.dp))

            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = item.name.ifBlank { "(Untitled)" },
                    style = MaterialTheme.typography.bodyLarge,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val subtitle = item.subtitle()
                if (subtitle.isNotBlank()) {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

/**
 * Icon representation for each item kind.
 */
private fun ItemKind.icon(): String = when (this) {
    ItemKind.LOGIN -> "🔑"
    ItemKind.SECURE_NOTE -> "📝"
    ItemKind.CARD -> "💳"
    ItemKind.IDENTITY -> "👤"
}

/**
 * Subtitle text for each item kind.
 */
private fun VaultItem.subtitle(): String = when (kind) {
    ItemKind.LOGIN -> username
    ItemKind.SECURE_NOTE -> notes.take(50)
    ItemKind.CARD -> if (cardNumber.length >= 4) "•••• ${cardNumber.takeLast(4)}" else ""
    ItemKind.IDENTITY -> identityEmail.ifBlank { identityName }
}
