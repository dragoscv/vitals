// The present state of one PC, rebuilt from a stream of frames.
//
// The same fold as the server's FrameSource, the desktop's metrics.ts and
// Android's MachineView: a keyframe replaces everything; a delta removes
// `exited` BEFORE upserting `changed`, so a PID that exited and was reused
// within one tick keeps its new owner. A delta before any keyframe is
// dropped — half a machine is not a snapshot.

import Foundation

public struct MachineView: Sendable {
    public let seq: Int64
    public let timestampMs: Int64
    public let system: SystemMetrics
    public let processes: [Int: Process]

    /// nil when `frame` is a delta: there is nothing to apply it to.
    public static func of(_ frame: Frame) -> MachineView? {
        guard case let .keyframe(system, processes) = frame.payload else { return nil }
        var byPid = [Int: Process](minimumCapacity: processes.count)
        for p in processes { byPid[p.key.pid] = p }
        return MachineView(seq: frame.seq, timestampMs: frame.timestampMs, system: system, processes: byPid)
    }

    public func applying(_ frame: Frame) -> MachineView {
        switch frame.payload {
        case .keyframe:
            return MachineView.of(frame) ?? self
        case let .delta(system, changed, exited):
            var next = processes
            for pid in exited { next.removeValue(forKey: pid) }
            for p in changed { next[p.key.pid] = p }
            return MachineView(seq: frame.seq, timestampMs: frame.timestampMs, system: system, processes: next)
        }
    }

    /// Folds `frame` into `current`, or starts from it.
    public static func fold(_ current: MachineView?, _ frame: Frame) -> MachineView? {
        current?.applying(frame) ?? of(frame)
    }
}
