// Number formatting, identical in shape to Android's Format.kt and the
// desktop's formatters: every function takes an optional and returns an em
// dash for nil. An unmeasured reading is "—" on every surface, never "0".
// Decimals follow the locale, so Romanian shows "12,5 GB".

import Foundation

public enum Format {
    public static let dash = "\u{2014}"

    static func one(_ v: Double, _ locale: Locale) -> String {
        v.formatted(.number.precision(.fractionLength(1)).locale(locale))
    }

    static func rounded(_ v: Double) -> Int? {
        v.isFinite ? Int(v.rounded()) : nil
    }

    /// NaN or infinity (a ratio over a zero total) is unmeasured, not a number.
    public static func percent(_ v: Double?) -> String {
        guard let v, let r = rounded(v) else { return dash }
        return "\(r) %"
    }

    public static func percentCompact(_ v: Double?) -> String {
        guard let v, let r = rounded(v) else { return dash }
        return "\(r)%"
    }

    public static func celsius(_ v: Double?) -> String {
        guard let v, let r = rounded(v) else { return dash }
        return "\(r) °C"
    }

    public static func watts(_ v: Double?, locale: Locale = .current) -> String {
        guard let v, v.isFinite else { return dash }
        return abs(v) < 10 ? "\(one(v, locale)) W" : "\(Int(v.rounded())) W"
    }

    public static func rpm(_ v: Int?) -> String {
        guard let v else { return dash }
        return "\(v) RPM"
    }

    public static func volts(_ v: Double?, locale: Locale = .current) -> String {
        guard let v, v.isFinite else { return dash }
        return "\(v.formatted(.number.precision(.fractionLength(2)).locale(locale))) V"
    }

    public static func ghz(_ hz: Int64?, locale: Locale = .current) -> String {
        guard let hz else { return dash }
        return "\(one(Double(hz) / 1e9, locale)) GHz"
    }

    /// Binary units, as the desktop uses: 1 GB = 1024³ bytes.
    public static func bytes(_ b: Int64?, locale: Locale = .current) -> String {
        guard let b else { return dash }
        let units = ["B", "KB", "MB", "GB", "TB", "PB"]
        var v = Double(b)
        var i = 0
        while abs(v) >= 1024, i < units.count - 1 {
            v /= 1024
            i += 1
        }
        return (i == 0 || abs(v) >= 100) ? "\(Int(v.rounded())) \(units[i])" : "\(one(v, locale)) \(units[i])"
    }

    public static func rate(_ bps: Int64?, locale: Locale = .current) -> String {
        guard let bps else { return dash }
        return "\(bytes(bps, locale: locale))/s"
    }

    public static func duration(_ secs: Int64?) -> String {
        guard let secs else { return dash }
        let d = secs / 86_400
        let h = (secs % 86_400) / 3_600
        let m = (secs % 3_600) / 60
        if d > 0 { return "\(d)d \(h)h" }
        if h > 0 { return "\(h)h \(m)m" }
        return "\(m)m"
    }

    /// A sensor line's value, by its unit key.
    public static func sensor(unit: String, value: Double, locale: Locale = .current) -> String {
        switch unit {
        case "temperature": celsius(value)
        case "power": watts(value, locale: locale)
        case "voltage": volts(value, locale: locale)
        case "fanSpeed": rpm(rounded(value))
        case "charge", "percent": percent(value)
        default: one(value, locale)
        }
    }
}

/// Warn / danger bands, the same thresholds as Android's Palette and the desktop.
public enum Level: Sendable, Equatable {
    case unknown, ok, warn, danger

    static func band(_ v: Double?, warn: Double, danger: Double) -> Level {
        guard let v, v.isFinite else { return .unknown }
        if v >= danger { return .danger }
        if v >= warn { return .warn }
        return .ok
    }

    public static func cpu(_ percent: Double?) -> Level { band(percent, warn: 85, danger: 95) }
    public static func memory(_ percent: Double?) -> Level { band(percent, warn: 85, danger: 95) }
    public static func cpuTemp(_ c: Double?) -> Level { band(c, warn: 85, danger: 95) }
    public static func gpuTemp(_ c: Double?) -> Level { band(c, warn: 80, danger: 90) }
    public static func diskTemp(_ c: Double?) -> Level { band(c, warn: 55, danger: 65) }
}

public enum Readings {
    /// The busiest GPU is the one worth a ring (Android Readings.kt).
    public static func primaryGpu(_ s: SystemMetrics) -> GpuMetrics? {
        s.gpus.max { ($0.utilization ?? -1) < ($1.utilization ?? -1) }
    }

    /// Hottest CPU/GPU reading for the "Temp" ring; nil when none is measured.
    public static func hottest(_ s: SystemMetrics) -> Double? {
        ([s.cpu.temperature] + s.gpus.map(\.temperature)).compactMap { $0 }.max()
    }
}
