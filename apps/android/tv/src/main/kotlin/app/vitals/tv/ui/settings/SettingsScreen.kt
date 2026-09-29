package app.vitals.tv.ui.settings

import android.app.LocaleManager
import android.content.Intent
import android.os.Build
import android.os.LocaleList
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.ListItem
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Switch
import androidx.tv.material3.Text
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import app.vitals.device.SpecialAccess
import app.vitals.tv.R
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Choice
import app.vitals.tv.ui.ChoiceRow
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.SafeHorizontal
import app.vitals.tv.ui.SafeVertical
import app.vitals.tv.ui.ScreenTitle
import app.vitals.tv.ui.readable
import app.vitals.ui.Palette
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun SettingsScreen() {
    val context = LocalContext.current
    val graph = context.tvGraph
    val scope = rememberCoroutineScope()
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()
    val history by graph.deviceHistory.collectAsStateWithLifecycle(true)
    val access by graph.device.access.collectAsStateWithLifecycle()
    var armed by remember { mutableStateOf<String?>(null) }
    LifecycleResumeEffect(Unit) {
        graph.device.refreshAccess()
        onPauseOrDispose { }
    }
    // Removing takes two presses within four seconds, the TV's confirmation dialog.
    LaunchedEffect(armed) {
        if (armed != null) {
            delay(4_000)
            armed = null
        }
    }

    Row(Modifier.fillMaxSize().padding(horizontal = SafeHorizontal, vertical = SafeVertical), horizontalArrangement = Arrangement.spacedBy(24.dp)) {
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            item { ScreenTitle(stringResource(R.string.settings_title)) }
            item { Text(stringResource(R.string.settings_pcs), style = MaterialTheme.typography.titleLarge) }
            items(pairings, key = { it.id }) { p -> PairingRow(p, armed == p.id) { if (armed == p.id) scope.launch { graph.forget(p.id); armed = null } else armed = p.id } }
            item {
                Panel(stringResource(R.string.settings_tv)) {
                    ListItem(
                        selected = false,
                        onClick = { scope.launch { graph.setDeviceHistory(!history) } },
                        headlineContent = { Text(stringResource(R.string.settings_device_history)) },
                        supportingContent = { Text(stringResource(R.string.settings_device_history_body)) },
                        trailingContent = { Switch(checked = history, onCheckedChange = null) },
                    )
                    AccessRow(stringResource(R.string.settings_access_usage), access.usageStats) { SpecialAccess.usage(context) }
                    AccessRow(stringResource(R.string.settings_access_files), access.allFiles) { SpecialAccess.allFiles(context) }
                }
            }
        }
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp), contentPadding = PaddingValues(top = 56.dp, bottom = 24.dp)) {
            item { LanguagePanel() }
            item {
                Panel(stringResource(R.string.settings_about)) {
                    Text(stringResource(R.string.settings_version, versionName(context)), style = MaterialTheme.typography.bodyLarge)
                    Hint(stringResource(R.string.settings_about_body))
                    // A TV has no browser worth opening a policy in; the
                    // address is shown so it can be typed on a phone.
                    Text(stringResource(R.string.settings_privacy), style = MaterialTheme.typography.bodyLarge)
                    Text(stringResource(R.string.settings_source), style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
    }
}

@Composable
private fun PairingRow(p: Pairing, armed: Boolean, onRemove: () -> Unit) {
    ListItem(
        selected = armed,
        onClick = onRemove,
        headlineContent = { Text(p.label) },
        supportingContent = {
            Text(
                p.baseUrl + " · " + stringResource(
                    when (p.scope) {
                        Scope.Read -> R.string.settings_read_only
                        Scope.Control -> R.string.settings_can_control
                        Scope.Unknown -> R.string.settings_unknown_scope
                    },
                ),
            )
        },
        trailingContent = {
            Text(
                stringResource(if (armed) R.string.settings_remove_confirm else R.string.settings_remove),
                color = if (armed) readable(Palette.Danger) else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
    )
}

/** A special access is revoked in Settings too, so the row opens it whichever way it stands. */
@Composable
private fun AccessRow(title: String, granted: Boolean, open: () -> Unit) {
    ListItem(
        selected = false,
        onClick = open,
        headlineContent = { Text(title) },
        supportingContent = { Text(stringResource(if (granted) R.string.settings_access_on else R.string.settings_access_off)) },
        trailingContent = { Text(stringResource(R.string.device_access_grant)) },
    )
}

/**
 * Per-app language through the platform LocaleManager (Android 13+). Google
 * TV below 13 has no per-app locale, so the choice opens the system language
 * settings instead of pretending.
 */
@Composable
private fun LanguagePanel() {
    val context = LocalContext.current
    Panel(stringResource(R.string.settings_language)) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            Choice(stringResource(R.string.settings_language_system), selected = true, onClick = {
                runCatching { context.startActivity(Intent(Settings.ACTION_LOCALE_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            })
            return@Panel
        }
        val manager = remember { context.getSystemService(LocaleManager::class.java) }
        var current by remember { mutableStateOf(manager.applicationLocales.toLanguageTags()) }
        ChoiceRow {
            listOf("" to R.string.settings_language_system, "en" to R.string.settings_language_en, "ro" to R.string.settings_language_ro).forEach { (tag, label) ->
                Choice(stringResource(label), selected = current.substringBefore('-') == tag, onClick = {
                    current = tag
                    manager.applicationLocales = if (tag.isEmpty()) LocaleList.getEmptyLocaleList() else LocaleList.forLanguageTags(tag)
                })
            }
        }
    }
}

private fun versionName(context: android.content.Context): String =
    runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }.getOrNull() ?: "—"
