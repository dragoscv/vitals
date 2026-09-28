package app.vitals.phone.ui.device

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.device.model.AppUsage
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.FillBar
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.relativeTime
import app.vitals.ui.Format
import app.vitals.ui.Palette

private enum class AppSort(val label: Int) {
    ScreenTime(R.string.device_sort_screen_time),
    Data(R.string.device_sort_data),
    Storage(R.string.device_sort_storage),
    Name(R.string.sort_name),
}

private fun AppUsage.traffic(): Long? {
    val parts = listOfNotNull(mobileRxBytes, mobileTxBytes, wifiRxBytes, wifiTxBytes)
    return if (parts.isEmpty()) null else parts.sum()
}

private fun AppUsage.footprint(): Long? {
    val parts = listOfNotNull(appBytes, dataBytes, cacheBytes)
    return if (parts.isEmpty()) null else parts.sum()
}

@Composable
internal fun DeviceAppsTab(padding: PaddingValues) {
    val context = LocalContext.current
    val graph = context.graph
    val access by graph.device.access.collectAsState()
    var apps by remember { mutableStateOf<List<AppUsage>?>(null) }
    var sort by rememberSaveable { mutableStateOf(AppSort.ScreenTime) }

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
    val top = when (sort) {
        AppSort.ScreenTime -> sorted.firstOrNull()?.foregroundMs
        AppSort.Data -> sorted.firstOrNull()?.traffic()
        AppSort.Storage -> sorted.firstOrNull()?.footprint()
        AppSort.Name -> null
    }

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxSize()) {
        if (!access.usageStats) {
            item(key = "usage") { AccessCard(R.string.device_access_apps_body) { SpecialAccess.usage(context) } }
        }
        item(key = "sort") {
            androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AppSort.entries.forEach { s ->
                    FilterChip(selected = sort == s, onClick = { sort = s }, label = { Text(stringResource(s.label)) })
                }
            }
        }
        item(key = "window") {
            Text(stringResource(R.string.device_apps_window), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (apps == null) item(key = "loading") { Text(stringResource(R.string.device_loading)) }
        items(sorted, key = { it.packageName }) { a ->
            val value = when (sort) {
                AppSort.ScreenTime, AppSort.Name -> a.foregroundMs
                AppSort.Data -> a.traffic()
                AppSort.Storage -> a.footprint()
            }
            SectionCard(
                a.label,
                accent = Palette.Accent,
                modifier = Modifier.animateItem().fillMaxWidth().clickable { SpecialAccess.appInfo(context, a.packageName) },
            ) {
                InfoRow(stringResource(R.string.device_screen_time), a.foregroundMs?.let { Format.duration(it / 1000) } ?: Format.DASH)
                if (top != null && top > 0 && value != null) FillBar(value.toFloat() / top, Palette.Accent, height = 4.dp)
                InfoRow(stringResource(R.string.device_launches), a.launches?.toString() ?: Format.DASH)
                InfoRow(stringResource(R.string.device_data_used), Format.bytes(a.traffic()))
                InfoRow(stringResource(R.string.device_storage_used), Format.bytes(a.footprint()))
                a.lastUsedMs?.let { InfoRow(stringResource(R.string.device_last_used), relativeTime(it)) }
                InfoRow(stringResource(R.string.device_version), a.versionName ?: Format.DASH)
            }
        }
    }
}
