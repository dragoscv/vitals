// The wire model, mirrored from crates/vitals-core (like the Android
// core/model/Wire.kt). Every optional here is `Option<T>` in Rust: nil means
// "not measured", never zero, and the UI renders an em dash for it. Never
// default an optional to 0 to make a screen simpler.
//
// Enums decode tolerantly: an unknown value from a newer desktop becomes
// `.unknown` instead of failing the whole frame.

import Foundation

public protocol TolerantEnum: RawRepresentable, Decodable, Sendable, Hashable where RawValue == String {
    static var unknownCase: Self { get }
}

extension TolerantEnum {
    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = Self(rawValue: raw) ?? Self.unknownCase
    }
}

public struct Frame: Decodable, Sendable {
    public let seq: Int64
    public let timestampMs: Int64
    public let elapsedMs: Int
    public let payload: FramePayload
}

public enum FramePayload: Decodable, Sendable {
    case keyframe(system: SystemMetrics, processes: [Process])
    case delta(system: SystemMetrics, changed: [Process], exited: [Int])

    private enum Keys: String, CodingKey { case kind, system, processes, changed, exited }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        let system = try c.decode(SystemMetrics.self, forKey: .system)
        switch try c.decode(String.self, forKey: .kind) {
        case "keyframe":
            self = .keyframe(system: system, processes: try c.decode([Process].self, forKey: .processes))
        case "delta":
            self = .delta(
                system: system,
                changed: try c.decode([Process].self, forKey: .changed),
                exited: try c.decode([Int].self, forKey: .exited))
        case let other:
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: c, debugDescription: "unknown payload kind \(other)")
        }
    }

    public var system: SystemMetrics {
        switch self {
        case let .keyframe(system, _), let .delta(system, _, _): system
        }
    }
}

public struct SystemMetrics: Decodable, Sendable {
    public let cpu: CpuMetrics
    public let memory: MemoryMetrics
    public let disks: [DiskMetrics]
    public let networks: [NetworkMetrics]
    public let gpus: [GpuMetrics]
    public let powerDraw: Double?
    public let battery: BatteryMetrics?
    public let fans: [FanMetrics]

    private enum Keys: String, CodingKey { case cpu, memory, disks, networks, gpus, powerDraw, battery, fans }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        cpu = try c.decode(CpuMetrics.self, forKey: .cpu)
        memory = try c.decode(MemoryMetrics.self, forKey: .memory)
        disks = try c.decode([DiskMetrics].self, forKey: .disks)
        networks = try c.decode([NetworkMetrics].self, forKey: .networks)
        gpus = try c.decode([GpuMetrics].self, forKey: .gpus)
        powerDraw = try c.decodeIfPresent(Double.self, forKey: .powerDraw)
        battery = try c.decodeIfPresent(BatteryMetrics.self, forKey: .battery)
        // `fans` arrived later; frames from older desktops do not carry it.
        fans = try c.decodeIfPresent([FanMetrics].self, forKey: .fans) ?? []
    }
}

public struct FanMetrics: Decodable, Sendable, Hashable {
    public let name: String
    public let rpm: Int
}

public enum ThrottleReason: String, TolerantEnum {
    case thermal, powerLimit, currentLimit, voltageDrop, powerPolicy, unknown
    public static let unknownCase = Self.unknown
}

public struct CpuMetrics: Decodable, Sendable {
    public let total: Double
    public let perCore: [Double]
    public let kernel: Double
    public let effectiveClock: Int64?
    public let maxClock: Int64?
    public let temperature: Double?
    public let power: Double?
    public let throttled: ThrottleReason?
    public let processCount: Int
    public let threadCount: Int
    public let handleCount: Int?
    public let uptimeSecs: Int64
    public let contextSwitches: Int64?
    public let interrupts: Int64?
}

public struct MemoryMetrics: Decodable, Sendable {
    public let total: Int64
    public let used: Int64
    public let available: Int64
    public let cached: Int64
    /// Windows quantities: nil from a Mac (no commit charge, no pool split).
    public let pagedPool: Int64?
    public let nonPagedPool: Int64?
    public let committed: Int64?
    public let commitLimit: Int64?
    public let swapTotal: Int64
    public let swapUsed: Int64?
    public let hardwareReserved: Int64?
    public let pageFaultsPerSec: Int64?
    public let speed: Int64?
    public let slotsUsed: Int?
    public let slotsTotal: Int?
    public let formFactor: String?

    /// nil when total is 0: an unknown share, not 0 %.
    public var usedPercent: Double? {
        total > 0 ? Double(used) / Double(total) * 100 : nil
    }
}

public enum DiskKind: String, TolerantEnum {
    case hdd, ssd, nvme, removable, network, optical, unknown
    public static let unknownCase = Self.unknown
}

