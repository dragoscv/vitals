// What the iPhone hands the Apple Watch over WatchConnectivity: a compact
// picture of the PCs the phone already watches. The watch has no pairing of
// its own, so nothing that could reach a PC leaves the phone — a snapshot is
// built from a pairing's id and label only, never its token, address or MAC.
// The encoded keys are a closed list (see WatchSnapshotTests) so adding a
// field to Pairing can never leak it to the watch by accident.
//
// Same rule as every other surface: an unmeasured reading is nil and the
// watch shows "—". Readings are sent only while the PC is live; a stale
// number beside "Can’t reach" would read as current.

import Foundation

public enum WatchPcState: String, TolerantEnum, Encodable {
    case live, connecting, offline, offLan, unauthorised, incompatible
    /// A state a newer phone app sends that this watch app does not know.
    case unknown
    public static let unknownCase = Self.unknown
}

public struct WatchPc: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let label: String
    public let state: WatchPcState
    public let cpuPercent: Double?
    public let memoryPercent: Double?
    public let tempC: Double?
    public let gpuPercent: Double?
    /// Warnings and critical alerts; informational ones are not counted,
    /// matching the iPhone's alert chip.
    public let alerts: Int
    public let lastSeenMs: Int64?
    /// The labelled Sample PC, never presented as a real machine.
    public let sample: Bool

    public init(
        id: String, label: String, state: WatchPcState,
        cpuPercent: Double?, memoryPercent: Double?, tempC: Double?, gpuPercent: Double?,
        alerts: Int, lastSeenMs: Int64?, sample: Bool
    ) {
        self.id = id
        self.label = label
        self.state = state
        self.cpuPercent = Self.finite(cpuPercent)
        self.memoryPercent = Self.finite(memoryPercent)
        self.tempC = Self.finite(tempC)
        self.gpuPercent = Self.finite(gpuPercent)
        self.alerts = max(0, alerts)
        self.lastSeenMs = lastSeenMs
        self.sample = sample
    }

    /// From a pairing and its live stream. Only `id` and `label` are read from
    /// the pairing: the token and address stay on the phone.
    public init(
        pairing: Pairing, state: WatchPcState, system: SystemMetrics?,
        alerts: Int, lastSeenMs: Int64?, sample: Bool = false
    ) {
        let live = state == .live ? system : nil
        self.init(
            id: pairing.id, label: pairing.label, state: state,
            cpuPercent: live?.cpu.total,
            memoryPercent: live?.memory.usedPercent,
            tempC: live.flatMap(Readings.hottest),
            gpuPercent: live.flatMap(Readings.primaryGpu)?.utilization,
            alerts: alerts, lastSeenMs: lastSeenMs, sample: sample)
    }

    /// The alerts worth a number on the wrist.
    public static func seriousCount(_ alerts: [Alert]) -> Int {
        alerts.filter { $0.severity != .info }.count
    }

    /// NaN or infinity (a ratio over a zero total) is unmeasured. JSONEncoder
    /// would also refuse to encode it, dropping the whole snapshot.
    static func finite(_ v: Double?) -> Double? {
        guard let v, v.isFinite else { return nil }
        return v
    }
}

public struct WatchSnapshot: Codable, Sendable, Equatable {
    public static let currentVersion = 1
    /// Key of the encoded snapshot in a WatchConnectivity message or context.
    public static let payloadKey = "snapshot"
    /// Key of the watch's "send me the latest" message.
    public static let requestKey = "request"
    /// Older than this, the watch marks the picture as out of date.
    public static let staleAfterMs: Int64 = 120_000

    public let version: Int
    public let sentMs: Int64
    public let pcs: [WatchPc]

    public init(sentMs: Int64, pcs: [WatchPc]) {
        version = Self.currentVersion
        self.sentMs = sentMs
        self.pcs = pcs
    }

    public func encoded() throws -> Data {
        let e = JSONEncoder()
        e.outputFormatting = .sortedKeys
        return try e.encode(self)
    }

    public static func decoded(from data: Data) throws -> WatchSnapshot {
        try JSONDecoder().decode(WatchSnapshot.self, from: data)
    }

    public func isStale(nowMs: Int64) -> Bool { nowMs - sentMs > Self.staleAfterMs }
}

/// How often the phone may talk to the watch. A message every 5 s while the
/// watch app is on screen (reachable) keeps the numbers moving; otherwise the
/// application context — which the system delivers when it sees fit and
/// keeps only the newest of — at most once a minute, so a phone left
/// streaming does not drain the watch's battery.
public struct WatchSendPolicy: Sendable, Equatable {
    public let reachableInterval: TimeInterval
    public let contextInterval: TimeInterval

    public init(reachableInterval: TimeInterval = 5, contextInterval: TimeInterval = 60) {
        self.reachableInterval = reachableInterval
        self.contextInterval = contextInterval
    }

    public struct Decision: Sendable, Equatable {
        public let message: Bool
        public let context: Bool
        public static let nothing = Decision(message: false, context: false)
    }

    public func decide(now: Date, reachable: Bool, lastMessage: Date?, lastContext: Date?) -> Decision {
        let message = reachable && Self.due(now, lastMessage, reachableInterval)
        // The context is refreshed even while reachable, so the next launch of
        // the watch app opens on recent numbers rather than a minute-old set.
        let context = Self.due(now, lastContext, contextInterval)
        return Decision(message: message, context: context)
    }

    static func due(_ now: Date, _ last: Date?, _ interval: TimeInterval) -> Bool {
        guard let last else { return true }
        return now.timeIntervalSince(last) >= interval
    }
}
