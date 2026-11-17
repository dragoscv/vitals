// The human words for a PC alert, shared by the Now tab and the local
// notifications so the two can never drift.

import Foundation
import VitalsCore

enum AlertWords {
    /// The catalog key for an alert kind.
    static func key(_ kind: AlertKind) -> String {
        switch kind {
        case .cpuSustained: "alert_cpu_sustained"
        case .cpuThrottled: "alert_cpu_throttled"
        case .memoryPressure: "alert_memory_pressure"
        case .memoryCommit: "alert_memory_commit"
        case .diskSaturated: "alert_disk_saturated"
        case .diskLatency: "alert_disk_latency"
        case .diskSpace: "alert_disk_space"
        case .diskHealth: "alert_disk_health"
        case .gpuThrottled: "alert_gpu_throttled"
        case .thermalCpu: "alert_thermal_cpu"
        case .networkErrors: "alert_network_errors"
        case .batteryLow: "alert_battery_low"
        case .batteryHealth: "alert_battery_health"
        case .custom: "alert_custom"
        case .serviceStopped: "alert_service_stopped"
        case .unknown: "alert_other"
        }
    }

    /// "The processor is being slowed down · chrome.exe".
    static func title(_ a: VitalsCore.Alert) -> String {
        let title = L(key(a.kind))
        return a.subject.isEmpty ? title : "\(title) · \(a.subject)"
    }
}
