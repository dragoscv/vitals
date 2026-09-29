package app.vitals.tv.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.ListItem
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import app.vitals.device.ScanProgress
import app.vitals.device.SpecialAccess
import app.vitals.device.model.AppUsage
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.FolderSize
import app.vitals.device.model.StorageVolume
import app.vitals.tv.R
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Action
import app.vitals.tv.ui.Bar
import app.vitals.tv.ui.Choice
import app.vitals.tv.ui.ChoiceRow
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.InfoRow
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.cleanupKind
import app.vitals.tv.ui.readable
import app.vitals.tv.ui.relativeTime
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Volumes and categories on the left; the folder scan and what could be freed
 * on the right. Deleting takes two presses of OK within four seconds, the TV's
 * version of the phone's confirmation dialog.
 */
@Composable
internal fun StorageTab() {
    val context = LocalContext.current
    val graph = context.tvGraph
    val resources = LocalResources.current
    val scope = rememberCoroutineScope()
    val access by graph.device.access.collectAsStateWithLifecycle()
    var volumes by remember { mutableStateOf<List<StorageVolume>?>(null) }
    var scan by remember { mutableStateOf<ScanProgress?>(null) }
    var scanRun by remember { mutableIntStateOf(0) }
    var path by remember { mutableStateOf<List<FolderSize>>(emptyList()) }
    val cleanup = remember { mutableStateListOf<CleanupItem>() }
    var cleanupLoaded by remember { mutableStateOf(false) }
    var armed by remember { mutableStateOf<CleanupItem?>(null) }
    var message by remember { mutableStateOf<String?>(null) }

    LifecycleResumeEffect(Unit) {
        graph.device.refreshAccess()
        onPauseOrDispose { }
    }
    LaunchedEffect(access) {
        volumes = graph.device.storage()
        cleanup.clear()
        cleanup.addAll(graph.device.cleanupCandidates())
        cleanupLoaded = true
    }
    LaunchedEffect(scanRun) {
        if (scanRun == 0) return@LaunchedEffect
        graph.device.scanFolders(graph.device.storageRoot).collect { p ->
            scan = p
            if (p is ScanProgress.Done) path = listOf(p.tree)
        }
    }
    LaunchedEffect(armed) {
        if (armed != null) {
            delay(4_000)
            armed = null
        }
    }

    Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            items(volumes.orEmpty(), key = { "v-" + it.id }) { v -> Volume(v, access.usageStats) }
            if (!access.usageStats) {
                item(key = "usage") {
                    Panel(stringResource(R.string.device_access_title), accent = Palette.Accent) {
                        Hint(stringResource(R.string.device_access_usage_body))
                        Action(stringResource(R.string.device_access_grant), onClick = { SpecialAccess.usage(context) })
                    }
                }
            }
            item(key = "scan") {
                Panel(stringResource(R.string.device_scan_title), accent = Palette.Disk) {
                    when (val s = scan) {
                        null -> {
                            Hint(stringResource(R.string.device_scan_body))
                            if (access.allFiles) {
                                Action(stringResource(R.string.device_scan_start), onClick = { scanRun++ })
                            } else {
                                Hint(stringResource(R.string.device_access_files_body))
                                Action(stringResource(R.string.device_access_grant), onClick = { SpecialAccess.allFiles(context) })
                            }
                        }
                        ScanProgress.NoAccess -> Action(stringResource(R.string.device_access_grant), onClick = { SpecialAccess.allFiles(context) })
                        is ScanProgress.Scanning -> {
                            Hint(pluralStringResource(R.plurals.device_scan_files, s.files, s.files) + ", " + stringResource(R.string.device_scan_progress, Format.bytes(s.bytes)))
                            Hint(s.path)
                        }
                        is ScanProgress.Done -> {
                            val here = path.lastOrNull() ?: s.tree
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                Text(here.path.removePrefix(graph.device.storageRoot).ifEmpty { "/" }, modifier = Modifier.weight(1f), maxLines = 1)
                                if (path.size > 1) Action(stringResource(R.string.back), onClick = { path = path.dropLast(1) })
                                Action(stringResource(R.string.device_scan_again), onClick = { scanRun++ })
                            }
                            InfoRow(pluralStringResource(R.plurals.device_scan_files, here.files, here.files), Format.bytes(here.bytes))
                            here.children.take(25).forEach { c ->
                                ListItem(
                                    selected = false,
                                    onClick = { if (c.children.isNotEmpty()) path = path + c },
                                    headlineContent = { Text(c.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                                    supportingContent = { Bar(if (here.bytes > 0) c.bytes.toFloat() / here.bytes else null, Palette.Disk, height = 4.dp) },
                                    trailingContent = { Text(Format.bytes(c.bytes)) },
                                )
                            }
                            Hint(stringResource(R.string.device_scan_took, s.elapsedMs / 1000f))
                        }
                    }
                }
            }
        }
        LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            item { Text(stringResource(R.string.device_cleanup_title), style = MaterialTheme.typography.titleLarge) }
            message?.let { m -> item { Hint(m) } }
            if (cleanupLoaded && cleanup.isEmpty()) {
                item {
                    Hint(stringResource(if (access.allFiles || access.usageStats) R.string.device_cleanup_empty else R.string.device_cleanup_needs_access))
                }
            }
            items(cleanup.take(60), key = { "c-" + it.kind + (it.path ?: it.packageName) }) { item ->
                val safe = item.rating == "safe"
                ListItem(
                    selected = armed == item,
                    onClick = {
                        val pkg = item.packageName
                        when {
                            item.path == null && pkg != null -> SpecialAccess.appInfo(context, pkg)
                            item.path == null -> Unit
                            armed != item -> armed = item
                            else -> {
                                armed = null
                                scope.launch {
                                    val freed = graph.device.delete(item)
                                    if (freed != null) cleanup.remove(item)
                                    message = if (freed != null) resources.getString(R.string.device_deleted, Format.bytes(freed)) else resources.getString(R.string.device_delete_failed)
                                }
                            }
                        }
                    },
                    overlineContent = { Text(stringResource(cleanupKind(item.kind))) },
                    headlineContent = { Text(item.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    supportingContent = {
                        Text(
                            when {
                                armed == item -> stringResource(R.string.device_delete_confirm)
                                item.path == null -> stringResource(R.string.device_open_app_info)
                                else -> stringResource(if (safe) R.string.device_rating_safe else R.string.device_rating_review)
                            },
                            color = if (armed == item) readable(Palette.Danger) else readable(if (safe) Palette.Ok else Palette.Warn),
                        )
                    },
                    trailingContent = { Text(Format.bytes(item.bytes)) },
                )
            }
        }
    }
}

