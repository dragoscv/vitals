package app.vitals.wear.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.Card
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.core.model.SensorLine
import app.vitals.ui.Format
import app.vitals.ui.Level
import app.vitals.ui.Palette
import app.vitals.ui.Thresholds
import app.vitals.wear.R

/** The groups, in the order a person looking for a problem cares about them. */
private enum class SensorGroup(val title: Int, val color: Color) {
    Temperatures(R.string.group_temperatures, Palette.Thermal),
    Fans(R.string.group_fans, Palette.Network),
    Power(R.string.group_power, Palette.Power),
    Voltages(R.string.group_voltages, Palette.Disk),
    Other(R.string.group_other, Palette.Memory),
    ;

    companion object {
        fun of(unit: String): SensorGroup = when (unit) {
            "temperature" -> Temperatures
            "fanSpeed" -> Fans
            "power" -> Power
            "voltage" -> Voltages
            else -> Other
        }
    }
}

/** Every sensor the PC reports, grouped by what it measures. */
@Composable
fun SensorsScreen(sensors: List<SensorLine>, reachable: Boolean) {
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    val groups = remember(sensors) {
        sensors.groupBy { SensorGroup.of(it.unit) }
            .mapValues { (group, lines) ->
                // Hottest first: the reading someone opened this screen to find.
                if (group == SensorGroup.Temperatures) lines.sortedByDescending { it.value } else lines.sortedBy { it.label }
            }
            .toSortedMap()
    }
    ScreenScaffold(scrollState = listState) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(stringResource(R.string.sensors))
                }
            }
            if (sensors.isEmpty()) {
                item {
                    Message(
                        title = stringResource(R.string.no_sensors_title),
                        body = stringResource(if (reachable) R.string.no_sensors_body else R.string.unreachable_title),
                        modifier = Modifier.transformedHeight(this, spec),
                    )
                }
            }
            groups.forEach { (group, lines) ->
                item(key = "header-${group.name}") {
                    ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                        Text(
                            stringResource(R.string.group_count, stringResource(group.title), lines.size),
                            color = readable(group.color),
                        )
                    }
                }
                items(lines, key = { it.key }) { line ->
                    SensorCard(
                        line = line,
                        accent = group.color,
                        modifier = Modifier.transformedHeight(this, spec),
                        transformation = SurfaceTransformation(spec),
                    )
                }
            }
        }
    }
}

@Composable
private fun SensorCard(line: SensorLine, accent: Color, modifier: Modifier, transformation: SurfaceTransformation) {
    val isTemp = line.unit == "temperature"
    val level = if (isTemp) Thresholds.temperature(line.key, line.value) else Level.Unknown
    val value = Format.sensor(line.unit, line.value)
    val valueColor = if (isTemp) temperatureColor(level) else MaterialTheme.colorScheme.onSurface
    val sourceLabel = sourceLabel(line.source, line.quality)
    Card(
        modifier = modifier
            .fillMaxWidth()
            .semantics(mergeDescendants = true) { contentDescription = "${line.label}, $value, $sourceLabel" },
        transformation = transformation,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(
                    line.label,
                    style = MaterialTheme.typography.labelMedium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    sourceLabel,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(6.dp))
            Text(value, style = MaterialTheme.typography.titleMedium, color = valueColor, maxLines = 1)
        }
        if (isTemp) {
            Spacer(Modifier.height(6.dp))
            // Relative to 100 °C, the point where every consumer part is in trouble,
            // so two bars side by side compare honestly across the list.
            Gauge(
                fraction = line.value / 100f,
                color = if (level == Level.Unknown) accent else levelColor(level),
                modifier = Modifier.padding(bottom = 2.dp),
            )
        }
    }
}

@Composable
private fun sourceLabel(source: String, quality: String): String {
    val s = when (source) {
        "acpiThermalZone" -> stringResource(R.string.source_acpi)
        "batteryMiniport" -> stringResource(R.string.source_battery)
        "systemPowerStatus" -> stringResource(R.string.source_os_power)
        "vendorLibrary" -> stringResource(R.string.source_vendor)
        "kernelDriver" -> stringResource(R.string.source_kernel)
        "storageDevice" -> stringResource(R.string.source_drive)
        else -> source
    }
    return when (quality) {
        "derived" -> stringResource(R.string.quality_derived, s)
        "nameplate" -> stringResource(R.string.quality_nameplate, s)
        else -> s
    }
}
