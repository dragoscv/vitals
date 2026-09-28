package app.vitals.core

import app.vitals.core.model.Frame
import app.vitals.core.model.FramePayload
import app.vitals.core.model.Process
import app.vitals.core.model.SystemMetrics

/**
 * The present state of one PC, rebuilt from a stream of frames.
 *
 * The same fold the server's `FrameSource` and the desktop's `metrics.ts`
 * perform: a keyframe replaces everything; a delta removes `exited` **before**
 * inserting `changed`, so a PID that exited and was reused within one tick
 * keeps its new owner. A delta before any keyframe is dropped — half a
 * machine is not a snapshot.
 *
 * Immutable: each [apply] returns a new view, so Compose can compare by
 * reference and skip recomposition of rows whose process did not change.
 */
class MachineView private constructor(
    val seq: Long,
    val timestampMs: Long,
    val system: SystemMetrics,
    val processes: Map<Int, Process>,
) {
    /** `null` when [frame] is a delta and there is nothing to apply it to. */
    fun apply(frame: Frame): MachineView = when (val p = frame.payload) {
        is FramePayload.Keyframe -> of(frame)!!
        is FramePayload.Delta -> {
            val next = HashMap(processes)
            for (pid in p.exited) next.remove(pid)
            for (proc in p.changed) next[proc.key.pid] = proc
            MachineView(frame.seq, frame.timestampMs, p.system, next)
        }
    }

    companion object {
        fun of(frame: Frame): MachineView? = when (val p = frame.payload) {
            is FramePayload.Keyframe -> MachineView(
                frame.seq,
                frame.timestampMs,
                p.system,
                p.processes.associateBy { it.key.pid },
            )
            is FramePayload.Delta -> null
        }

        /** Folds [frame] into [current], or starts from it. */
        fun fold(current: MachineView?, frame: Frame): MachineView? =
            current?.apply(frame) ?: of(frame)
    }
}
