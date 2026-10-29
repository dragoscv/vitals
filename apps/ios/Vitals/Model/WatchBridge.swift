// Feeds the Apple Watch app the PCs this phone already watches. The watch
// has no pairing of its own: it shows what the phone sends, built by
// WatchPc(pairing:...) from id and label only — never a token or address.
//
// Throttled by WatchSendPolicy (VitalsCore, tested): a message at most every
// 5 s while the watch app is reachable, the application context at most every
// 60 s. A change that arrives inside the window is not lost: a single
// trailing send is scheduled for when the window opens.

import Foundation
import VitalsCore
import WatchConnectivity

@MainActor
final class WatchBridge {
    private let store: PairingStore
    private let hub: LiveHub
    private let sample: SampleMode
    private let policy = WatchSendPolicy()
    private var relay: WatchSessionRelay?
    private var lastMessage: Date?
    private var lastContext: Date?
    /// News the watch has not had yet, per channel. Without these a trailing
    /// send would re-send the same picture every 5 / 60 s for ever.
    private var messageDirty = false
    private var contextDirty = false
    private var pending: Task<Void, Never>?

    init(store: PairingStore, hub: LiveHub, sample: SampleMode) {
        self.store = store
        self.hub = hub
        self.sample = sample
    }

    func start() {
        guard relay == nil, WCSession.isSupported() else { return }
        let relay = WatchSessionRelay { [weak self] event in self?.handle(event) }
        self.relay = relay
        // The pairings themselves are news: once activation completes (.changed)
        // the watch learns the list even if no stream is open yet.
        messageDirty = true
        contextDirty = true
        WCSession.default.delegate = relay
        WCSession.default.activate()
        hub.onChange = { [weak self] in self?.changed() }
        sample.onChange = { [weak self] in self?.changed() }
    }

    private func handle(_ event: WatchSessionRelay.Event) {
        switch event {
        case .changed: flush() // reachability may have opened the message channel
        case .request:
            // The watch app just opened: answer now, outside the throttle.
            lastMessage = nil
            changed()
        case .snapshot: break // the phone only sends
        }
    }

    private var session: WCSession? {
        let s = WCSession.default
        guard s.activationState == .activated, s.isPaired, s.isWatchAppInstalled else { return nil }
        return s
    }

    private func changed() {
        messageDirty = true
        contextDirty = true
        flush()
    }

    /// Sends what the policy allows now and schedules one trailing send for
    /// whatever news it held back.
    private func flush() {
        guard let session, messageDirty || contextDirty else { return }
        let now = Date()
        let reachable = session.isReachable
        let d = policy.decide(now: now, reachable: reachable, lastMessage: lastMessage, lastContext: lastContext)
        let message = d.message && messageDirty
        let context = d.context && contextDirty
        if message || context { send(session, message: message, context: context, now: now) }
        scheduleTrailing(reachable: reachable, now: now)
    }

    private func scheduleTrailing(reachable: Bool, now: Date) {
        guard pending == nil else { return }
        let messageAt = reachable && messageDirty
            ? lastMessage.map { $0.addingTimeInterval(policy.reachableInterval) } : nil
        let contextAt = contextDirty ? lastContext.map { $0.addingTimeInterval(policy.contextInterval) } : nil
        guard let at = [messageAt, contextAt].compactMap({ $0 }).min() else { return }
        let delay = max(0.1, at.timeIntervalSince(now))
        pending = Task { [weak self] in
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled, let self else { return }
            self.pending = nil
            self.flush()
        }
    }

    private func send(_ session: WCSession, message: Bool, context: Bool, now: Date) {
        guard let data = try? snapshot(now: now).encoded() else { return }
        let payload: [String: Any] = [WatchSnapshot.payloadKey: data]
        if message {
            lastMessage = now
            messageDirty = false
            session.sendMessage(payload, replyHandler: nil, errorHandler: nil)
        }
        if context {
            lastContext = now
            contextDirty = false
            try? session.updateApplicationContext(payload)
        }
    }

    private func snapshot(now: Date) -> WatchSnapshot {
        var pcs = store.pairings.map { p -> WatchPc in
            guard let s = hub.existing(p.id) else {
                // Never opened on this launch: no reading, honestly so.
                return WatchPc(pairing: p, state: .connecting, system: nil, alerts: 0, lastSeenMs: nil)
            }
            return WatchPc(
                pairing: p, state: Self.state(s.state), system: s.view?.system,
                alerts: WatchPc.seriousCount(s.alerts), lastSeenMs: s.lastSeenMs)
        }
        if sample.enabled, let system = sample.view?.system {
            pcs.append(
                WatchPc(
                    pairing: sample.pairing, state: .live, system: system, alerts: 0,
                    lastSeenMs: Int64(now.timeIntervalSince1970 * 1000), sample: true))
        }
        return WatchSnapshot(sentMs: Int64(now.timeIntervalSince1970 * 1000), pcs: pcs)
    }

    static func state(_ s: PcState) -> WatchPcState {
        switch s {
        case .live: .live
        case .connecting: .connecting
        case .unreachable: .offline
        case .unauthorised: .unauthorised
        case .incompatible: .incompatible
        case .offLan: .offLan
        }
    }
}
