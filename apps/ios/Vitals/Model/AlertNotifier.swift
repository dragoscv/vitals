// Local notifications for the alerts a PC raises, while this iPhone is on the
// same network.
//
// Best effort by design: iOS suspends an app in the background, so this can
// only fire while Vitals is (or was just) on screen — a PC-side push would
// need a server the LAN API does not have. It polls `/api/v1/alerts` for each
// paired PC rather than depending on a live stream, so it works whatever tab
// is open. An alert is told once: the id is remembered per PC, so an ongoing
// warning does not repeat every minute.

import Foundation
@preconcurrency import UserNotifications
import VitalsCore

@MainActor
final class AlertNotifier: NSObject, UNUserNotificationCenterDelegate {
    private let store: PairingStore
    private let center = UNUserNotificationCenter.current()
    /// Alert ids already announced, per PC.
    private var announced: [String: Set<String>] = [:]
    private var asked = false
    private var poller: Task<Void, Never>?

    /// At most this many notifications per poll, so one bad PC cannot bury
    /// the phone under banners.
    private static let maxPerPoll = 3
    private static let pollInterval: TimeInterval = 60

    init(store: PairingStore) {
        self.store = store
        super.init()
    }

    func start() {
        guard poller == nil else { return }
        center.delegate = self
        Task { await askOnce() }
        poller = Task { [weak self] in
            while !Task.isCancelled {
                await self?.poll()
                try? await Task.sleep(for: .seconds(Self.pollInterval))
            }
        }
    }

    func stop() {
        poller?.cancel()
        poller = nil
    }

    private func askOnce() async {
        guard !asked else { return }
        asked = true
        _ = try? await center.requestAuthorization(options: [.alert, .sound])
    }

    /// One request per paired PC; nothing leaves the LAN (the client refuses an
    /// authenticated call off it), and a failure is simply ignored.
    private func poll() async {
        guard LanGate.shared.isOnLan else { return }
        var posted = 0
        for p in store.pairings where posted < Self.maxPerPoll {
            let client = VitalsClient(baseUrl: p.baseUrl, token: p.token)
            guard case let .success(alerts) = await client.alerts() else { continue }
            posted += deliver(pairing: p, alerts: alerts, budget: Self.maxPerPoll - posted)
        }
    }

    private func deliver(pairing: Pairing, alerts: [VitalsCore.Alert], budget: Int) -> Int {
        var seen = announced[pairing.id] ?? []
        var posted = 0
        for a in alerts where a.severity != .info {
            guard posted < budget, !seen.contains(a.id) else { continue }
            seen.insert(a.id)
            post(pairing: pairing, alert: a)
            posted += 1
        }
        announced[pairing.id] = seen
        return posted
    }

    private func post(pairing: Pairing, alert: VitalsCore.Alert) {
        let content = UNMutableNotificationContent()
        content.title = pairing.label
        content.body = AlertWords.title(alert)
        content.sound = .default
        let id = "alert|\(pairing.id)|\(alert.id)"
        // The completion-handler overload: this is a synchronous hop-out, and
        // the async one would need an `await` inside a non-async function.
        center.add(UNNotificationRequest(identifier: id, content: content, trigger: nil), withCompletionHandler: nil)
    }

    /// Show the banner even when Vitals is in front: the alert is news whatever
    /// tab happens to be open.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
}
