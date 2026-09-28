package app.vitals.phone.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.phone.R
import app.vitals.phone.data.DeviceLive
import app.vitals.phone.graph
import app.vitals.phone.ui.components.FillBar
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.components.Sparkline
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlin.math.roundToInt

@Composable
internal fun DeviceBatteryTab(padding: PaddingValues) {
    val graph = LocalContext.current.graph
    val live by graph.deviceLive.live.collectAsStateWithLifecycle(DeviceLive())
    val b = live.snapshot?.battery
    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        if (b == null) {
            item { SectionCard(stringResource(R.string.metric_battery)) { Text(stringResource(R.string.device_no_battery)) } }
            return@LazyColumn
        }
        item(key = "level") {
            SectionCard(stringResource(R.string.metric_battery), accent = Palette.Power) {
                InfoRow(stringResource(batteryStatus(b.status)), Format.percent(b.percent))
                FillBar(b.percent / 100f, Palette.Power, height = 10.dp)
                Sparkline(live.battery, Palette.Power)
                b.chargeTimeRemainingMs?.let { InfoRow(stringResource(R.string.device_full_in), Format.duration(it / 1000)) }
                InfoRow(stringResource(R.string.device_plugged), stringResource(plugged(b.plugged)))
            }
        }
        item(key = "power") {
            SectionCard(stringResource(R.string.metric_power), accent = Palette.Power) {
                InfoRow(stringResource(R.string.device_current), b.currentMa?.let(::milliamps) ?: Format.DASH)
                InfoRow(stringResource(R.string.device_power_draw), Format.watts(b.powerW))
                InfoRow(stringResource(R.string.device_voltage), Format.volts(b.voltageV))
                Text(
                    stringResource(R.string.device_current_sign),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        item(key = "health") {
            SectionCard(stringResource(R.string.battery_health), accent = Palette.Ok) {
                InfoRow(stringResource(R.string.battery_health), stringResource(batteryHealth(b.health)))
                InfoRow(stringResource(R.string.cpu_temperature), Format.celsius(b.temperatureC))
                InfoRow(stringResource(R.string.device_cycles), b.cycleCount?.toString() ?: Format.DASH)
                InfoRow(stringResource(R.string.device_capacity), b.capacityMah?.let { "${it.roundToInt()} mAh" } ?: Format.DASH)
                InfoRow(stringResource(R.string.device_technology), b.technology ?: Format.DASH)
            }
        }
    }
}

/** A trickle of 5 mA is not "5 mA" rounded from 5.5 and not "0 mA": one decimal below 10. */
private fun milliamps(v: Float): String =
    if (kotlin.math.abs(v) < 10f) String.format(java.util.Locale.getDefault(), "%.1f mA", v) else "${v.roundToInt()} mA"
