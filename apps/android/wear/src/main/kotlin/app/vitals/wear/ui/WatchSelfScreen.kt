package app.vitals.wear.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.TitleCard
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
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
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) {
                MetricRing(fraction = s?.battery?.percent?.div(100f), color = Palette.Power, modifier = Modifier.size(40.dp), strokeWidth = 4.dp)
                Text(Format.percentCompact(s?.battery?.percent), style = MaterialTheme.typography.labelSmall)
            }
            Spacer(Modifier.width(10.dp))
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(stringResource(R.string.cpu_value, Format.percent(s?.cpu?.load)), style = MaterialTheme.typography.bodySmall)
                Text(stringResource(R.string.ram_value, Format.percent(s?.memoryPercent())), style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

private fun DeviceSnapshot.memoryPercent(): Float = memory.usedBytes * 100f / memory.totalBytes

@Composable
fun WatchSelfScreen(monitor: DeviceMonitor) {
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    val flow = remember(monitor) { monitor.snapshots(1_000) }
    val s by flow.collectAsStateWithLifecycle(null)
    val storage by produceState<List<StorageVolume>>(emptyList()) { value = monitor.storage() }
    val sensors by produceState<List<HardwareSensor>>(emptyList()) { value = monitor.hardwareSensors() }

    ScreenScaffold(scrollState = listState) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(stringResource(R.string.this_watch))
                }
            }
            val snap = s
            if (snap == null) {
                item { Message(stringResource(R.string.loading), "", Modifier.transformedHeight(this, spec)) }
                return@TransformingLazyColumn
            }
            item {
                Section(stringResource(R.string.section_cpu), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                    ValueRow(stringResource(R.string.usage), Format.percent(snap.cpu.load), valueColor = readable(levelColor(Thresholds.cpu(snap.cpu.load))))
                    Gauge(snap.cpu.load?.div(100f), Palette.Cpu)
                    snap.cpu.clusters.forEach { c ->
                        ValueRow(stringResource(R.string.clock), Format.ghz(c.currentFrequencyHz) + " / " + Format.ghz(c.maxFrequencyHz))
                    }
                    snap.cpu.temperature?.let { ValueRow(stringResource(R.string.temperature), Format.celsius(it)) }
                    Text(
                        stringResource(R.string.cpu_estimate_short),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            item {
                Section(stringResource(R.string.section_memory), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                    val m = snap.memory
                    ValueRow(stringResource(R.string.in_use), Format.percent(snap.memoryPercent()), valueColor = readable(levelColor(Thresholds.memory(snap.memoryPercent()))))
                    Gauge(snap.memoryPercent() / 100f, Palette.Memory)
                    Text(
                        stringResource(R.string.of_total, Format.bytes(m.usedBytes), Format.bytes(m.totalBytes)),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            snap.battery?.let { b ->
                item {
                    Section(stringResource(R.string.battery), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        ValueRow(stringResource(R.string.level), Format.percent(b.percent))
                        Gauge(b.percent / 100f, Palette.Power)
                        ValueRow(stringResource(R.string.temperature), Format.celsius(b.temperatureC))
                        b.currentMa?.let { ValueRow(stringResource(R.string.current), "${it.roundToInt()} mA") }
                        b.voltageV?.let { ValueRow(stringResource(R.string.voltage), Format.volts(it)) }
                        ValueRow(
                            stringResource(R.string.state),
                            stringResource(
                                when (b.status) {
                                    "charging" -> R.string.charging
                                    "full" -> R.string.full
                                    else -> R.string.on_battery
                                },
                            ),
                        )
                    }
                }
            }
            storage.firstOrNull()?.let { v ->
                item {
                    Section(stringResource(R.string.storage), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        val used = v.totalBytes - v.freeBytes
                        ValueRow(stringResource(R.string.in_use), stringResource(R.string.of_total, Format.bytes(used), Format.bytes(v.totalBytes)))
                        Gauge(used.toFloat() / v.totalBytes, Palette.Disk)
                    }
                }
            }
            if (sensors.isNotEmpty()) {
                item {
                    Section(stringResource(R.string.sensors), Modifier.transformedHeight(this, spec), SurfaceTransformation(spec)) {
                        sensors.filter { it.values != null }.take(12).forEach { h ->
                            ValueRow(h.name, sensorValue(h))
                        }
                        Text(
                            stringResource(R.string.sensor_count, sensors.size),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}

private fun sensorValue(h: HardwareSensor): String {
    val v = h.values?.firstOrNull() ?: return Format.DASH
    val shown = String.format(Locale.getDefault(), "%.1f", v)
    return if (h.unit != null) "$shown ${h.unit}" else shown
}
