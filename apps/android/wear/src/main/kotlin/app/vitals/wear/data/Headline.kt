package app.vitals.wear.data

import app.vitals.core.model.Severity
import app.vitals.core.wear.PcState
import app.vitals.ui.Readings

/**
 * The few numbers every watch surface headlines, derived once so the app,
 * the tile and the complications can never disagree. Each is `null` when the
 * PC did not measure it — including when the PC is unreachable, because a
 * last-known reading shown as current would be a lie with a number on it.
 */
data class Headline(
    val label: String,
    /** When the PC last answered: the age a surface prints, reached or not. */
    val fetchedMs: Long,
    val reachable: Boolean,
    val cpu: Float?,
    val memory: Float?,
    val gpu: Float?,
    val cpuTemp: Float?,
    val worst: Severity?,
) {
    companion object {
        /**
         * Older than this, a cached reading is not shown as current (AND-20):
         * a watch face that kept "CPU 12 %" for an hour after the PC went to
         * sleep was showing a lie with a number on it.
         */
        const val STALE_AFTER_MS = 10 * 60_000L

        fun of(state: PcState, nowMs: Long = System.currentTimeMillis()): Headline {
            val fresh = nowMs - state.fetchedMs <= STALE_AFTER_MS
            val system = state.summary?.system?.takeIf { fresh }
            return Headline(
                label = state.label,
                fetchedMs = state.lastSeenMs,
                reachable = system != null,
                cpu = system?.cpu?.total,
                memory = system?.let(Readings::memoryPercent),
                gpu = system?.let(Readings::primaryGpu)?.utilization,
                cpuTemp = system?.let(Readings::cpuTemperature),
                // A severity a newer desktop added ranks as a warning, not
                // above critical (Unknown is the last enum entry).
                worst = state.alerts.maxOfOrNull { if (it.severity == Severity.Unknown) Severity.Warning else it.severity },
            )
        }

        /** The PC a tile or complication shows: the one updated most recently. */
        fun primary(states: Collection<PcState>): PcState? = states.maxByOrNull { it.fetchedMs }
    }
}
