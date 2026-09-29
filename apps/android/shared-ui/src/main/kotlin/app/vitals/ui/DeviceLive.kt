package app.vitals.ui

import androidx.compose.runtime.Immutable
import app.vitals.device.model.DeviceSnapshot

/** This device's latest reading plus a minute of each headline metric, for the sparklines. */
@Immutable
data class DeviceLive(
    val snapshot: DeviceSnapshot? = null,
    val cpu: Series = Series.Empty,
    val gpu: Series = Series.Empty,
    val memory: Series = Series.Empty,
    val temperature: Series = Series.Empty,
    val battery: Series = Series.Empty,
) {
    /** Folds the next snapshot in; the one place the phone and the TV derive their sparklines. */
    fun next(s: DeviceSnapshot): DeviceLive = DeviceLive(
        snapshot = s,
        cpu = cpu.plus(s.cpu.load),
        gpu = gpu.plus(s.gpu?.load),
        memory = memory.plus(if (s.memory.totalBytes > 0) s.memory.usedBytes * 100f / s.memory.totalBytes else null),
        temperature = temperature.plus(s.cpu.temperature),
        battery = battery.plus(s.battery?.percent),
    )
}
