package app.vitals.device.internal

import android.os.Build
import android.os.PowerManager
import app.vitals.device.model.ThermalState
import app.vitals.device.model.ThermalZone

/**
 * PowerManager's verdict plus the raw zones. The zone list is discovered once
 * and narrowed to the ones that actually read as temperatures: the S25 has
 * 80-odd zones, a third of them alarm levels or disconnected sensors, and
 * reading those every second would be work for nothing.
 */
internal class ThermalSampler(private val power: PowerManager) {
    private data class Zone(val type: String, val path: String)

    private val zones: List<Zone> by lazy { discover() }
    private var lastHeadroom: Float? = null
    private var lastHeadroomAt = 0L

    fun sample(): ThermalState {
        val readings = zones.mapNotNull { z ->
            SysFs.read(z.path)?.let { zoneCelsius(z.type, it) }?.let { ThermalZone(z.type, zoneGroup(z.type), it) }
        }
        return ThermalState(status = status(), headroom = headroom(), zones = readings)
    }

    private fun status(): String = when (power.currentThermalStatus) {
        PowerManager.THERMAL_STATUS_NONE -> "none"
        PowerManager.THERMAL_STATUS_LIGHT -> "light"
        PowerManager.THERMAL_STATUS_MODERATE -> "moderate"
        PowerManager.THERMAL_STATUS_SEVERE -> "severe"
        PowerManager.THERMAL_STATUS_CRITICAL -> "critical"
        PowerManager.THERMAL_STATUS_EMERGENCY -> "emergency"
        PowerManager.THERMAL_STATUS_SHUTDOWN -> "shutdown"
        else -> "none"
    }

    /**
     * The platform returns NaN when asked more than about once a second, so
     * the last good value is reused inside that window instead of flickering
     * to "unknown".
     */
    private fun headroom(): Float? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return null
        val now = System.currentTimeMillis()
        if (now - lastHeadroomAt < 1_500) return lastHeadroom
        lastHeadroomAt = now
        val h = power.getThermalHeadroom(10)
        if (!h.isNaN() && h >= 0f) lastHeadroom = h
        return lastHeadroom
    }

    private fun discover(): List<Zone> = SysFs.list("/sys/class/thermal")
        .filter { it.name.startsWith("thermal_zone") }
        .mapNotNull { dir ->
            val type = SysFs.read("${dir.path}/type")?.trim() ?: return@mapNotNull null
            val path = "${dir.path}/temp"
            SysFs.read(path)?.let { zoneCelsius(type, it) } ?: return@mapNotNull null
            Zone(type, path)
        }
}
