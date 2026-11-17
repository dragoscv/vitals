// Small pieces of wording the screens share: duration units read from the
// string catalog (so "1d 1h" becomes "1z 1o" in Romanian) and the reason under
// a throttled processor.

import Foundation
import VitalsCore

enum DurationWords {
    /// The d/h/m suffixes, translated.
    static var units: Format.DurationUnits {
        Format.DurationUnits(day: L("unit_day"), hour: L("unit_hour"), minute: L("unit_minute"))
    }

    /// "3h 20m", with the units the reader's language uses.
    static func text(_ secs: Int64?) -> String { Format.duration(secs, units: units) }
}

enum CpuWords {
    /// The value beside "Slowed down": the reason when the PC named one, an em
    /// dash for a reason this build cannot name (never a guess), and the plain
    /// "not throttled" when the PC reported no throttle at all.
    static func throttle(_ reason: ThrottleReason?) -> String {
        switch reason {
        case .thermal: L("throttle_thermal")
        case .powerLimit: L("throttle_power")
        case .currentLimit: L("throttle_current")
        case .voltageDrop: L("throttle_voltage")
        case .powerPolicy: L("throttle_policy")
        case .unknown: Format.dash
        case nil: L("cpu_not_throttled")
        }
    }
}
