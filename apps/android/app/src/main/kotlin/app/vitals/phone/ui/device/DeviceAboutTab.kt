package app.vitals.phone.ui.device

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.device.model.DeviceInfo
import app.vitals.phone.R
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.components.SectionCard
import app.vitals.phone.ui.relativeTime
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlin.math.roundToInt

@Composable
internal fun DeviceAboutTab(info: DeviceInfo?, padding: PaddingValues) {
    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxSize()) {
        if (info == null) {
            item { Text(stringResource(R.string.device_loading)) }
            return@LazyColumn
        }
        item(key = "device") {
            SectionCard(stringResource(R.string.device_tab_about), accent = Palette.Accent) {
                InfoRow(stringResource(R.string.device_manufacturer), info.manufacturer)
                InfoRow(stringResource(R.string.device_model), info.model)
                info.marketingName?.let { InfoRow(stringResource(R.string.field_name), it) }
                InfoRow(stringResource(R.string.device_codename), info.device)
                InfoRow("Android", "${info.androidVersion} (API ${info.sdkInt})")
                InfoRow(stringResource(R.string.device_security_patch), info.securityPatch ?: Format.DASH)
                InfoRow(stringResource(R.string.device_kernel), info.kernel ?: Format.DASH)
                InfoRow(stringResource(R.string.device_started), relativeTime(info.bootTimeMs))
            }
        }
        item(key = "chip") {
            SectionCard(stringResource(R.string.device_chip), accent = Palette.Cpu) {
                InfoRow(stringResource(R.string.device_soc), listOfNotNull(info.socManufacturer, info.soc).joinToString(" ").ifEmpty { Format.DASH })
                InfoRow(stringResource(R.string.device_core_count), info.cpuCores.toString())
                InfoRow("ABI", info.abis.joinToString(", "))
                InfoRow(stringResource(R.string.device_total_memory), Format.bytes(info.totalMemoryBytes))
                InfoRow("OpenGL ES", info.glEsVersion ?: Format.DASH)
                InfoRow("Vulkan", stringResource(if (info.vulkan) R.string.device_yes else R.string.device_no))
                if (info.mediaPerformanceClass > 0) InfoRow(stringResource(R.string.device_perf_class), info.mediaPerformanceClass.toString())
                if (info.isLowRam) InfoRow(stringResource(R.string.device_low_ram), stringResource(R.string.device_yes))
            }
        }
        item(key = "display") {
            SectionCard(stringResource(R.string.device_display), accent = Palette.Network) {
                InfoRow(stringResource(R.string.device_resolution), "${info.displayWidthPx} × ${info.displayHeightPx}")
                InfoRow(stringResource(R.string.device_density), "${info.displayDensityDpi} dpi")
                InfoRow(
                    stringResource(R.string.device_refresh),
                    info.refreshRatesHz.joinToString(" / ") { "${it.roundToInt()} Hz" }.ifEmpty { Format.DASH },
                )
                InfoRow("HDR", info.hdr.joinToString(", ").ifEmpty { Format.DASH })
            }
        }
    }
}
