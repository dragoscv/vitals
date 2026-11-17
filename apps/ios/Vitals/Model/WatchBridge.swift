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
#if canImport(WidgetKit)
import WidgetKit
#endif

/// Redraws the Home/Lock Screen widget after a fresh snapshot.
enum WidgetRefresh {
    static func reload() {
        #if canImport(WidgetKit)
        WidgetCenter.shared.reloadAllTimelines()
        #endif
    }
}

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
    /// PCs whose stream this bridge opened just to answer a watch request; the
    /// ids are detached again after `borrowSeconds` so no battery is held.
    private var borrowed: Set<String> = []
    /// Borrowed streams that have not yet delivered a frame. When one does,
    /// the 5-second message throttle is bypassed once so the watch does not sit
    /// on "Connecting…" for a reading that has already arrived.
    private var waitingForFrame: Set<String> = []
    private var borrowTask: Task<Void, Never>?
    /// How long a stream borrowed for the watch is kept open. Long enough for
    /// a keyframe (and one delta) to arrive on a LAN, short enough to matter
    /// for the battery.
    private static let borrowSeconds = 8
    /// WidgetKit budgets timeline reloads, and frames arrive once a second, so
    /// the widget is asked to redraw at most this often.
    private static let widgetReloadInterval: TimeInterval = 60
    /// How often the app-group picture may be rewritten while nothing
    /// meaningful changed. Frames arrive once a second; a `UserDefaults` write
    /// plus a JSON encode per second is pure cost for a widget whose freshness
    /// is measured in minutes. A change that matters — a PC appearing, going
    /// offline, or raising an alert — is published at once regardless.
    private static let publishInterval: TimeInterval = 10
    private var lastPublishedSignature: String?
    private var lastPublishedAt: Date?
    private var lastWidgetReload: Date?

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
        hub.observe { [weak self] in self?.changed() }
        sample.onChange = { [weak self] in self?.changed() }
        publishShared()
    }

    private func handle(_ event: WatchSessionRelay.Event) {
        switch event {
        case .changed: flush() // reachability may have opened the message channel
        case .request:
            // The watch app just opened: answer now, outside the throttle, and
            // — when no screen is watching a PC — open its stream briefly so
            // the watch gets a live picture instead of "Connecting…" for ever.
            lastMessage = nil
            borrowStreams()
            changed()
        case .snapshot: break // the phone only sends
        }
    }

    /// Opens the stream of every PC nothing else is watching, for a few
    /// seconds. On a screen that is already open this is a no-op (`existing`
    /// returns the live stream), so it only does work when the UI is closed —
    /// which is exactly the case the watch used to see as stuck.
    private func borrowStreams() {
        var opened = false
        for p in store.pairings where hub.existing(p.id) == nil {
            hub.stream(for: p.id).attach(p)
            borrowed.insert(p.id)
            waitingForFrame.insert(p.id)
            opened = true
        }
        guard opened, borrowTask == nil else { return }
        borrowTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.borrowSeconds))
            guard !Task.isCancelled, let self else { return }
            self.borrowTask = nil
            self.waitingForFrame = []
            let ids = self.borrowed
            self.borrowed = []
            for id in ids { self.hub.existing(id)?.detach() }
        }
    }

    private var session: WCSession? {
        let s = WCSession.default
        guard s.activationState == .activated, s.isPaired, s.isWatchAppInstalled else { return nil }
        return s
    }

    private func changed() {
        // A borrowed stream just came alive: send its first reading at once.
        if !waitingForFrame.isEmpty {
            let cameAlive = Set(waitingForFrame.filter { hub.existing($0)?.state == .live })
            if !cameAlive.isEmpty {
                waitingForFrame.subtract(cameAlive)
                lastMessage = nil
            }
        }
        publishShared()
        messageDirty = true
        contextDirty = true
        flush()
    }

    /// The Home/Lock Screen widget and the watch complication read this picture
    /// from the app group. It is published on every news item — *not* only when
    /// a watch is paired: `flush()` returns early without an activated, paired
    /// session, so publishing from `send()` would leave the widget dead on a
    /// phone that has no Apple Watch.
    ///
    /// Throttled, but not blind: a change to the set of PCs, to whether one is
    /// live, or to its alert count is written immediately, because that is what
    /// a glance at the widget is for. The per-second churn of the readings in
    /// between waits for `publishInterval`.
    private func publishShared(now: Date = Date()) {
        let snap = SharedSnapshot(watch: snapshot(now: now))
        let signature = Self.signature(snap)
        let meaningful = signature != lastPublishedSignature
        let due = lastPublishedAt.map { now.timeIntervalSince($0) >= Self.publishInterval } ?? true
        guard meaningful || due else { return }
        lastPublishedSignature = signature
        lastPublishedAt = now
        SharedSnapshotStore.save(snap)
        if let last = lastWidgetReload, now.timeIntervalSince(last) < Self.widgetReloadInterval { return }
        lastWidgetReload = now
        WidgetRefresh.reload()
    }

    /// What a widget actually shows: which PCs there are and how they are
    /// doing, not the digits. A string rather than a tuple array because
    /// comparing tuples needs a conformance this code should not depend on.
    private static func signature(_ snap: SharedSnapshot) -> String {
        snap.pcs.map { "\($0.id)|\($0.label)|\($0.live)|\($0.alerts)|\($0.sample)" }
            .joined(separator: ",")
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
        let snap = snapshot(now: now)
        guard let data = try? snap.encoded() else { return }
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
