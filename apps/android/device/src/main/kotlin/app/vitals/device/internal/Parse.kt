package app.vitals.device.internal

// Pure parsers for the kernel files an app may still read. Kept free of
// Android types so they are unit-tested against text captured from the real
// phones (S25 Ultra, A51, Watch 7) rather than against what the docs claim.

/** `time_in_state`: one "kHz ticks" pair per line, ticks in USER_HZ (10 ms). */
internal fun parseTimeInState(text: String): LongArray {
    val out = ArrayList<Long>()
    for (line in text.lineSequence()) {
        val parts = line.trim().split(WHITESPACE)
        if (parts.size < 2) continue
        val khz = parts[0].toLongOrNull() ?: continue
        val ticks = parts[1].toLongOrNull() ?: continue
        out += khz
        out += ticks
    }
    return out.toLongArray()
}

/**
 * Frequency-weighted residency between two `time_in_state` readings, 0..100.
 * Each step counts (f - min) / (max - min) of its time: an idle cluster is
 * parked at the lowest step and reads 0, a saturated one runs at the top and
 * reads 100. Null when no time passed, which means the cluster was offline or
 * the file was reset (hotplug) and a delta would be meaningless.
 */
internal fun clusterLoad(previous: LongArray, current: LongArray): Float? {
    if (current.size < 4 || previous.size != current.size) return null
    var min = Long.MAX_VALUE
    var max = Long.MIN_VALUE
    for (i in current.indices step 2) {
        min = minOf(min, current[i])
        max = maxOf(max, current[i])
    }
    if (max <= min) return null
    var total = 0.0
    var weighted = 0.0
    for (i in current.indices step 2) {
        if (current[i] != previous[i]) return null
        val dt = current[i + 1] - previous[i + 1]
        if (dt < 0) return null
        total += dt
        weighted += dt * (current[i] - min).toDouble() / (max - min)
    }
    if (total <= 0.0) return null
    return (weighted / total * 100.0).toFloat().coerceIn(0f, 100f)
}

/** A kernel cpu list: "0-7", "0 1 2 3 4 5", "0-3,6". */
internal fun parseCpuList(text: String): List<Int> {
    val out = ArrayList<Int>()
    for (token in text.trim().split(',', ' ', '\n').filter { it.isNotBlank() }) {
        val dash = token.indexOf('-')
        if (dash > 0) {
            val a = token.substring(0, dash).toIntOrNull() ?: continue
            val b = token.substring(dash + 1).toIntOrNull() ?: continue
            for (i in a..b) out += i
        } else {
            token.toIntOrNull()?.let { out += it }
        }
    }
    return out
}

/** `/proc/meminfo` in bytes, keyed by field name. */
internal fun parseMeminfo(text: String): Map<String, Long> {
    val out = HashMap<String, Long>()
    for (line in text.lineSequence()) {
        val colon = line.indexOf(':')
        if (colon <= 0) continue
        val parts = line.substring(colon + 1).trim().split(WHITESPACE)
        val value = parts.firstOrNull()?.toLongOrNull() ?: continue
        val unit = parts.getOrNull(1)
        out[line.substring(0, colon)] = if (unit == "kB") value * 1024 else value
    }
    return out
}

/**
 * GPU busy in either of the two shapes phones expose: `/sys/kernel/gpu/gpu_busy`
 * ("37 %") or kgsl `gpubusy` ("busy total" since the last read). A kgsl
 * reading of "0 0" means no window has elapsed, not an idle GPU, so it is null.
 */
internal fun parseGpuBusy(text: String): Float? {
    val t = text.trim()
    if (t.isEmpty()) return null
    if (t.endsWith("%")) return t.removeSuffix("%").trim().toFloatOrNull()?.coerceIn(0f, 100f)
    val parts = t.split(WHITESPACE)
    if (parts.size >= 2) {
        val busy = parts[0].toDoubleOrNull() ?: return null
        val total = parts[1].toDoubleOrNull() ?: return null
        if (total <= 0.0) return null
        return (busy / total * 100.0).toFloat().coerceIn(0f, 100f)
    }
    return t.toFloatOrNull()?.coerceIn(0f, 100f)
}

