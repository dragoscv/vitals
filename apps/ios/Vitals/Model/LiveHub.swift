// One live stream per paired PC, only while something on screen watches it.
//
// Mirrors Android's LiveHub/PcStream: WebSocket frames folded into a
// MachineView; backoff 0.5 s → 30 s with jitter, reset on any frame; 401/403
// stops retrying until the user acts; three failures in a row = unreachable;
// an unreadable frame = incompatible; a plain close asks /health to tell
// "PC off" from "wrong version". Alerts are polled every 15 s alongside.

import Foundation
import Observation
import VitalsCore

enum PcState: Equatable {
    case connecting
    case live
    case unreachable(lastSeenMs: Int64?)
    case unauthorised
    case incompatible
    case offLan
}

@MainActor @Observable
final class PcStream {
    let pairingId: String
    private(set) var state: PcState = .connecting {
        didSet { onChange?() }
    }
    private(set) var view: MachineView?
    private(set) var alerts: [VitalsCore.Alert] = [] {
        didSet { onChange?() }
    }
    /// What this PC says it can do, fetched on every connect and reconnect
    /// (macOS differs from Windows, and a helper can connect mid-session).
    /// nil means "not known yet" — the caller shows a control rather than
    /// hiding a feature it cannot confirm.
    private(set) var capabilities: Capabilities?
    /// Told of every new frame, state and alert list (the watch bridge).
    @ObservationIgnored var onChange: (@MainActor () -> Void)?
    private(set) var cpuHistory: [Double] = []
    /// nil entries are ticks where the PC reported no memory total — a gap in
    /// the line, never a plotted zero.
    private(set) var memHistory: [Double?] = []
    private(set) var lastSeenMs: Int64?

    private var task: Task<Void, Never>?
    private var alertTask: Task<Void, Never>?
    private var backoff = Backoff()
    private var reconnects = ReconnectCounter()
    private var watchers = 0
    private var stopWork: Task<Void, Never>?

    init(pairingId: String) { self.pairingId = pairingId }

    func attach(_ pairing: Pairing) {
        watchers += 1
        stopWork?.cancel()
        if task == nil { start(pairing) }
    }

    /// Closes 5 s after the last watcher leaves, so moving between two screens
    /// of the same PC does not reconnect.
    func detach() {
        watchers = max(0, watchers - 1)
        guard watchers == 0 else { return }
        stopWork = Task { [weak self] in
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            self?.stop()
        }
    }

    /// Pull to refresh: restart now, skipping the backoff.
    func retry(_ pairing: Pairing) {
        stop()
        backoff.reset()
        state = .connecting
        start(pairing)
    }

    private func stop() {
        task?.cancel()
        alertTask?.cancel()
        task = nil
        alertTask = nil
    }

    private func start(_ pairing: Pairing) {
        let client = VitalsClient(baseUrl: pairing.baseUrl, token: pairing.token)
        task = Task { [weak self] in
            await self?.loadCapabilities(client)
            await self?.run(client)
        }
        alertTask = Task { [weak self] in
            while !Task.isCancelled {
                if case let .success(a) = await client.alerts() { self?.alerts = a }
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    /// Best effort: a PC that does not serve the route (an older build) leaves
    /// `capabilities` nil, and the UI falls back to showing the controls.
    private func loadCapabilities(_ client: VitalsClient) async {
        switch await client.capabilities() {
        case let .success(c): capabilities = c
        case .failure: capabilities = nil
        }
    }

    private func run(_ client: VitalsClient) async {
        while !Task.isCancelled {
            guard LanGate.shared.isOnLan else {
                state = .offLan
                try? await Task.sleep(for: .seconds(3))
                continue
            }
            do {
                for try await frame in client.frames() {
                    fold(frame)
                    reconnects.recovered()
                    backoff.reset()
                }
                // The server closed cleanly: ask /health why.
                if case let .failure(f) = await client.health(), case .incompatible = f {
                    state = .incompatible
                    return
                }
            } catch let end as VitalsClient.StreamEnd {
                switch end {
                case .unreadable:
                    state = .incompatible
                    return
                case .offLan:
                    state = .offLan
                    continue
                default:
                    break
                }
            } catch {}
            // A refused socket may be an expired token: one cheap read tells.
            if case .failure(.unauthorised) = await client.summary(top: 1) {
                state = .unauthorised
                return
            }
            if reconnects.failed() { state = .unreachable(lastSeenMs: lastSeenMs) }
            let delay = backoff.next()
            try? await Task.sleep(for: .milliseconds(Int(delay)))
            // A reconnect may land on a freshly updated PC (or a Mac): ask
            // again rather than trusting the first answer for the session.
            await loadCapabilities(client)
        }
    }

    private func fold(_ frame: Frame) {
        guard let next = MachineView.fold(view, frame) else { return }
        view = next
        state = .live
        lastSeenMs = Int64(Date().timeIntervalSince1970 * 1000)
        cpuHistory = Array((cpuHistory + [next.system.cpu.total]).suffix(60))
        memHistory = Array((memHistory + [next.system.memory.usedPercent]).suffix(60))
        // After lastSeenMs and the view are both new (the state's didSet ran
        // before them).
        onChange?()
    }
}

@MainActor @Observable
final class LiveHub {
    private var streams: [String: PcStream] = [:]
    /// Told whenever any stream has news: the watch bridge and the local
    /// notifier both listen, so this is a list rather than one slot.
    @ObservationIgnored private var observers: [@MainActor () -> Void] = []

    func observe(_ handler: @escaping @MainActor () -> Void) { observers.append(handler) }

    func stream(for id: String) -> PcStream {
        if let s = streams[id] { return s }
        let s = PcStream(pairingId: id)
        s.onChange = { [weak self] in
            guard let self else { return }
            for handler in self.observers { handler() }
        }
        streams[id] = s
        return s
    }

    /// The stream if one was ever opened, without opening one.
    func existing(_ id: String) -> PcStream? { streams[id] }

    func forget(_ id: String) { streams[id] = nil }
}
