package app.vitals.device.internal

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// Inputs are the literal text read as the app's uid on a Galaxy S25 Ultra
// (Android 16), 2026-09-28, not what the kernel docs describe.
class ParseTest {
    @Test
    fun an_idle_cluster_parked_at_its_lowest_step_reads_zero() {
        val before = parseTimeInState("384000 100\n1000000 50\n3532800 10\n")
        val after = parseTimeInState("384000 200\n1000000 50\n3532800 10\n")
        assertEquals(0f, clusterLoad(before, after)!!, 0.01f)
    }

    @Test
    fun a_cluster_pinned_at_its_top_step_reads_one_hundred() {
        val before = parseTimeInState("384000 100\n3532800 10\n")
        val after = parseTimeInState("384000 100\n3532800 60\n")
        assertEquals(100f, clusterLoad(before, after)!!, 0.01f)
    }

    @Test
    fun time_split_across_steps_is_weighted_by_frequency() {
        // Half the interval at min, half at max -> 50.
        val before = parseTimeInState("400000 0\n800000 0\n1200000 0\n")
        val after = parseTimeInState("400000 50\n800000 0\n1200000 50\n")
        assertEquals(50f, clusterLoad(before, after)!!, 0.01f)
    }

    @Test
    fun an_offline_cluster_is_unmeasured_not_idle() {
        val same = parseTimeInState("384000 100\n3532800 10\n")
        assertNull(clusterLoad(same, same))
    }

    @Test
    fun a_counter_reset_by_hotplug_is_unmeasured_rather_than_negative() {
        val before = parseTimeInState("384000 500\n3532800 500\n")
        val after = parseTimeInState("384000 10\n3532800 10\n")
        assertNull(clusterLoad(before, after))
    }

    @Test
    fun cpu_lists_parse_in_every_kernel_shape() {
        assertEquals((0..7).toList(), parseCpuList("0-7\n"))
        assertEquals((0..5).toList(), parseCpuList("0 1 2 3 4 5\n"))
        assertEquals(listOf(0, 1, 2, 3, 6), parseCpuList("0-3,6"))
    }

    @Test
    fun samsung_gpu_busy_percent_parses() {
        assertEquals(37f, parseGpuBusy("37 %\n")!!, 0.01f)
        assertEquals(0f, parseGpuBusy("0 %")!!, 0.01f)
    }

    @Test
    fun kgsl_zero_over_zero_is_no_window_not_an_idle_gpu() {
        assertNull(parseGpuBusy("      0       0\n"))
        assertEquals(25f, parseGpuBusy("  250  1000")!!, 0.01f)
    }

    @Test
    fun thermal_zones_drop_alarm_levels_and_disconnected_sensors() {
        assertEquals(41.5f, zoneCelsius("cpu-0-0-0", "41500")!!, 0.01f)
        assertNull(zoneCelsius("pm8550-bcl-lvl0", "0"))
        assertNull(zoneCelsius("sdr0", "-273000"))
        assertEquals(35f, zoneCelsius("battery", "35")!!, 0.01f)
    }

    @Test
    fun zone_names_from_the_s25_land_in_the_right_groups() {
        assertEquals("cpu", zoneGroup("cpu-1-0-0"))
        assertEquals("cpu", zoneGroup("cpuss-0-1"))
        assertEquals("gpu", zoneGroup("gpuss-3"))
        assertEquals("battery", zoneGroup("battery"))
        assertEquals("skin", zoneGroup("sys-therm-0"))
        assertEquals("modem", zoneGroup("mdmss-2"))
        assertEquals("npu", zoneGroup("nsphvx-0"))
        assertEquals("memory", zoneGroup("ddr"))
    }

    @Test
    fun cluster_roles_rank_by_max_frequency() {
        // A51 (Exynos 9611): 1.7 / 2.3 GHz is little + big.
        assertEquals(listOf("efficiency", "performance"), clusterRoles(listOf(1_742_000, 2_314_000)))
        // S25 Ultra (8 Elite): 3.5 / 4.5 GHz is performance + prime, no little cores.
        assertEquals(listOf("performance", "prime"), clusterRoles(listOf(3_532_800, 4_473_600)))
        assertEquals(listOf("efficiency", "performance", "prime"), clusterRoles(listOf(1_800_000, 2_400_000, 3_000_000)))
        assertEquals(listOf("cpu"), clusterRoles(listOf(1_700_000)))
    }

    @Test
    fun meminfo_is_converted_to_bytes() {
        val m = parseMeminfo("MemTotal:       11534336 kB\nMemAvailable:    4194304 kB\nSwapTotal: 0 kB\n")
        assertEquals(11534336L * 1024, m["MemTotal"])
        assertEquals(0L, m["SwapTotal"])
    }

    @Test
    fun gpu_clocks_in_mhz_khz_and_hz_all_land_in_hz() {
        assertEquals(1_200_000_000L, gpuClockHz(1200)) // Adreno 830, /sys/kernel/gpu
        assertEquals(1_053_000_000L, gpuClockHz(1_053_000)) // Mali-G72, A51
        assertEquals(1_200_000_000L, gpuClockHz(1_200_000_000)) // kgsl
    }

    @Test
    fun a_powered_off_gpu_has_no_clock_rather_than_zero_hertz() {
        assertNull(gpuClockHz(0))
    }

    @Test
    fun battery_current_is_microamps_even_at_a_trickle() {
        // S25 Ultra, plugged in and held at 85 %: dumpsys "current now: -20312" and "5468".
        assertEquals(-20.3f, currentToMa(-20_312), 0.1f)
        assertEquals(5.5f, currentToMa(5_468), 0.1f)
        assertEquals(523.4f, currentToMa(523_437), 0.1f)
    }
}