/**
 * A thermal zone reading in °C, or null when it is not a temperature. Zones
 * report millidegrees except a few that report degrees; a disconnected
 * sensor reads -273 000, and the battery-current-limit zones (`bcl`) are
 * alarm levels that read 0.
 */
internal fun zoneCelsius(type: String, raw: String): Float? {
    if (type.contains("bcl", ignoreCase = true) || type.contains("lvl", ignoreCase = true)) return null
    val v = raw.trim().toLongOrNull() ?: return null
    val c = if (v > 1_000 || v < -1_000) v / 1000f else v.toFloat()
    return c.takeIf { it > -40f && it < 150f && v != 0L }
}

/** A readable group for a zone name, so 60 zones collapse into a handful of lines. */
internal fun zoneGroup(type: String): String {
    val t = type.lowercase()
    return when {
        t.startsWith("cpu") || t.contains("cluster") || t.startsWith("little") || t.startsWith("big") || t.startsWith("mid") -> "cpu"
        t.startsWith("gpu") || t.contains("g3d") || t.contains("mali") -> "gpu"
        t.contains("battery") || t == "bms" -> "battery"
        t.contains("skin") || t.contains("sys-therm") || t.contains("quiet") || t.contains("xo-therm") || t.contains("ap_therm") -> "skin"
        t.contains("mdm") || t.contains("modem") || t.startsWith("sdr") || t.contains("pa_therm") -> "modem"
        t.contains("camera") -> "camera"
        t.contains("ddr") || t.contains("mem") -> "memory"
        t.contains("nsp") || t.contains("npu") -> "npu"
        else -> "other"
    }
}

/**
 * Cluster roles by max-frequency rank. With two clusters the slower one is
 * "efficiency" only when it is capped like a little core (at most 2.2 GHz):
 * the A51's 1.74 / 2.31 GHz split is little/big, but the S25's 3.53 / 4.47
 * GHz split is performance and prime, and a ratio cannot tell them apart —
 * 75 % and 79 %, measured. No little core ships above 2.2 GHz; every big
 * core since 2018 does.
 */
internal fun clusterRoles(maxFrequencies: List<Long>): List<String> {
    val distinct = maxFrequencies.distinct().sortedDescending()
    return maxFrequencies.map { f ->
        when (distinct.size) {
            1 -> "cpu"
            2 -> when {
                distinct[1] <= LITTLE_CORE_MAX_KHZ -> if (f == distinct[0]) "performance" else "efficiency"
                else -> if (f == distinct[0]) "prime" else "performance"
            }
            else -> when (f) {
                distinct[0] -> "prime"
                distinct.last() -> "efficiency"
                else -> "performance"
            }
        }
    }
}

private const val LITTLE_CORE_MAX_KHZ = 2_200_000L

/**
 * A GPU clock in Hz, whatever unit the driver chose. Measured: Adreno under
 * `/sys/kernel/gpu` reports MHz ("1200", S25 Ultra), Mali reports kHz
 * ("1053000", A51), kgsl reports Hz ("1200000000"). No GPU runs below
 * 10 MHz or above 10 GHz, so the magnitude decides. 0 is a powered-off GPU,
 * which has no clock rather than a clock of zero.
 */
internal fun gpuClockHz(v: Long): Long? = when {
    v <= 0 -> null
    v < 10_000 -> v * 1_000_000
    v < 10_000_000 -> v * 1_000
    else -> v
}

private val WHITESPACE = Regex("\\s+")

/**
 * BATTERY_PROPERTY_CURRENT_NOW is microamps, and is treated as exactly that.
 * A size cutoff to catch gauges that report milliamps was tried and was
 * wrong both ways on one phone: the S25 Ultra reads 5 468 µA at a trickle
 * and 523 437 µA charging (dumpsys, 2026-09-29), so no threshold separates
 * "small µA" from "mA".
 */
internal fun currentToMa(raw: Int): Float = raw / 1000f
