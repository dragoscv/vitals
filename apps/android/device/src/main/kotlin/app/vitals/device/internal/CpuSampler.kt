package app.vitals.device.internal

import app.vitals.device.model.ClusterState
import app.vitals.device.model.CoreState

/**
 * CPU for an app that may not read `/proc/stat` (closed since Android 10).
 *
 * First choice is each core's cpuidle `stateN/time`: how long it sat idle, so
 * busy = 1 − idle / wall is real utilisation. Where the kernel keeps cpuidle
 * from apps, the fallback is `time_in_state`, weighting each frequency step
 * by how far above the minimum it is. That estimate assumes the governor
 * parks an idle core low, and it is badly wrong where it does not: the
 * Chromecast with Google TV's schedutil sits at its top step, and the
 * estimate read 64 % while top and cpuidle agreed on 15–25 % (2026-09-29).
 * The previous reading is kept per policy so each call yields a delta.
 */
internal class CpuSampler {
    private data class Policy(
        val id: Int,
        val dir: String,
        val cpus: List<Int>,
        val minKhz: Long?,
        val maxKhz: Long?,
        val role: String,
    )

    private val policies: List<Policy> by lazy { discover() }
    private val previous = HashMap<Int, LongArray>()

    /** One reading of a cluster's summed idle time, with the cores it covered. */
    private data class IdleMark(val wallUs: Long, val idleUs: Long, val cores: List<Int>)

    /**
     * The last [IDLE_WINDOW] readings per cluster. The kernel adds a core's
     * idle time only when the core wakes, so a core asleep across a one-second
     * boundary reads 100 % busy for that second and over 100 % idle for the
     * next. Clamping each second threw the correction away and read 32–41 %
     * on the Chromecast while top said 15–20 %; summing the cluster over five
     * seconds lets the late-counted idle land in the same window.
     */
    private val idleHistory = HashMap<Int, ArrayDeque<IdleMark>>()

    val coreCount: Int by lazy {
        SysFs.read("$ROOT/present")?.let { parseCpuList(it).size }?.takeIf { it > 0 }
            ?: Runtime.getRuntime().availableProcessors()
    }

    data class Reading(val cores: List<CoreState>, val clusters: List<ClusterState>, val load: Float?, val measured: Boolean)

    fun sample(): Reading {
        val wallUs = System.nanoTime() / 1_000
        val online = (0 until coreCount).associateWith { isOnline(it) }
        var allMeasured = true
        val clusters = policies.map { p ->
            val now = SysFs.read("${p.dir}/stats/time_in_state")?.let(::parseTimeInState)
            val before = previous[p.id]
            if (now != null) previous[p.id] = now
            val idle = measuredLoad(p, online, wallUs)
            if (idle == null) allMeasured = false
            ClusterState(
                id = p.id,
                role = p.role,
                cores = p.cpus,
                minFrequencyHz = p.minKhz?.times(1000),
                maxFrequencyHz = p.maxKhz?.times(1000),
                currentFrequencyHz = SysFs.readLong("${p.dir}/scaling_cur_freq")?.times(1000),
                // While cpuidle is readable its first reading has no window
                // yet: that second is unmeasured, not estimated. Falling back
                // put a 100 % spike at the start of every history session.
                load = when (idle) {
                    null -> if (now != null && before != null) clusterLoad(before, now) else null
                    else -> idle.load
                },
            )
        }
        val clusterOf = HashMap<Int, ClusterState>()
        clusters.forEach { c -> c.cores.forEach { clusterOf[it] = c } }
        val cores = (0 until coreCount).map { i ->
            val freq = SysFs.readLong("$ROOT/cpu$i/cpufreq/scaling_cur_freq")
            val on = online[i] == true || freq != null
            CoreState(
                index = i,
                online = on,
                frequencyHz = if (on) freq?.times(1000) else null,
                maxFrequencyHz = clusterOf[i]?.maxFrequencyHz,
                cluster = clusterOf[i]?.id ?: 0,
            )
        }
        return Reading(cores, clusters, overall(clusters), allMeasured && clusters.isNotEmpty())
    }

    /** A cluster whose cores all expose cpuidle; [load] is null until the window has two readings. */
    private class Idle(val load: Float?)

    /** Measured beats estimated: busy share of the cluster's online cores over the window, or null if any hides cpuidle. */
    private fun measuredLoad(p: Policy, online: Map<Int, Boolean>, wallUs: Long): Idle? {
        val cores = p.cpus.filter { online[it] == true }
        var sum = 0L
        for (c in cores) sum += idleUs(c) ?: return null.also { idleHistory.remove(p.id) }
        if (cores.isEmpty()) return null
        val history = idleHistory.getOrPut(p.id) { ArrayDeque() }
        // A core going on- or offline changes what the sum means; start over.
        if (history.isNotEmpty() && history.last().cores != cores) history.clear()
        history.addLast(IdleMark(wallUs, sum, cores))
        while (history.size > IDLE_WINDOW + 1) history.removeFirst()
        val oldest = history.first()
        if (oldest === history.last()) return Idle(null)
        return Idle(idleBusy(oldest.idleUs, sum, wallUs - oldest.wallUs, cores.size))
    }

    // cpu0 has no `online` node on most kernels: it cannot be unplugged.
    private fun isOnline(i: Int): Boolean = SysFs.read("$ROOT/cpu$i/online")?.trim()?.let { it == "1" } ?: (i == 0)

    /** Summed idle time of one core, µs, or null when the kernel hides cpuidle from apps. */
    private fun idleUs(i: Int): Long? {
        val states = SysFs.list("$ROOT/cpu$i/cpuidle").filter { it.name.startsWith("state") }
        if (states.isEmpty()) return null
        var sum = 0L
        for (s in states) sum += SysFs.readLong("${s.path}/time") ?: return null
        return sum
    }

    /** Core-weighted mean of the clusters that reported, or null when none did. */
    private fun overall(clusters: List<ClusterState>): Float? {
        var cores = 0
        var sum = 0f
        for (c in clusters) {
            val l = c.load ?: continue
            cores += c.cores.size
            sum += l * c.cores.size
        }
        return if (cores == 0) null else sum / cores
    }

    private fun discover(): List<Policy> {
        val dirs = SysFs.list("$ROOT/cpufreq")
            .filter { it.name.startsWith("policy") }
            .mapNotNull { f -> f.name.removePrefix("policy").toIntOrNull()?.let { it to f.path } }
            .sortedBy { it.first }
        val maxes = dirs.map { (_, d) -> SysFs.readLong("$d/cpuinfo_max_freq") ?: 0L }
        val roles = clusterRoles(maxes)
        return dirs.mapIndexed { i, (id, d) ->
            Policy(
                id = id,
                dir = d,
                cpus = SysFs.read("$d/related_cpus")?.let(::parseCpuList).orEmpty().ifEmpty { listOf(id) },
                minKhz = SysFs.readLong("$d/cpuinfo_min_freq"),
                maxKhz = maxes[i].takeIf { it > 0 },
                role = roles[i],
            )
        }
    }

    private companion object {
        const val ROOT = "/sys/devices/system/cpu"
        const val IDLE_WINDOW = 5
    }
}
