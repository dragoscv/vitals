package app.vitals.device.internal

import app.vitals.device.model.ClusterState
import app.vitals.device.model.CoreState

/**
 * CPU from cpufreq. Android 10+ closes `/proc/stat` to apps, so true
 * utilisation is out of reach; `time_in_state` is still readable and says how
 * long each cluster spent at each frequency, which the governor raises with
 * load. The previous reading is kept per policy so each call yields a delta.
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

    val coreCount: Int by lazy {
        SysFs.read("$ROOT/present")?.let { parseCpuList(it).size }?.takeIf { it > 0 }
            ?: Runtime.getRuntime().availableProcessors()
    }

    data class Reading(val cores: List<CoreState>, val clusters: List<ClusterState>, val load: Float?)

    fun sample(): Reading {
        val clusters = policies.map { p ->
            val now = SysFs.read("${p.dir}/stats/time_in_state")?.let(::parseTimeInState)
            val before = previous[p.id]
            if (now != null) previous[p.id] = now
            ClusterState(
                id = p.id,
                role = p.role,
                cores = p.cpus,
                minFrequencyHz = p.minKhz?.times(1000),
                maxFrequencyHz = p.maxKhz?.times(1000),
                currentFrequencyHz = SysFs.readLong("${p.dir}/scaling_cur_freq")?.times(1000),
                load = if (now != null && before != null) clusterLoad(before, now) else null,
            )
        }
        val clusterOf = HashMap<Int, ClusterState>()
        clusters.forEach { c -> c.cores.forEach { clusterOf[it] = c } }
        val cores = (0 until coreCount).map { i ->
            val freq = SysFs.readLong("$ROOT/cpu$i/cpufreq/scaling_cur_freq")
            // cpu0 has no `online` node on most kernels: it cannot be unplugged.
            val online = SysFs.read("$ROOT/cpu$i/online")?.trim()?.let { it == "1" } ?: (i == 0 || freq != null)
            CoreState(
                index = i,
                online = online,
                frequencyHz = if (online) freq?.times(1000) else null,
                maxFrequencyHz = clusterOf[i]?.maxFrequencyHz,
                cluster = clusterOf[i]?.id ?: 0,
            )
        }
        return Reading(cores, clusters, overall(clusters))
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
    }
}
