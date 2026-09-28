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
    val fetchedMs: Long,
    val reachable: Boolean,
    val cpu: Float?,
    val memory: Float?,
    val gpu: Float?,
    val cpuTemp: Float?,
    val worst: Severity?,
) {
    companion object {
        fun of(state: PcState): Headline {
            val system = state.summary?.system
            return Headline(
                label = state.label,
                fetchedMs = state.fetchedMs,
                reachable = system != null,
                cpu = system?.cpu?.total,
                memory = system?.let(Readings::memoryPercent),
                gpu = system?.let(Readings::primaryGpu)?.utilization,
                cpuTemp = system?.let(Readings::cpuTemperature),
                worst = state.alerts.maxOfOrNull { it.severity },
            )
        }

        /** The PC a tile or complication shows: the one updated most recently. */
        fun primary(states: Collection<PcState>): PcState? = states.maxByOrNull { it.fetchedMs }
    }
}
