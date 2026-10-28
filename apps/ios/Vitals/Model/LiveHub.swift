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
    private(set) var state: PcState = .connecting
    private(set) var view: MachineView?
    private(set) var alerts: [VitalsCore.Alert] = []
    private(set) var cpuHistory: [Double] = []
    private(set) var memHistory: [Double] = []
    private(set) var lastSeenMs: Int64?

    private var task: Task<Void, Never>?
    private var alertTask: Task<Void, Never>?
    private var backoff = Backoff()
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
        task = Task { [weak self] in await self?.run(client) }
        alertTask = Task { [weak self] in
            while !Task.isCancelled {
                if case let .success(a) = await client.alerts() { self?.alerts = a }
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    private func run(_ client: VitalsClient) async {
        var failures = 0
        while !Task.isCancelled {
            guard LanGate.shared.isOnLan else {
                state = .offLan
                try? await Task.sleep(for: .seconds(3))
                continue
            }
            do {
                for try await frame in client.frames() {
                    fold(frame)
                    failures = 0
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
            failures += 1
            if failures >= 3 { state = .unreachable(lastSeenMs: lastSeenMs) }
            let delay = backoff.next()
            try? await Task.sleep(for: .milliseconds(Int(delay)))
        }
    }

    private func fold(_ frame: Frame) {
        guard let next = MachineView.fold(view, frame) else { return }
        view = next
        state = .live
        lastSeenMs = Int64(Date().timeIntervalSince1970 * 1000)
        cpuHistory = Array((cpuHistory + [next.system.cpu.total]).suffix(60))
        memHistory = Array((memHistory + [next.system.memory.usedPercent ?? 0]).suffix(60))
    }
}

@MainActor @Observable
final class LiveHub {
    private var streams: [String: PcStream] = [:]

    func stream(for id: String) -> PcStream {
        if let s = streams[id] { return s }
        let s = PcStream(pairingId: id)
        streams[id] = s
        return s
    }

    func forget(_ id: String) { streams[id] = nil }
}
