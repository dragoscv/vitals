package app.vitals.wear.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.TitleCard
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.device.DeviceMonitor
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.HardwareSensor
import app.vitals.device.model.StorageVolume
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.wear.R
import java.util.Locale
import kotlin.math.roundToInt

/**
 * The watch's own card at the top of the list. It samples only while the
 * list is on screen: a watch battery is a tenth of a phone's, and a reading
 * nobody is looking at is pure cost.
 */
@Composable
fun WatchSelfCard(monitor: DeviceMonitor, onClick: () -> Unit, modifier: Modifier, transformation: SurfaceTransformation) {
    val flow = remember(monitor) { monitor.snapshots(2_000) }
    val s by flow.collectAsStateWithLifecycle(null)
    TitleCard(
        onClick = onClick,
        title = { Text(stringResource(R.string.this_watch)) },
        modifier = modifier.fillMaxWidth(),
        transformation = transformation,
    ) {
        RingAndLines(fraction = s?.battery?.percent?.div(100f), ringColor = Palette.Power, ringText = Format.percentCompact(s?.battery?.percent)) {
            Text(
                stringResource(R.string.cpu_value, Format.percent(s?.cpu?.load)),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                stringResource(R.string.ram_value, Format.percent(s?.memoryPercent())),
                style = MaterialTheme.typography.bodySmall,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

private fun DeviceSnapshot.memoryPercent(): Float = memory.usedBytes * 100f / memory.totalBytes

@Composable
fun WatchSelfScreen(monitor: DeviceMonitor) {
    val flow = remember(monitor) { monitor.snapshots(1_000) }
    val s by flow.collectAsStateWithLifecycle(null)
    val storage by produceState<List<StorageVolume>>(emptyList()) { value = monitor.storage() }
    val sensors by produceState<List<HardwareSensor>>(emptyList()) { value = monitor.hardwareSensors() }
    WatchSelfList(s, storage, sensors)
}

/** The watch's own readings, one list item per reading so none rides the curve of a round screen. */
@Composable
internal fun WatchSelfList(snap: DeviceSnapshot?, storage: List<StorageVolume>, sensors: List<HardwareSensor>) {
    VitalsList { spec ->
        item { ScreenHeader(spec, stringResource(R.string.this_watch)) }
        if (snap == null) {
            item { Message(stringResource(R.string.loading), "", Modifier.transformedHeight(this, spec)) }
            return@VitalsList
        }
        item { SectionHeader(spec, stringResource(R.string.section_cpu)) }
        item {
            ValueCard(
                spec, stringResource(R.string.usage), Format.percent(snap.cpu.load),
                valueColor = readable(levelColor(Thresholds.cpu(snap.cpu.load))),
                gauge = snap.cpu.load?.div(100f), gaugeColor = Palette.Cpu,
            )
        }
        snap.cpu.clusters.forEach { c ->
            item { ValueCard(spec, stringResource(R.string.clock), Format.ghz(c.currentFrequencyHz) + " / " + Format.ghz(c.maxFrequencyHz)) }
        }
        snap.cpu.temperature?.let { t -> item { ValueCard(spec, stringResource(R.string.temperature), Format.celsius(t)) } }
        item { Note(spec, stringResource(R.string.cpu_estimate_short)) }

        item { SectionHeader(spec, stringResource(R.string.section_memory)) }
        item {
            ValueCard(
                spec, stringResource(R.string.in_use), Format.percent(snap.memoryPercent()),
                valueColor = readable(levelColor(Thresholds.memory(snap.memoryPercent()))),
                gauge = snap.memoryPercent() / 100f, gaugeColor = Palette.Memory,
            )
        }
        item { Note(spec, stringResource(R.string.of_total, Format.bytes(snap.memory.usedBytes), Format.bytes(snap.memory.totalBytes))) }

        snap.battery?.let { b ->
            item { SectionHeader(spec, stringResource(R.string.battery)) }
            item { ValueCard(spec, stringResource(R.string.level), Format.percent(b.percent), gauge = b.percent / 100f, gaugeColor = Palette.Power) }
            item { ValueCard(spec, stringResource(R.string.temperature), Format.celsius(b.temperatureC)) }
            b.currentMa?.let { ma -> item { ValueCard(spec, stringResource(R.string.current), "${ma.roundToInt()} mA") } }
            b.voltageV?.let { v -> item { ValueCard(spec, stringResource(R.string.voltage), Format.volts(v)) } }
            item {
                val state = when (b.status) {
                    "charging" -> R.string.charging
                    "full" -> R.string.full
                    else -> R.string.on_battery
                }
                ValueCard(spec, stringResource(R.string.state), stringResource(state))
            }
        }

        storage.firstOrNull()?.let { v ->
            val used = v.totalBytes - v.freeBytes
            item { SectionHeader(spec, stringResource(R.string.storage)) }
            item {
                ValueCard(
                    spec, stringResource(R.string.in_use),
                    stringResource(R.string.of_total, Format.bytes(used), Format.bytes(v.totalBytes)),
                    gauge = used.toFloat() / v.totalBytes, gaugeColor = Palette.Disk,
                )
            }
        }

        if (sensors.isNotEmpty()) {
            item { SectionHeader(spec, stringResource(R.string.sensors)) }
            sensors.filter { it.values != null }.take(12).forEach { h ->
                item { ValueCard(spec, h.name, sensorValue(h)) }
            }
            item { Note(spec, stringResource(R.string.sensor_count, sensors.size)) }
        }
    }
}

private fun sensorValue(h: HardwareSensor): String {
    val v = h.values?.firstOrNull() ?: return Format.DASH
    val shown = String.format(Locale.getDefault(), "%.1f", v)
    return if (h.unit != null) "$shown ${h.unit}" else shown
}
