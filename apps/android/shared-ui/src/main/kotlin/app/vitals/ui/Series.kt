package app.vitals.ui

import androidx.compose.runtime.Immutable

/**
 * The last [CAPACITY] readings of one metric. `NaN` marks a sample where it
 * was not measured, so a sparkline breaks rather than dropping to zero.
 * Shared by the phone and the TV, which draw the same minute of history.
 */
@Immutable
class Series private constructor(private val values: FloatArray, val size: Int) {
    operator fun get(i: Int): Float = values[i]

    fun plus(v: Float?): Series {
        val next = FloatArray(CAPACITY)
        val keep = minOf(size, CAPACITY - 1)
        System.arraycopy(values, size - keep, next, 0, keep)
        next[keep] = v ?: Float.NaN
        return Series(next, keep + 1)
    }

    /** The newest reading, or null when there is none or it was not measured. */
    fun last(): Float? = if (size == 0) null else values[size - 1].takeUnless { it.isNaN() }

    companion object {
        const val CAPACITY = 60
        val Empty = Series(FloatArray(CAPACITY), 0)
    }
}

/**
 * One chart's points, bucketed to at most [MAX_CHART_POINTS] so a week of
 * one-second samples does not become 600 000 path segments. `NaN` is a
 * bucket where the metric was never measured.
 */
@Immutable
class Chart(val points: FloatArray, val max: Float, val peak: Float?)

const val MAX_CHART_POINTS = 240

fun <T> chart(samples: List<T>, max: Float, pick: (T) -> Float?): Chart {
    if (samples.isEmpty()) return Chart(FloatArray(0), max, null)
    val buckets = minOf(MAX_CHART_POINTS, samples.size)
    val out = FloatArray(buckets) { Float.NaN }
    val per = samples.size.toFloat() / buckets
    var peak: Float? = null
    for (b in 0 until buckets) {
        val from = (b * per).toInt()
        val to = ((b + 1) * per).toInt().coerceAtMost(samples.size).coerceAtLeast(from + 1)
        var sum = 0f
        var n = 0
        for (i in from until to) {
            val v = pick(samples[i]) ?: continue
            sum += v
            n++
            if (peak == null || v > peak) peak = v
        }
        if (n > 0) out[b] = sum / n
    }
    return Chart(out, max, peak)
}