@Composable
private fun Volume(v: StorageVolume, usageAccess: Boolean) {
    val used = v.totalBytes - v.freeBytes
    Panel(v.label, accent = Palette.Disk) {
        val pct = if (v.totalBytes > 0) used * 100f / v.totalBytes else null
        InfoRow(stringResource(R.string.memory_used_of, Format.bytes(used), Format.bytes(v.totalBytes)), Format.percent(pct))
        Bar(pct?.div(100f), Palette.Disk, height = 12.dp)
        InfoRow(stringResource(R.string.memory_available), Format.bytes(v.freeBytes))
        if (usageAccess) {
            InfoRow(stringResource(R.string.device_tab_apps), Format.bytes(v.apps))
            InfoRow(stringResource(R.string.device_images), Format.bytes(v.images))
            InfoRow(stringResource(R.string.device_video), Format.bytes(v.video))
            InfoRow(stringResource(R.string.device_audio), Format.bytes(v.audio))
            InfoRow(stringResource(R.string.device_other_files), Format.bytes(v.otherFiles))
            InfoRow(stringResource(R.string.device_system), Format.bytes(v.system))
        }
    }
}

private enum class AppSort(val label: Int) {
    ScreenTime(R.string.device_sort_screen_time),
    Data(R.string.device_sort_data),
    Storage(R.string.device_sort_storage),
    Name(R.string.sort_name),
}

private fun AppUsage.traffic(): Long? = listOfNotNull(mobileRxBytes, mobileTxBytes, wifiRxBytes, wifiTxBytes).takeIf { it.isNotEmpty() }?.sum()
private fun AppUsage.footprint(): Long? = listOfNotNull(appBytes, dataBytes, cacheBytes).takeIf { it.isNotEmpty() }?.sum()

@Composable
internal fun AppsTab() {
    val context = LocalContext.current
    val graph = context.tvGraph
    val access by graph.device.access.collectAsStateWithLifecycle()
    var apps by remember { mutableStateOf<List<AppUsage>?>(null) }
    var sort by rememberSaveable { mutableStateOf(AppSort.ScreenTime) }
    LifecycleResumeEffect(Unit) {
        graph.device.refreshAccess()
        onPauseOrDispose { }
    }
    LaunchedEffect(access.usageStats) { apps = graph.device.apps() }
    val sorted = remember(apps, sort) {
        apps.orEmpty().sortedWith(
            when (sort) {
                AppSort.ScreenTime -> compareByDescending<AppUsage> { it.foregroundMs ?: -1 }
                AppSort.Data -> compareByDescending { it.traffic() ?: -1 }
                AppSort.Storage -> compareByDescending { it.footprint() ?: -1 }
                AppSort.Name -> compareBy { it.label.lowercase() }
            },
        )
    }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (!access.usageStats) {
            Panel(stringResource(R.string.device_access_title), accent = Palette.Accent) {
                Hint(stringResource(R.string.device_access_usage_body))
                Action(stringResource(R.string.device_access_grant), onClick = { SpecialAccess.usage(context) })
            }
        }
        ChoiceRow { AppSort.entries.forEach { s -> Choice(stringResource(s.label), selected = sort == s, onClick = { sort = s }) } }
        Hint(stringResource(R.string.device_apps_window))
        if (apps == null) Hint(stringResource(R.string.device_loading))
        // Column headings: three unlabelled numbers per row mean nothing from the sofa.
        Row(Modifier.padding(horizontal = 16.dp)) {
            Text("", Modifier.weight(1f))
            listOf(R.string.device_screen_time, R.string.device_data_used, R.string.device_storage_used).forEach { h ->
                Text(stringResource(h), Modifier.width(110.dp), textAlign = TextAlign.End, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
            items(sorted, key = { it.packageName }) { a ->
                ListItem(
                    selected = false,
                    onClick = { SpecialAccess.appInfo(context, a.packageName) },
                    headlineContent = { Text(a.label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    supportingContent = {
                        Text(
                            listOfNotNull(
                                a.versionName,
                                a.lastUsedMs?.let { stringResource(R.string.device_last_used) + " " + relativeTime(it) },
                            ).joinToString(" · "),
                        )
                    },
                    trailingContent = {
                        Row {
                            Text(a.foregroundMs?.let { Format.duration(it / 1000) } ?: Format.DASH, Modifier.width(110.dp), textAlign = TextAlign.End)
                            Text(Format.bytes(a.traffic()), Modifier.width(110.dp), textAlign = TextAlign.End)
                            Text(Format.bytes(a.footprint()), Modifier.width(110.dp), textAlign = TextAlign.End)
                        }
                    },
                )
            }
        }
    }
}