public struct DiskHealth: Decodable, Sendable {
    public let lifeRemaining: Double?
    public let powerOnHours: Int64?
    public let totalWritten: Int64?
    public let reallocatedSectors: Int64?
    public let failing: Bool
}

public struct DiskMetrics: Decodable, Sendable, Identifiable {
    public let id: Int
    public let name: String
    public let model: String?
    public let mount: String?
    public let kind: DiskKind
    public let total: Int64
    public let free: Int64
    public let read: Int64
    public let write: Int64
    public let activeTime: Double
    public let responseMs: Double?
    public let queueDepth: Double?
    public let temperature: Double?
    public let health: DiskHealth?
}

public enum NetworkKind: String, TolerantEnum {
    case ethernet, wiFi, cellular, bluetooth, loopback, virtual, vpn, unknown
    public static let unknownCase = Self.unknown
}

public struct NetworkMetrics: Decodable, Sendable, Identifiable {
    public let id: Int
    public let name: String
    public let adapter: String?
    public let kind: NetworkKind
    public let rx: Int64
    public let tx: Int64
    public let rxTotal: Int64
    public let txTotal: Int64
    public let linkSpeed: Int64?
    public let ipv4: String?
    public let ipv6: String?
    public let mac: String?
    public let connected: Bool
    public let signal: Double?
    public let ssid: String?
    public let errorsPerSec: Int64?
}

public enum GpuVendor: String, TolerantEnum {
    case nvidia, amd, intel, apple, qualcomm, unknown
    public static let unknownCase = Self.unknown
}

public struct GpuEngine: Decodable, Sendable, Hashable {
    public let name: String
    public let utilization: Double
}

public struct GpuMetrics: Decodable, Sendable, Identifiable {
    public let id: Int
    public let name: String
    public let vendor: GpuVendor
    public let engines: [GpuEngine]
    public let utilization: Double?
    public let memoryUsed: Int64?
    public let memoryTotal: Int64?
    public let sharedMemoryUsed: Int64?
    public let coreClock: Int64?
    public let memoryClock: Int64?
    public let temperature: Double?
    public let hotspotTemperature: Double?
    public let power: Double?
    public let powerLimit: Double?
    public let fanPercent: Double?
    public let fanRpm: Int?
    public let throttled: ThrottleReason?
    public let driverVersion: String?
}

public struct BatteryMetrics: Decodable, Sendable {
    public let charge: Double
    public let charging: Bool
    public let timeRemainingSecs: Int64?
    public let power: Double?
    public let health: Double?
    public let cycleCount: Int?
    public let temperature: Double?
}

public struct ProcessKey: Codable, Sendable, Hashable {
    public let pid: Int
    /// Opaque: echoed back exactly. A FILETIME on Windows (~1.3e17), so Int64.
    public let startTime: Int64

    public init(pid: Int, startTime: Int64) {
        self.pid = pid
        self.startTime = startTime
    }
}

public enum ProcessKind: String, TolerantEnum {
    case app, background, service, system, containerized, unknown
    public static let unknownCase = Self.unknown
}

public enum ProcessState: String, TolerantEnum {
    case running, suspended, waiting, notResponding, zombie, unknown
    public static let unknownCase = Self.unknown
}

public enum IntegrityLevel: String, TolerantEnum {
    case untrusted, low, medium, high, system, protected, unknown
    public static let unknownCase = Self.unknown
}

public enum ProtectionLevel: String, TolerantEnum {
    case none, light, full, unknown
    public static let unknownCase = Self.unknown
}

public struct Process: Decodable, Sendable, Identifiable {
    public let key: ProcessKey
    public let parent: Int?
    public let name: String
    public let kind: ProcessKind
    public let state: ProcessState
    public let flags: Int
    public let integrity: IntegrityLevel?
    public let protection: ProtectionLevel
    public let cpu: Double
    public let memoryPrivate: Int64
    public let memoryWorkingSet: Int64
    public let diskRead: Int64
    public let diskWrite: Int64
    public let netRx: Int64?
    public let netTx: Int64?
    public let gpu: Double?
    public let gpuMemory: Int64?
    public let threadCount: Int
    public let handleCount: Int?
    public let user: String?
    public let uptimeSecs: Int64
    public let description: String?

    public var id: ProcessKey { key }

    /// The app's own name, else the file name (mirrors `displayName` in @vitals/protocol).
    public var displayName: String {
        let d = description?.trimmingCharacters(in: .whitespaces) ?? ""
        return d.isEmpty ? name : d
    }

    public static let flagEfficiencyMode = 1 << 3
    public static let flagCritical = 1 << 5

    /// Mirrors `Process::is_safely_terminable` in Rust.
    public var isSafelyTerminable: Bool {
        flags & Self.flagCritical == 0 && protection == .none && kind != .system
    }

