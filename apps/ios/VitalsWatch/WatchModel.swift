// The latest snapshot from the iPhone, in memory. On launch it starts from
// the application context the system kept (the phone refreshes it at most
// once a minute), then asks the phone for a fresh one if it is reachable.

import Foundation
import Observation
import VitalsCore
import WatchConnectivity

@MainActor @Observable
final class WatchModel {
    private(set) var snapshot: WatchSnapshot?
    private(set) var activated = false
    @ObservationIgnored private var relay: WatchSessionRelay?

    func start() {
        if WatchScreenshotRoute.opensSample {
            snapshot = WatchScreenshotRoute.sampleSnapshot()
            return
        }
        guard relay == nil, WCSession.isSupported() else { return }
        let relay = WatchSessionRelay { [weak self] event in self?.handle(event) }
        self.relay = relay
        WCSession.default.delegate = relay
        WCSession.default.activate()
    }

    func pc(_ id: String) -> WatchPc? { snapshot?.pcs.first { $0.id == id } }

    /// Asks the phone for the newest picture now (it answers outside its
    /// throttle). Silently nothing when the phone is not reachable.
    func requestLatest() {
        let s = WCSession.default
        guard s.activationState == .activated, s.isReachable else { return }
        s.sendMessage([WatchSnapshot.requestKey: true], replyHandler: nil, errorHandler: nil)
    }

    private func handle(_ event: WatchSessionRelay.Event) {
        switch event {
        case .changed:
            let s = WCSession.default
            activated = s.activationState == .activated
            if activated, snapshot == nil, let data = s.receivedApplicationContext[WatchSnapshot.payloadKey] as? Data {
                accept(data)
            }
            requestLatest()
        case let .snapshot(data):
            accept(data)
        case .request:
            break // only the phone answers requests
        }
    }

    /// Keeps the newest by the phone's clock: a context delivered late must not
    /// replace a fresher message.
    private func accept(_ data: Data) {
        guard let next = try? WatchSnapshot.decoded(from: data) else { return }
        if let current = snapshot, current.sentMs > next.sentMs { return }
        snapshot = next
    }
}

/// Screenshot route, passed only as a launch argument (`-vitalsRoute sample`)
/// by `simctl launch` in apps/ios/scripts/screenshots.sh: the labelled Sample
/// PC, the same one the iPhone shows, so the App Store images are real
/// captures without a paired phone. Nothing in the app writes this key.
enum WatchScreenshotRoute {
    static var opensSample: Bool { UserDefaults.standard.string(forKey: "vitalsRoute") == "sample" }

    static func sampleSnapshot() -> WatchSnapshot {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        let pc = WatchPc(
            id: "sample", label: L("sample_title"), state: .live, cpuPercent: 27, memoryPercent: 58,
            tempC: 64, gpuPercent: 16, alerts: 0, lastSeenMs: now, sample: true)
        return WatchSnapshot(sentMs: now, pcs: [pc])
    }
}
