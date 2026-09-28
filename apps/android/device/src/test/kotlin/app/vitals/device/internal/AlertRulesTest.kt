package app.vitals.device.internal

import app.vitals.device.model.BatteryState
import app.vitals.device.model.CpuState
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.MemoryState
import app.vitals.device.model.NetworkState
import app.vitals.device.model.ThermalState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AlertRulesTest {
    private fun snap(cpu: Float? = 10f, batteryTemp: Float? = 30f, percent: Float = 80f, status: String = "discharging", thermal: String = "none") =
        DeviceSnapshot(
            timestampMs = 0,
            cpu = CpuState(emptyList(), emptyList(), cpu, null),
            gpu = null,
            memory = MemoryState(100, 50, 50, null, null, null, false, null),
            battery = BatteryState(percent, status, null, "good", batteryTemp, null, null, null, null, null, null, null),
            thermal = ThermalState(thermal, null, emptyList()),
            network = NetworkState(null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null),
        )

    @Test
    fun a_short_cpu_spike_does_not_alert_but_a_minute_of_it_does() {
        val r = AlertRules()
        assertTrue(r.evaluate(snap(cpu = 95f), null, 0).isEmpty())
        assertTrue(r.evaluate(snap(cpu = 95f), null, 30_000).isEmpty())
        assertEquals("cpuSustained", r.evaluate(snap(cpu = 95f), null, 61_000).single().kind)
    }

    @Test
    fun a_spike_that_drops_resets_the_clock() {
        val r = AlertRules()
        r.evaluate(snap(cpu = 95f), null, 0)
        r.evaluate(snap(cpu = 20f), null, 30_000)
        assertTrue(r.evaluate(snap(cpu = 95f), null, 61_000).isEmpty())
    }

    @Test
    fun an_active_battery_heat_alert_holds_until_it_clears_the_margin() {
        val r = AlertRules()
        r.evaluate(snap(batteryTemp = 42f), null, 0)
        assertEquals(1, r.evaluate(snap(batteryTemp = 42f), null, 31_000).size)
        // 40 is below the 41 trigger but above the 39 release: still hot.
        assertEquals(1, r.evaluate(snap(batteryTemp = 40f), null, 32_000).size)
        assertTrue(r.evaluate(snap(batteryTemp = 38f), null, 33_000).isEmpty())
    }

    @Test
    fun a_low_battery_that_is_charging_is_not_an_alert() {
        val r = AlertRules()
        assertTrue(r.evaluate(snap(percent = 4f, status = "charging"), null, 0).isEmpty())
        assertEquals("critical", r.evaluate(snap(percent = 4f), null, 1).single().severity)
    }

    @Test
    fun an_unmeasured_cpu_never_alerts() {
        val r = AlertRules()
        r.evaluate(snap(cpu = null), null, 0)
        assertTrue(r.evaluate(snap(cpu = null), null, 120_000).isEmpty())
    }

    @Test
    fun platform_thermal_throttling_alerts_at_once() {
        assertEquals("critical", AlertRules().evaluate(snap(thermal = "severe"), null, 0).single().severity)
    }
}
