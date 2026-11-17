// What a PC can actually do, mirrored from `crates/vitals-core/src/capability.rs`.
//
// The UI asks before it renders: a control the PC, its OS or its privilege
// level cannot honour is hidden rather than offered and then refused. The
// route is `GET /api/v1/capabilities`; a PC that does not serve it (or an
// older build) answers 404, and the client then treats every capability as
// unknown rather than guessing — see `VitalsClient.capabilities()`.
//
// Tolerant everywhere: an unknown enum value or a malformed entry from a
// newer desktop is dropped, never allowed to fail the whole answer.

import Foundation

/// The platform a PC build runs on.
public enum Platform: String, TolerantEnum {
    case windows, macos, linux, unknown
    public static let unknownCase = Self.unknown
}

/// A single discrete thing a PC backend may or may not be able to do.
public enum Capability: String, TolerantEnum {
    // Observation
    case perProcessDiskIo, perProcessNetworkIo, perProcessGpu
    case shortLivedProcesses, windowFlashCapture, handleEnumeration, threadStacks
    case thermals, powerDraw, errorReports
    // Mutation
    case terminateProcess, suspendProcess, setProcessPriority, setProcessAffinity
    case setEfficiencyMode, trimWorkingSet
    case managePowerPlans, manageServices, manageStartupItems, manageScheduledTasks
    case uninstallApplications, firewallControl, closeConnection
    case gpuOverclock, fanControl, diskCleanup
    case unknown
    public static let unknownCase = Self.unknown
}

/// Why a capability is unavailable, so the UI can say something useful.
public enum Unavailable: String, TolerantEnum {
    case notSupportedOnPlatform, noSuchHardware, needsElevation, needsHelper
    case needsPlugin, disabledByUser, appSandbox, notImplemented
    case unknown
    public static let unknownCase = Self.unknown
}

/// The full capability report for a PC at a point in time. `unavailable` is a
/// list of `(capability, reason)` pairs on the wire (`Vec<(Capability, Unavailable)>`
/// in Rust, so `[[String]]` in JSON).
public struct Capabilities: Sendable, Equatable {
    public let platform: Platform
    public let available: [Capability]
    public let unavailable: [Capability: Unavailable]

    public init(platform: Platform, available: [Capability], unavailable: [Capability: Unavailable]) {
        self.platform = platform
        self.available = available
        self.unavailable = unavailable
    }

    /// Whether the PC can do this. An unknown capability is never available.
    public func has(_ cap: Capability) -> Bool {
        cap != .unknown && available.contains(cap)
    }

    /// Why `cap` is unavailable, or nil if it is available or unreported.
    public func reason(_ cap: Capability) -> Unavailable? { unavailable[cap] }
}

extension Capabilities: Decodable {
    private enum Keys: String, CodingKey { case platform, available, unavailable }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        platform = (try? c.decode(Platform.self, forKey: .platform)) ?? .unknown
        available = (try? c.decode([Capability].self, forKey: .available)) ?? []
        var map: [Capability: Unavailable] = [:]
        for pair in (try? c.decode([[String]].self, forKey: .unavailable)) ?? [] where pair.count == 2 {
            map[Capability(rawValue: pair[0]) ?? .unknown] = Unavailable(rawValue: pair[1]) ?? .unknown
        }
        unavailable = map
    }
}
