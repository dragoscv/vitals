package app.vitals.phone.ui.detail

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.repeatOnLifecycle
import app.vitals.core.model.SensorLine
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import kotlinx.coroutines.delay

private enum class SensorGroup(@param:StringRes val title: Int, val accent: Color) {
    Temperature(R.string.sensors_group_temperature, Palette.Thermal),
    Fan(R.string.sensors_group_fan, Palette.Power),
    Power(R.string.sensors_group_power, Palette.Gpu),
    Voltage(R.string.sensors_group_voltage, Palette.Disk),
    Percent(R.string.sensors_group_percent, Palette.Memory),
    Other(R.string.sensors_group_other, Palette.Network),
}

private fun groupOf(unit: String) = when (unit) {
    "temperature" -> SensorGroup.Temperature
    "fanSpeed" -> SensorGroup.Fan
    "power" -> SensorGroup.Power
    "voltage" -> SensorGroup.Voltage
    "percent", "charge" -> SensorGroup.Percent
    else -> SensorGroup.Other
}

private sealed interface SensorsState {
    data object Loading : SensorsState
    data object Failed : SensorsState
    data class Loaded(val groups: List<Pair<SensorGroup, List<SensorLine>>>) : SensorsState
}

/**
 * Polled every five seconds only while this tab is on screen and the app is
 * in front: the desktop caches `/sensors` for five seconds because reading
 * them means WMI, so polling faster would buy nothing.
 */
@Composable
fun SensorsTab(pairing: Pairing, padding: PaddingValues) {
    val graph = LocalContext.current.graph
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var state by remember { mutableStateOf<SensorsState>(SensorsState.Loading) }

    androidx.compose.runtime.LaunchedEffect(pairing.id, lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            val client = graph.client(pairing)
            while (true) {
                state = when (val r = client.sensors()) {
                    is ApiResult.Ok -> SensorsState.Loaded(
                        r.value.groupBy { groupOf(it.unit) }.toSortedMap().map { it.key to it.value },
                    )
                    is ApiResult.Err -> if (state is SensorsState.Loaded) state else SensorsState.Failed
                }
                delay(5_000)
            }
        }
    }

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        when (val s = state) {
            SensorsState.Loading -> item { Text(stringResource(R.string.state_connecting)) }
            SensorsState.Failed -> item { Text(stringResource(R.string.sensors_unavailable)) }
            is SensorsState.Loaded -> {
                if (s.groups.isEmpty()) item { Text(stringResource(R.string.sensors_empty)) }
                items(s.groups, key = { it.first.name }) { (group, lines) ->
                    SectionCard(stringResource(group.title), accent = group.accent, modifier = Modifier.animateItem()) {
                        lines.forEach { line ->
                            val level = if (group == SensorGroup.Temperature) {
                                Thresholds.temperature(line.key, line.value)
                            } else {
                                Level.Unknown
                            }
                            InfoRow(
                                line.label,
                                Format.sensor(line.unit, line.value),
                                valueColour = if (level == Level.Unknown) Color.Unspecified else Palette.of(level),
                            )
                        }
                    }
                }
            }
        }
    }
}
