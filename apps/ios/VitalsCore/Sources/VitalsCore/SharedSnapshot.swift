// The one picture a WidgetKit widget (Home/Lock Screen) and a watch
// complication / Smart-Stack widget read. It is written where the readings
// are: the iPhone app writes its own container's app group before it sends a
// `WatchSnapshot`; the watch app writes the picture it just received into the
// watch's app group. Both sides then read from `SharedSnapshotStore`.
//
// The same rule as `WatchSnapshot`: a pairing's id and label only, never a
// token, address or MAC, and an unmeasured reading is nil (the widget shows an
// em dash), never a borrowed zero.

import Foundation

public struct SharedPc: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let label: String
    /// Whether the readings below are live; a not-live PC carries no numbers.
    public let live: Bool
    public let cpuPercent: Double?
    public let memoryPercent: Double?
    public let tempC: Double?
    public let alerts: Int
    public let sample: Bool

    public init(
        id: String, label: String, live: Bool, cpuPercent: Double?, memoryPercent: Double?,
        tempC: Double?, alerts: Int, sample: Bool
    ) {
        self.id = id
        self.label = label
        self.live = live
        self.cpuPercent = Self.finite(cpuPercent)
        self.memoryPercent = Self.finite(memoryPercent)
        self.tempC = Self.finite(tempC)
        self.alerts = max(0, alerts)
        self.sample = sample
    }

    /// NaN or infinity (a ratio over a zero total) is unmeasured: the encoder
    /// would refuse it, dropping the whole snapshot.
    static func finite(_ v: Double?) -> Double? {
        guard let v, v.isFinite else { return nil }
        return v
    }
}

public struct SharedSnapshot: Codable, Sendable, Equatable {
    public static let currentVersion = 1
    public let version: Int
    public let sentMs: Int64
    public let pcs: [SharedPc]

    public init(sentMs: Int64, pcs: [SharedPc]) {
        version = Self.currentVersion
        self.sentMs = sentMs
        self.pcs = pcs
    }

    /// The PC a widget shows: the first live one, else the first.
    public var headline: SharedPc? {
        pcs.first { $0.live } ?? pcs.first
    }
}

public extension SharedSnapshot {
    /// From the picture the phone sends the watch — the same fields, minus the
    /// watch-only `WatchPcState` cases the widget does not draw.
    init(watch: WatchSnapshot) {
        self.init(
            sentMs: watch.sentMs,
            pcs: watch.pcs.map { pc in
                let live = pc.state == .live
                return SharedPc(
                    id: pc.id, label: pc.label, live: live,
                    cpuPercent: live ? pc.cpuPercent : nil,
                    memoryPercent: live ? pc.memoryPercent : nil,
                    tempC: live ? pc.tempC : nil,
                    alerts: pc.alerts, sample: pc.sample)
            })
    }
}

/// Reads and writes the snapshot in the app-group container the widget
/// extension shares with its app. When the group is not configured (a build
/// without the entitlement) `UserDefaults` still returns a usable store, so
/// nothing crashes — the widget simply shows nothing.
public enum SharedSnapshotStore {
    public static let appGroup = "group.app.vitals.ios"
    public static let key = "shared-snapshot"

    private static var defaults: UserDefaults? { UserDefaults(suiteName: appGroup) }

    public static func load() -> SharedSnapshot? {
        guard let data = defaults?.data(forKey: key) else { return nil }
        return try? JSONDecoder().decode(SharedSnapshot.self, from: data)
    }

    public static func save(_ snapshot: SharedSnapshot) {
        guard let data = try? JSONEncoder().encode(snapshot) else { return }
        defaults?.set(data, forKey: key)
    }
}