    public var inEfficiencyMode: Bool { flags & Self.flagEfficiencyMode != 0 }
}

public struct Summary: Decodable, Sendable {
    public let seq: Int64
    public let timestampMs: Int64
    public let system: SystemMetrics
    public let top: [Process]
    public let processCount: Int
}

public struct MachineSample: Decodable, Sendable, Identifiable {
    public let ts: Int64
    public let cpuPercent: Double
    public let cpuKernelPercent: Double
    public let memoryUsed: Int64
    public let memoryTotal: Int64
    public let diskReadBps: Int64
    public let diskWriteBps: Int64
    public let netRxBps: Int64
    public let netTxBps: Int64
    public let gpuPercent: Double?
    public let cpuTempC: Double?
    public let powerDrawW: Double?
    public var id: Int64 { ts }
}

public struct SensorLine: Decodable, Sendable, Identifiable {
    public let key: String
    public let label: String
    public let value: Double
    public let unit: String
    public let source: String
    public let quality: String
    public var id: String { key }
}

public enum CoreClass: String, TolerantEnum {
    case performance, efficiency, lowPower, standard, unknown
    public static let unknownCase = Self.unknown
}

public struct HostInfo: Decodable, Sendable {
    public let hostname: String
    public let osName: String
    public let osVersion: String
    public let kernelVersion: String
    public let architecture: String
    public let cpuModel: String
    public let cpuVendor: String
    public let physicalCores: Int
    public let logicalCores: Int
    public let coreTopology: [CoreClass]?
    public let totalMemory: Int64
    public let bootTimeMs: Int64
    public let isVirtualMachine: Bool
    public let motherboard: String?
    public let biosVersion: String?
}

public struct Health: Decodable, Sendable {
    public let ok: Bool
    public let version: String
    public let modelVersion: Int
}

public enum AlertKind: String, TolerantEnum {
    case cpuSustained, cpuThrottled, memoryPressure, memoryCommit, diskSaturated, diskLatency
    case diskSpace, diskHealth, gpuThrottled, thermalCpu, networkErrors, batteryLow, batteryHealth
    case unknown
    public static let unknownCase = Self.unknown
}

public enum Severity: String, TolerantEnum {
    case info, warning, critical, unknown
    public static let unknownCase = Self.unknown
}

public enum AlertRoute: String, TolerantEnum {
    case performance, processes, storage, network, devices, unknown
    public static let unknownCase = Self.unknown
}

/// A number or a string, as Rust's untagged value.
public enum AlertValue: Decodable, Sendable, Hashable {
    case number(Double)
    case text(String)

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else {
            self = .text(try c.decode(String.self))
        }
    }
}

public struct Alert: Decodable, Sendable, Identifiable {
    public let kind: AlertKind
    public let severity: Severity
    public let subject: String
    public let title: String
    public let cause: String
    public let values: [String: AlertValue]
    public let route: AlertRoute?
    public let sinceSample: Int64
    public var id: String { "\(kind.rawValue)|\(subject)" }
}

/// `POST /api/v1/control` body, tagged by `action` in kebab-case.
public enum ControlRequest: Encodable, Sendable {
    case terminate(ProcessKey)
    case suspend(ProcessKey)
    case resume(ProcessKey)
    case setPriority(ProcessKey, Priority)
    case setEfficiencyMode(ProcessKey, Bool)

    public enum Priority: String, Sendable, CaseIterable {
        case idle, belowNormal = "below-normal", normal, aboveNormal = "above-normal", high, realtime
    }

    private enum Keys: String, CodingKey { case action, key, priority, enabled }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case let .terminate(k):
            try c.encode("terminate", forKey: .action); try c.encode(k, forKey: .key)
        case let .suspend(k):
            try c.encode("suspend", forKey: .action); try c.encode(k, forKey: .key)
        case let .resume(k):
            try c.encode("resume", forKey: .action); try c.encode(k, forKey: .key)
        case let .setPriority(k, p):
            try c.encode("set-priority", forKey: .action); try c.encode(k, forKey: .key)
            try c.encode(p.rawValue, forKey: .priority)
        case let .setEfficiencyMode(k, on):
            try c.encode("set-efficiency-mode", forKey: .action); try c.encode(k, forKey: .key)
            try c.encode(on, forKey: .enabled)
        }
    }
}

/// Why the PC refused a control request (`kind`-tagged in Rust).
public enum ControlFailure: Equatable, Sendable, Error {
    case forbidden, accessDenied, notFound
    case unsupported(String)
    case internalError(String)
    case unreachable
}

public extension JSONDecoder {
    /// Decoder for every LAN response.
    static var vitals: JSONDecoder { JSONDecoder() }
}
