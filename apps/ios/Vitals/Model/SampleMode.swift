// The labelled "Sample PC": lets someone without a paired PC (and App Review)
// see every screen with plausible numbers. It is never presented as real: a
// banner says "Sample data, not a real PC" on every screen that shows it, it
// cannot be controlled, and it disappears from the list once a real PC is
// paired unless the user brings it back.

import Foundation
import Observation
import VitalsCore

@MainActor @Observable
final class SampleMode {
    static let id = "sample"
    var enabled: Bool {
        didSet {
            UserDefaults.standard.set(enabled, forKey: "sampleEnabled")
            onChange?()
        }
    }

    private(set) var view: MachineView? {
        didSet { onChange?() }
    }
    /// Told of every sample tick and toggle (the watch bridge).
    @ObservationIgnored var onChange: (@MainActor () -> Void)?
    private(set) var cpuHistory: [Double] = []
    private(set) var memHistory: [Double] = []
    private var ticker: Task<Void, Never>?

    init() {
        enabled = UserDefaults.standard.bool(forKey: "sampleEnabled")
    }

    var pairing: Pairing {
        Pairing(id: Self.id, label: L("sample_title"), baseUrl: "sample", token: "", scope: .read, createdMs: 0)
    }

    func start() {
        guard ticker == nil else { return }
        let base = SampleData.keyframe()
        ticker = Task { [weak self] in
            var t = 0.0
            while !Task.isCancelled {
                guard let self, let base else { return }
                let f = SampleData.wobble(base, t: t)
                self.view = MachineView.fold(nil, f)
                if let s = self.view?.system {
                    self.cpuHistory = Array((self.cpuHistory + [s.cpu.total]).suffix(60))
                    self.memHistory = Array((self.memHistory + [s.memory.usedPercent ?? 0]).suffix(60))
                }
                t += 1
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    func stop() {
        ticker?.cancel()
        ticker = nil
    }
}

enum SampleData {
    static func keyframe() -> Frame? {
        guard let url = Bundle.main.url(forResource: "sample-keyframe", withExtension: "json"),
              let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder.vitals.decode(Frame.self, from: data)
    }

    static func history() -> [MachineSample] {
        guard let url = Bundle.main.url(forResource: "sample-history", withExtension: "json"),
              let data = try? Data(contentsOf: url) else { return [] }
        return (try? JSONDecoder.vitals.decode([MachineSample].self, from: data)) ?? []
    }

    /// Re-encodes the keyframe with gently moving CPU so the sample looks alive.
    static func wobble(_ base: Frame, t: Double) -> Frame {
        guard let data = try? JSONSerialization.data(withJSONObject: rawWobble(base, t: t)),
              let f = try? JSONDecoder.vitals.decode(Frame.self, from: data) else { return base }
        return f
    }

    private static func rawWobble(_ base: Frame, t: Double) -> Any {
        guard let url = Bundle.main.url(forResource: "sample-keyframe", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              var obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              var payload = obj["payload"] as? [String: Any],
              var system = payload["system"] as? [String: Any],
              var cpu = system["cpu"] as? [String: Any] else { return [:] }
        let total = 22 + 9 * sin(t / 6) + Double.random(in: -2...2)
        cpu["total"] = total
        if let cores = cpu["perCore"] as? [Double] {
            cpu["perCore"] = cores.enumerated().map { i, _ in max(0, min(100, total + 12 * sin(t / 4 + Double(i)))) }
        }
        system["cpu"] = cpu
        payload["system"] = system
        obj["payload"] = payload
        obj["seq"] = Int(t) + 1
        return obj
    }
}
