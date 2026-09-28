package app.vitals.device.internal

import app.vitals.device.model.DeviceAlert
import app.vitals.device.model.DeviceSnapshot

/**
 * Turns snapshots into alerts. Each rule needs its condition to hold for a
 * while before it fires and to clear with a margin before it stops (the same
 * hysteresis as the desktop), so a phone hovering at 44.9 °C does not flash
 * an alert every other second. Pure: the clock is passed in, so it is tested
 * without waiting.
 */
internal class AlertRules {
    private val since = HashMap<String, Long>()
    private val active = LinkedHashMap<String, DeviceAlert>()

    fun evaluate(s: DeviceSnapshot, storageFreeFraction: Float?, now: Long): List<DeviceAlert> {
        val thermal = when (s.thermal.status) {
            "severe", "critical", "emergency", "shutdown" -> "critical"
            "moderate" -> "warning"
            else -> null
        }
        rule("thermal", thermal, s.thermal.headroom, now, holdMs = 0)

        val temp = s.battery?.temperatureC
        rule(
            "batteryHot",
            when {
                temp == null -> null
                temp >= 45f -> "critical"
                temp >= 41f || (isActive("batteryHot") && temp >= 39f) -> "warning"
                else -> null
            },
            temp,
            now,
            holdMs = 30_000,
        )

        val battery = s.battery
        val low = battery != null && battery.status != "charging" && battery.status != "full"
        rule(
            "batteryLow",
            when {
                !low -> null
                battery.percent <= 5f -> "critical"
                battery.percent <= 15f || (isActive("batteryLow") && battery.percent <= 17f) -> "warning"
                else -> null
            },
            battery?.percent,
            now,
            holdMs = 0,
        )

        rule("memoryPressure", if (s.memory.lowMemory) "warning" else null, s.memory.usedBytes * 100f / s.memory.totalBytes, now, holdMs = 10_000)

        rule(
            "storageLow",
            when {
                storageFreeFraction == null -> null
                storageFreeFraction < 0.03f -> "critical"
                storageFreeFraction < 0.10f -> "warning"
                else -> null
            },
            storageFreeFraction?.times(100f),
            now,
            holdMs = 0,
        )

        val cpu = s.cpu.load
        rule(
            "cpuSustained",
            when {
                cpu == null -> null
                cpu >= 85f || (isActive("cpuSustained") && cpu >= 70f) -> "warning"
                else -> null
            },
            cpu,
            now,
            holdMs = 60_000,
        )
        return active.values.toList()
    }

    private fun isActive(kind: String) = kind in active

    private fun rule(kind: String, severity: String?, value: Float?, now: Long, holdMs: Long) {
        if (severity == null) {
            since.remove(kind)
            active.remove(kind)
            return
        }
        val start = since.getOrPut(kind) { now }
        if (now - start < holdMs) return
        val prior = active[kind]
        active[kind] = DeviceAlert(kind, severity, prior?.sinceMs ?: start, value)
    }
}
