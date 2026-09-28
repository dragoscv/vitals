package app.vitals.phone.ui.device

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.device.ScanProgress
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.FolderSize
import app.vitals.device.model.StorageVolume
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.components.FillBar
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.theme.readable
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlinx.coroutines.launch

@Composable
internal fun DeviceStorageTab(padding: PaddingValues) {
    val context = LocalContext.current
    val graph = context.graph
    val resources = LocalResources.current
    val snackbar = LocalSnackbar.current
    val scope = rememberCoroutineScope()
    val access by graph.device.access.collectAsState()
    var volumes by remember { mutableStateOf<List<StorageVolume>?>(null) }
    var scan by remember { mutableStateOf<ScanProgress?>(null) }
    var scanRun by remember { mutableIntStateOf(0) }
    var path by remember { mutableStateOf<List<FolderSize>>(emptyList()) }
    val cleanup = remember { mutableStateListOf<CleanupItem>() }
    var cleanupLoaded by remember { mutableStateOf(false) }
    var confirm by remember { mutableStateOf<CleanupItem?>(null) }

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

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        items(volumes.orEmpty(), key = { "v-" + it.id }) { v -> Volume(v, access.usageStats) }
        if (!access.usageStats) {
            item(key = "usage") { AccessCard(R.string.device_access_usage_body) { SpecialAccess.usage(context) } }
        }
        item(key = "scan") {
            SectionCard(stringResource(R.string.device_scan_title), accent = Palette.Disk, modifier = Modifier.animateContentSize()) {
                when (val s = scan) {
                    null -> {
                        Text(stringResource(R.string.device_scan_body), style = MaterialTheme.typography.bodyMedium)
                        if (access.allFiles) {
                            Button(onClick = { scanRun++ }) { Text(stringResource(R.string.device_scan_start)) }
                        } else {
                            Text(stringResource(R.string.device_access_files_body), style = MaterialTheme.typography.bodySmall)
                            Button(onClick = { SpecialAccess.allFiles(context) }) { Text(stringResource(R.string.device_access_grant)) }
                        }
                    }
                    ScanProgress.NoAccess -> Button(onClick = { SpecialAccess.allFiles(context) }) {
                        Text(stringResource(R.string.device_access_grant))
                    }
                    is ScanProgress.Scanning -> {
                        LinearProgressIndicator(Modifier.fillMaxWidth())
                        Text(
                            stringResource(R.string.device_scan_progress, s.files, Format.bytes(s.bytes)),
                            style = MaterialTheme.typography.bodySmall,
                        )
                        Text(s.path, style = MaterialTheme.typography.labelSmall, maxLines = 1, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    is ScanProgress.Done -> {
                        val here = path.lastOrNull() ?: s.tree
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(here.path.removePrefix(graph.device.storageRoot).ifEmpty { "/" }, modifier = Modifier.weight(1f), maxLines = 1)
                            if (path.size > 1) TextButton(onClick = { path = path.dropLast(1) }) { Text(stringResource(R.string.back)) }
                            TextButton(onClick = { scanRun++ }) { Text(stringResource(R.string.device_scan_again)) }
                        }
                        InfoRow(stringResource(R.string.device_scan_total, here.files), Format.bytes(here.bytes))
                        here.children.take(25).forEach { c ->
                            Column(
                                Modifier.fillMaxWidth()
                                    .clickable(enabled = c.children.isNotEmpty()) { path = path + c }
                                    .padding(vertical = 4.dp),
                            ) {
                                InfoRow(c.name, Format.bytes(c.bytes))
                                FillBar(if (here.bytes > 0) c.bytes.toFloat() / here.bytes else null, Palette.Disk, height = 4.dp)
                            }
                        }
                        Text(
                            stringResource(R.string.device_scan_took, s.elapsedMs / 1000f),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
        item(key = "clean-title") {
            Text(stringResource(R.string.device_cleanup_title), style = MaterialTheme.typography.titleMedium)
        }
        if (cleanupLoaded && cleanup.isEmpty()) {
            item(key = "clean-empty") {
                Text(
                    stringResource(if (access.allFiles || access.usageStats) R.string.device_cleanup_empty else R.string.device_cleanup_needs_access),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        items(cleanup.take(60), key = { "c-" + it.kind + (it.path ?: it.packageName) }) { item ->
            SectionCard(
                stringResource(cleanupKind(item.kind)),
                accent = if (item.rating == "safe") Palette.Ok else Palette.Warn,
                modifier = Modifier.animateItem(),
            ) {
                InfoRow(item.label, Format.bytes(item.bytes))
                Text(
                    stringResource(if (item.rating == "safe") R.string.device_rating_safe else R.string.device_rating_review),
                    style = MaterialTheme.typography.labelMedium,
                    color = readable(if (item.rating == "safe") Palette.Ok else Palette.Warn),
                )
                if (item.path != null) {
                    TextButton(onClick = { confirm = item }) { Text(stringResource(R.string.device_delete)) }
                } else if (item.packageName != null) {
                    TextButton(onClick = { SpecialAccess.appInfo(context, item.packageName!!) }) {
                        Text(stringResource(R.string.device_open_app_info))
                    }
                }
            }
        }
    }

    confirm?.let { item ->
        AlertDialog(
            onDismissRequest = { confirm = null },
            title = { Text(stringResource(R.string.device_delete_confirm_title)) },
            text = { Text(stringResource(R.string.device_delete_confirm_body, item.label, Format.bytes(item.bytes))) },
            confirmButton = {
                TextButton(onClick = {
                    confirm = null
                    scope.launch {
                        val freed = graph.device.delete(item)
                        if (freed != null) cleanup.remove(item)
                        snackbar.showSnackbar(
                            if (freed != null) resources.getString(R.string.device_deleted, Format.bytes(freed))
                            else resources.getString(R.string.device_delete_failed),
                        )
                    }
                }) { Text(stringResource(R.string.device_delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun Volume(v: StorageVolume, usageAccess: Boolean) {
    val used = v.totalBytes - v.freeBytes
    SectionCard(v.label, accent = Palette.Disk) {
        InfoRow(stringResource(R.string.memory_used_of, Format.bytes(used), Format.bytes(v.totalBytes)), Format.percent(used * 100f / v.totalBytes))
        FillBar(used.toFloat() / v.totalBytes, Palette.Disk, height = 10.dp)
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

@Composable
internal fun AccessCard(body: Int, onGrant: () -> Unit) {
    SectionCard(stringResource(R.string.device_access_title), accent = Palette.Accent) {
        Text(stringResource(body), style = MaterialTheme.typography.bodyMedium)
        Button(onClick = onGrant) { Text(stringResource(R.string.device_access_grant)) }
    }
}
