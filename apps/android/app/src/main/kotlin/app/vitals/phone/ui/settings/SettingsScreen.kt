package app.vitals.phone.ui.settings

import android.Manifest
import android.app.LocaleManager
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.LocaleList
import android.provider.Settings as AndroidSettings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import app.vitals.phone.BuildConfig
import app.vitals.phone.R
import app.vitals.phone.data.GlassMode
import app.vitals.phone.graph
import app.vitals.phone.service.WatchService
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.components.Tab
import kotlinx.coroutines.launch

@Composable
fun SettingsScreen() {
    val context = LocalContext.current
    val graph = context.graph
    val scope = rememberCoroutineScope()
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()
    val watched by graph.settings.watched.collectAsStateWithLifecycle(emptySet())
    val selected by graph.settings.selected.collectAsStateWithLifecycle(null)
    val glass by graph.settings.glass.collectAsStateWithLifecycle(GlassMode.Auto)
    var renaming by remember { mutableStateOf<Pairing?>(null) }
    var removing by remember { mutableStateOf<Pairing?>(null) }
    var notificationsAllowed by remember { mutableStateOf(notificationsGranted(context)) }
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        notificationsAllowed = it
    }

    GlassScaffold(title = stringResource(R.string.settings_title), tab = Tab.Settings) { padding ->
        LazyColumn(
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                top = padding.calculateTopPadding() + 8.dp,
                bottom = padding.calculateBottomPadding() + 24.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.fillMaxSize(),
        ) {
            if (!notificationsAllowed && watched.isNotEmpty()) {
                item(key = "notif") {
                    SectionCard(stringResource(R.string.channel_alerts)) {
                        Text(stringResource(R.string.settings_notifications_off))
                        TextButton(onClick = {
                            if (Build.VERSION.SDK_INT >= 33) permission.launch(Manifest.permission.POST_NOTIFICATIONS)
                        }) { Text(stringResource(R.string.settings_notifications_allow)) }
                    }
                }
            }
            item(key = "phone") { PhoneCard() }
            item(key = "pcs-title") {
                Text(stringResource(R.string.settings_pcs), style = MaterialTheme.typography.titleMedium)
            }
            items(pairings, key = { it.id }) { p ->
                val isWidgetPc = selected == p.id || (selected == null && pairings.firstOrNull()?.id == p.id)
                SectionCard(p.label, modifier = Modifier.animateItem()) {
                    Text(
                        p.baseUrl + " · " + stringResource(
                            if (p.scope == Scope.Read) R.string.settings_read_only else R.string.settings_can_control,
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    ToggleRow(
                        title = stringResource(R.string.settings_watch),
                        body = stringResource(R.string.settings_watch_body),
                        checked = p.id in watched,
                        onChange = { on ->
                            scope.launch {
                                graph.settings.setWatched(p.id, on)
                                if (on) {
                                    if (Build.VERSION.SDK_INT >= 33 && !notificationsAllowed) {
                                        permission.launch(Manifest.permission.POST_NOTIFICATIONS)
                                    }
                                    WatchService.start(context)
                                }
                            }
                        },
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(selected = isWidgetPc, onClick = { scope.launch { graph.settings.setSelected(p.id) } })
                        Text(stringResource(R.string.settings_widget_pc))
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { renaming = p }) { Text(stringResource(R.string.settings_rename)) }
                        TextButton(onClick = { removing = p }) {
                            Text(stringResource(R.string.settings_remove), color = MaterialTheme.colorScheme.error)
                        }
                    }
                }
            }
            item(key = "language") { LanguageCard() }
            item(key = "glass") {
                SectionCard(stringResource(R.string.settings_glass)) {
                    Text(
                        stringResource(R.string.settings_glass_body),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        GlassMode.entries.forEach { m ->
                            FilterChip(
                                selected = glass == m,
                                onClick = { scope.launch { graph.settings.setGlass(m) } },
                                label = {
                                    Text(
                                        stringResource(
                                            when (m) {
                                                GlassMode.Auto -> R.string.settings_glass_auto
                                                GlassMode.On -> R.string.settings_glass_on
                                                GlassMode.Off -> R.string.settings_glass_off
                                            },
                                        ),
                                    )
                                },
                            )
                        }
                    }
                }
            }
            item(key = "about") {
                SectionCard(stringResource(R.string.settings_about)) {
                    Text(stringResource(R.string.settings_version, BuildConfig.VERSION_NAME))
                    Text(
                        stringResource(R.string.settings_about_body),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        TextButton(onClick = { openUrl(context, PRIVACY_URL) }) {
                            Text(stringResource(R.string.settings_privacy))
                        }
                        TextButton(onClick = { openUrl(context, SOURCE_URL) }) {
                            Text(stringResource(R.string.settings_source))
                        }
                    }
                }
            }
        }
    }

    renaming?.let { p ->
        var name by remember(p.id) { mutableStateOf(p.label) }
        AlertDialog(
            onDismissRequest = { renaming = null },
            title = { Text(stringResource(R.string.settings_rename)) },
            text = {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(stringResource(R.string.field_name)) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        scope.launch { graph.rename(p.id, name.trim()) }
                        renaming = null
                    },
                    enabled = name.isNotBlank(),
                ) { Text(stringResource(R.string.settings_rename)) }
            },
            dismissButton = { TextButton(onClick = { renaming = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }

    removing?.let { p ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text(stringResource(R.string.settings_remove)) },
            text = { Text(stringResource(R.string.settings_remove_confirm, p.label)) },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch {
                        graph.forget(p.id)
                        graph.widgetCache.retain(graph.pairings.pairings.value.map { it.id }.toSet())
                    }
                    removing = null
                }) { Text(stringResource(R.string.settings_remove), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { removing = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun ToggleRow(title: String, body: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(body, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun PhoneCard() {
    val context = LocalContext.current
    val graph = context.graph
    val scope = rememberCoroutineScope()
    val history by graph.settings.deviceHistory.collectAsStateWithLifecycle(true)
    val access by graph.device.access.collectAsStateWithLifecycle()
    androidx.lifecycle.compose.LifecycleResumeEffect(Unit) {
        graph.device.refreshAccess()
        onPauseOrDispose { }
    }
    SectionCard(stringResource(R.string.settings_phone)) {
        ToggleRow(
            title = stringResource(R.string.settings_device_history),
            body = stringResource(R.string.settings_device_history_body),
            checked = history,
            onChange = { on -> scope.launch { graph.settings.setDeviceHistory(on) } },
        )
        AccessRow(stringResource(R.string.settings_access_usage), access.usageStats) {
            app.vitals.device.SpecialAccess.usage(context)
        }
        AccessRow(stringResource(R.string.settings_access_files), access.allFiles) {
            app.vitals.device.SpecialAccess.allFiles(context)
        }
    }
}

/** A special access is revoked in Settings too, so the row opens it whichever way it stands. */
@Composable
private fun AccessRow(title: String, granted: Boolean, open: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(
                stringResource(if (granted) R.string.settings_access_on else R.string.settings_access_off),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TextButton(onClick = open) { Text(stringResource(R.string.device_access_grant)) }
    }
}

/**
 * Per-app language. On Android 13+ the platform LocaleManager owns it (and
 * the system settings page shows the same choice, fed by locale_config.xml);
 * older versions have no per-app locale, so they open the system language
 * settings instead of pretending.
 */
@Composable
private fun LanguageCard() {
    val context = LocalContext.current
    val manager = remember { if (Build.VERSION.SDK_INT >= 33) context.getSystemService(LocaleManager::class.java) else null }
    var current by remember {
        mutableStateOf(if (Build.VERSION.SDK_INT >= 33) manager?.applicationLocales?.toLanguageTags().orEmpty() else "")
    }
    SectionCard(stringResource(R.string.settings_language)) {
        if (manager == null || Build.VERSION.SDK_INT < 33) {
            TextButton(onClick = {
                context.startActivity(Intent(AndroidSettings.ACTION_LOCALE_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            }) { Text(stringResource(R.string.settings_language_system)) }
            return@SectionCard
        }
        val options = listOf(
            "" to R.string.settings_language_system,
            "en" to R.string.settings_language_en,
            "ro" to R.string.settings_language_ro,
        )
        options.forEach { (tag, label) ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                RadioButton(
                    selected = current.substringBefore('-') == tag,
                    onClick = {
                        current = tag
                        manager.applicationLocales =
                            if (tag.isEmpty()) LocaleList.getEmptyLocaleList() else LocaleList.forLanguageTags(tag)
                    },
                )
                Text(stringResource(label))
            }
        }
    }
}

private fun notificationsGranted(context: android.content.Context): Boolean =
    Build.VERSION.SDK_INT < 33 ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

// Play requires the policy to be reachable from inside the app, not only
// from the listing; the site page is the one kept in step with PRIVACY.md.
private const val PRIVACY_URL = "https://vitals.dragoscatalin.ro/privacy/"
private const val SOURCE_URL = "https://github.com/dragoscv/vitals"

private fun openUrl(context: android.content.Context, url: String) {
    // No browser is a real case on a stripped-down device; the link simply does nothing.
    runCatching {
        context.startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}
