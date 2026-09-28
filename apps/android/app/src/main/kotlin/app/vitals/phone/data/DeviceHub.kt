package app.vitals.phone.data

import androidx.compose.runtime.Immutable
import app.vitals.device.DeviceMonitor
import app.vitals.device.model.DeviceSnapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.runningFold
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.launch

/** This phone's latest reading plus a minute of each headline metric, for the sparklines. */
@Immutable
data class DeviceLive(
    val snapshot: DeviceSnapshot? = null,
    val cpu: Series = Series.Empty,
    val gpu: Series = Series.Empty,
    val memory: Series = Series.Empty,
    val temperature: Series = Series.Empty,
    val battery: Series = Series.Empty,
)

/**
 * One live stream of this phone for every screen that shows it. Runs only
 * while collected (the screen uses a lifecycle-aware collector, so it stops
 * in the background); while it runs it also appends one history point a
 * minute, so the chart has detail for the time the user was looking.
 */
class DeviceHub(private val monitor: DeviceMonitor, private val settings: SettingsStore, scope: CoroutineScope) {
    private var lastRecorded = 0L

    val live: Flow<DeviceLive> = monitor.snapshots(1_000)
        .onEach { s -> maybeRecord(s, scope) }
        .runningFold(DeviceLive()) { acc, s ->
            DeviceLive(
                snapshot = s,
                cpu = acc.cpu.plus(s.cpu.load),
                gpu = acc.gpu.plus(s.gpu?.load),
                memory = acc.memory.plus(s.memory.usedBytes * 100f / s.memory.totalBytes),
                temperature = acc.temperature.plus(s.cpu.temperature),
                battery = acc.battery.plus(s.battery?.percent),
            )
        }
        .shareIn(scope, SharingStarted.WhileSubscribed(2_000), replay = 1)

    private fun maybeRecord(s: DeviceSnapshot, scope: CoroutineScope) {
        if (s.timestampMs - lastRecorded < 60_000 || s.cpu.load == null) return
        lastRecorded = s.timestampMs
        scope.launch {
            if (settings.deviceHistory.first()) monitor.record(s)
        }
    }
}
