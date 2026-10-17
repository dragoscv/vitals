package app.vitals.phone.ui.device

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.device.model.HardwareSensor
import app.vitals.phone.R
import app.vitals.ui.DeviceLive
import app.vitals.phone.graph
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Format
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import java.util.Locale

@Composable
internal fun DeviceSensorsTab(padding: PaddingValues) {
    val graph = LocalContext.current.graph
    val live by graph.deviceLive.live.collectAsStateWithLifecycle(DeviceLive())
    val sensors by produceState<List<HardwareSensor>?>(null) { value = graph.device.hardwareSensors() }
    var open by rememberSaveable { mutableStateOf<String?>(null) }
    val zones = live.snapshot?.thermal?.zones.orEmpty()
    val locale = androidx.compose.ui.platform.LocalConfiguration.current.locales[0]

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        if (zones.isNotEmpty()) {
            // Hottest group first: that is the one a person opening this tab is looking for.
            zones.groupBy { it.group }.entries.sortedByDescending { e -> e.value.maxOf { it.celsius } }.forEach { (group, list) ->
                item(key = "z-$group") {
                    SectionCard(stringResource(zoneGroupName(group)), accent = Palette.Thermal) {
                        list.sortedByDescending { it.celsius }.forEach { z ->
                            InfoRow(z.name, Format.celsius(z.celsius), valueColour = Palette.of(Thresholds.cpuTemp(z.celsius)))
                        }
                    }
                }
            }
        }
        item(key = "hw-title") {
            if (sensors == null) {
                Text(stringResource(R.string.device_loading))
            } else {
                Text(stringResource(R.string.device_hardware_sensors, sensors!!.size), style = MaterialTheme.typography.titleMedium)
            }
        }
        // type+name repeats on real phones (two "BMI260 Accelerometer"s from
        // different HALs, or wake-up twins), and a duplicate LazyColumn key
        // crashes the list. Vendor and the position make it unique.
        itemsIndexed(sensors.orEmpty(), key = { i, s -> "${s.type}|${s.name}|${s.vendor}|$i" }) { i, s ->
            val key = "${s.type}|${s.name}|${s.vendor}|$i"
            SectionCard(
                s.name,
                accent = Palette.Accent,
                modifier = Modifier.animateItem().clickable { open = if (open == key) null else key },
            ) {
                Text(s.vendor + " · " + s.type.substringAfterLast('.'), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                AnimatedVisibility(open == key) { LiveValues(s) }
                if (open != key) InfoRow(stringResource(R.string.device_value), values(s.values, s.unit))
                InfoRow(stringResource(R.string.device_sensor_power), String.format(locale, "%.2f mA", s.powerMa))
            }
        }
    }
}

/** Streams only while expanded: an open accelerometer card is the only thing keeping the sensor awake. */
@Composable
private fun LiveValues(s: HardwareSensor) {
    val graph = LocalContext.current.graph
    val flow = remember(s.type) { graph.device.sensorValues(s.type) }
    val v by flow.collectAsStateWithLifecycle(s.values ?: emptyList())
    InfoRow(stringResource(R.string.device_value), values(v, s.unit))
}

private fun values(v: List<Float>?, unit: String?): String {
    if (v.isNullOrEmpty()) return Format.DASH
    val shown = v.take(3).joinToString(" · ") { String.format(Locale.getDefault(), "%.2f", it) }
    return if (unit != null) "$shown $unit" else shown
}
