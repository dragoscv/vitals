package app.vitals.ui

import app.vitals.core.model.GpuMetrics
import app.vitals.core.model.SystemMetrics

/**
 * Derived headline numbers, computed the same way on every Android surface.
 */
object Readings {
    /**
     * The GPU to headline: the busiest one that reports utilisation, else the
     * first with a name. Mirrors `primaryGpu` in `@vitals/protocol`, so a
     * machine with a virtual display adapter does not show "GPU —" beside a
     * real GPU at 16 %.
     */
    fun primaryGpu(system: SystemMetrics): GpuMetrics? =
        system.gpus.filter { it.utilization != null }.maxByOrNull { it.utilization ?: 0f }
            ?: system.gpus.firstOrNull()

    fun memoryPercent(system: SystemMetrics): Float? =
        system.memory.takeIf { it.total > 0 }?.let { it.used * 100f / it.total }

    /** The hottest CPU reading available, or null. */
    fun cpuTemperature(system: SystemMetrics): Float? = system.cpu.temperature

    /** Whole-machine network throughput across connected adapters, bytes/s. */
    fun networkRx(system: SystemMetrics): Long = system.networks.filter { it.connected }.sumOf { it.rx }
    fun networkTx(system: SystemMetrics): Long = system.networks.filter { it.connected }.sumOf { it.tx }

    /** The MAC to wake: the connected wired adapter first, then any connected one. */
    fun wakeMac(system: SystemMetrics): Pair<String, String?>? {
        val candidates = system.networks.filter { it.connected && it.mac != null }
        val nic = candidates.firstOrNull { it.kind == app.vitals.core.model.NetworkKind.Ethernet }
            ?: candidates.firstOrNull()
            ?: return null
        return nic.mac!! to nic.ipv4
    }
}
