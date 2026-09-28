package app.vitals.phone.ui.device

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import app.vitals.device.model.DeviceInfo
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.GlassScaffold
import app.vitals.phone.ui.components.Tab as BottomTab

internal enum class DeviceTab(val label: Int) {
    Now(R.string.tab_now),
    Battery(R.string.metric_battery),
    Storage(R.string.device_tab_storage),
    Apps(R.string.device_tab_apps),
    Sensors(R.string.tab_sensors),
    History(R.string.tab_history),
    About(R.string.device_tab_about),
}

/**
 * This phone. The live readings are collected only by the tabs that show
 * them, through a lifecycle-aware collector, so the sampler stops within two
 * seconds of the screen leaving the foreground.
 */
@Composable
fun DeviceScreen() {
    val context = LocalContext.current
    val graph = context.graph
    var tab by rememberSaveable { mutableStateOf(DeviceTab.Now) }
    val info by produceState<DeviceInfo?>(null) { value = graph.device.info() }

    // Special access is granted in Settings, outside the app: re-read it
    // every time the user comes back.
    LifecycleResumeEffect(Unit) {
        graph.device.refreshAccess()
        onPauseOrDispose { }
    }
    LaunchedEffect(Unit) { graph.device.refreshAccess() }

    GlassScaffold(
        title = info?.let { it.marketingName ?: it.model } ?: stringResource(R.string.nav_phone),
        tab = BottomTab.Phone,
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
            PrimaryScrollableTabRow(
                selectedTabIndex = tab.ordinal,
                containerColor = Color.Transparent,
                edgePadding = 12.dp,
            ) {
                DeviceTab.entries.forEach { t ->
                    Tab(selected = tab == t, onClick = { tab = t }, text = { Text(stringResource(t.label)) })
                }
            }
            val content = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = padding.calculateBottomPadding() + 24.dp)
            AnimatedContent(tab, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "device-tab") { t ->
                when (t) {
                    DeviceTab.Now -> DeviceNowTab(content)
                    DeviceTab.Battery -> DeviceBatteryTab(content)
                    DeviceTab.Storage -> DeviceStorageTab(content)
                    DeviceTab.Apps -> DeviceAppsTab(content)
                    DeviceTab.Sensors -> DeviceSensorsTab(content)
                    DeviceTab.History -> DeviceHistoryTab(content)
                    DeviceTab.About -> DeviceAboutTab(info, content)
                }
            }
        }
    }
}
