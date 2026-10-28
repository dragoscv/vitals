import Foundation
import Testing
@testable import VitalsCore

// MARK: Contract — the same JSON the Rust test writes for Android
// (crates/vitals-server/tests/android_contract.rs). scripts/ios-contract.ps1
// copies them here; check-drift fails when the copies differ.

func fixture(_ name: String) throws -> Data {
    let url = try #require(Bundle.module.url(forResource: name, withExtension: nil, subdirectory: "Contract"))
    return try Data(contentsOf: url)
}

@Suite struct Contract {
    @Test func aFullKeyframeDecodesWithEveryOptionalMeasured() throws {
        let f = try JSONDecoder.vitals.decode(Frame.self, from: fixture("keyframe_full.json"))
        guard case let .keyframe(system, processes) = f.payload else { Issue.record("not a keyframe"); return }
        #expect(system.cpu.temperature == 81.5)
        #expect(system.cpu.throttled == .powerLimit)
        #expect(system.gpus.first?.utilization == 16)
        #expect(processes.first?.state == .notResponding)
        // FILETIME-sized start times survive as Int64, not rounded through Double.
        #expect(processes.first?.key.startTime == 133_700_000_000_000_000)
    }

    @Test func aSparseKeyframeKeepsUnmeasuredValuesNilNotZero() throws {
        let f = try JSONDecoder.vitals.decode(Frame.self, from: fixture("keyframe_sparse.json"))
        #expect(f.payload.system.cpu.temperature == nil)
        #expect(f.payload.system.cpu.power == nil)
        #expect(f.payload.system.battery == nil)
    }

    @Test func aMacFrameKeepsTheWindowsOnlyMemoryFiguresNil() throws {
        let m = try JSONDecoder.vitals.decode(Frame.self, from: fixture("keyframe_macos.json")).payload.system.memory
        #expect(m.committed == nil && m.commitLimit == nil && m.pagedPool == nil)
        #expect(m.nonPagedPool == nil && m.hardwareReserved == nil)
    }

    @Test func aDeltaCarriesExitsAndChanges() throws {
        let f = try JSONDecoder.vitals.decode(Frame.self, from: fixture("delta.json"))
        guard case let .delta(_, changed, exited) = f.payload else { Issue.record("not a delta"); return }
        #expect(exited == [7])
        #expect(changed.count == 1)
    }

    @Test func summaryHistorySensorsAlertsAndHostDecode() throws {
        #expect(try JSONDecoder.vitals.decode(Summary.self, from: fixture("summary.json")).processCount >= 1)
        #expect(try JSONDecoder.vitals.decode([MachineSample].self, from: fixture("history.json")).count == 2)
        #expect(try JSONDecoder.vitals.decode([SensorLine].self, from: fixture("sensors.json")).first?.unit == "temperature")
        #expect(try !JSONDecoder.vitals.decode([Alert].self, from: fixture("alerts.json")).isEmpty)
        #expect(try !JSONDecoder.vitals.decode(HostInfo.self, from: fixture("host.json")).hostname.isEmpty)
    }

    @Test func theAgentsCardReadsWhatThePcServes() throws {
        let a = try JSONDecoder.vitals.decode(AgentsSnapshot.self, from: fixture("agents.json"))
        #expect(a.agentqInstalled)
        #expect(!a.sessions.isEmpty)
    }

    @Test func anUnknownEnumFromANewerDesktopDecodesToUnknown() throws {
        var json = String(decoding: try fixture("keyframe_full.json"), as: UTF8.self)
        json = json.replacingOccurrences(of: "\"notResponding\"", with: "\"hibernating\"")
        let f = try JSONDecoder.vitals.decode(Frame.self, from: Data(json.utf8))
        guard case let .keyframe(_, processes) = f.payload else { Issue.record("not a keyframe"); return }
        #expect(processes.first?.state == .unknown)
    }
}

// MARK: Merge

func frame(_ json: String) throws -> Frame {
    try JSONDecoder.vitals.decode(Frame.self, from: Data(json.utf8))
}

@Suite struct Merge {
    func keyframeJSON() throws -> String { String(decoding: try fixture("keyframe_full.json"), as: UTF8.self) }
    func deltaJSON() throws -> String { String(decoding: try fixture("delta.json"), as: UTF8.self) }

    @Test func aDeltaBeforeAnyKeyframeIsDropped() throws {
        #expect(MachineView.fold(nil, try frame(deltaJSON())) == nil)
    }

    @Test func exitsApplyBeforeChangesSoARecycledPidSurvives() throws {
        let key = try frame(keyframeJSON())
        guard case let .keyframe(_, procs) = key.payload, let p = procs.first else { Issue.record("fixture"); return }
        // One delta: the old owner of the pid exited AND a new process took the
        // same pid (different start time). Applied in the wrong order, the new
        // owner would be deleted right after being inserted.
        let pid = p.key.pid
        let d = try JSONSerialization.jsonObject(with: Data(deltaJSON().utf8)) as! [String: Any]
        var payload = d["payload"] as! [String: Any]
        var changed = (payload["changed"] as! [[String: Any]])[0]
        changed["key"] = ["pid": pid, "startTime": 999]
        payload["changed"] = [changed]
        payload["exited"] = [pid]
        var out = d
        out["payload"] = payload
        let delta = try JSONDecoder.vitals.decode(Frame.self, from: JSONSerialization.data(withJSONObject: out))
        let view = try #require(MachineView.fold(MachineView.fold(nil, key), delta))
        #expect(view.processes[pid]?.key.startTime == 999, "the reused pid's new owner was removed")
    }

    @Test func aKeyframeReplacesEverything() throws {
        let key = try frame(keyframeJSON())
        let sparse = try JSONDecoder.vitals.decode(Frame.self, from: fixture("keyframe_sparse.json"))
        let view = try #require(MachineView.fold(MachineView.fold(nil, key), sparse))
        guard case let .keyframe(_, procs) = sparse.payload else { return }
        #expect(view.processes.count == procs.count)
        #expect(view.seq == sparse.seq)
    }
}

// MARK: Pairing

@Suite struct PairingRulesTests {
    let token = String(repeating: "A", count: 42) + "_"

    @Test func aQrLinkYieldsBaseUrlAndToken() {
        let l = PairingLink.parse("http://192.168.1.20:7331/mobile.html#t=\(token)")
        #expect(l == PairingLink(baseUrl: "http://192.168.1.20:7331", token: token))
    }

    @Test func aQrWithoutATokenOrWithAShortOneIsRefused() {
        #expect(PairingLink.parse("http://192.168.1.20:7331/mobile.html") == nil)
        #expect(PairingLink.parse("http://192.168.1.20:7331/#t=short") == nil)
        #expect(PairingLink.parse("ftp://192.168.1.20/#t=\(token)") == nil)
    }

    @Test func onlyPrivateAddressesAreAccepted() {
        for ok in ["http://10.0.0.5:7331", "http://172.20.1.1:7331", "http://192.168.0.2:7331",
                   "http://100.100.1.1:7331", "http://pc.local:7331", "http://[fe80::1]:7331", "http://[fd00::2]:7331"] {
            #expect(LanAddress.isPrivate(ok), "\(ok)")
        }
        for bad in ["http://8.8.8.8:7331", "http://10.0.0.1.evil.com:7331", "http://10.0.0.256:7331",
                    "http://example.com:7331", "http://172.32.0.1:7331", "http://[2001:db8::1]:7331"] {
            #expect(!LanAddress.isPrivate(bad), "\(bad)")
        }
    }

    @Test func addressesNormaliseWithTheDefaultPort() {
        #expect(PairingRules.normalise(" 192.168.1.2/ ") == "http://192.168.1.2:7331")
        #expect(PairingRules.normalise("https://pc.local:9000") == "https://pc.local:9000")
        #expect(PairingRules.normalise("") == nil)
    }

    @Test func aCodeIsSixDigitsWhateverTheSpacing() {
        #expect(PairingRules.isCode("482 913"))
        #expect(!PairingRules.isCode("48291"))
        #expect(PairingRules.cleanCode("4-8 2.9 1-3") == "482913")
    }

    @Test func backoffDoublesToThirtySecondsWithJitter() {
        var b = Backoff()
        var rng = SeededRNG(seed: 1)
        let first = b.next(using: &rng)
        #expect((400...600).contains(first))
        for _ in 0..<20 { _ = b.next(using: &rng) }
        #expect(b.next(using: &rng) <= 36_000)
        b.reset()
        #expect(b.failures == 0)
    }

    @Test func controlRequestsAreTaggedInKebabCase() throws {
        let key = ProcessKey(pid: 42, startTime: 133_700_000_000_000_000)
        let json = String(decoding: try JSONEncoder().encode(ControlRequest.setPriority(key, .belowNormal)), as: UTF8.self)
        #expect(json.contains("\"action\":\"set-priority\""))
        #expect(json.contains("\"priority\":\"below-normal\""))
        #expect(json.contains("133700000000000000"))
    }
}

// MARK: Format

@Suite struct FormatTests {
    let en = Locale(identifier: "en_GB")
    let ro = Locale(identifier: "ro_RO")

    @Test func unmeasuredIsADashNeverZero() {
        #expect(Format.percent(nil) == "—")
        #expect(Format.percent(.nan) == "—")
        #expect(Format.bytes(nil) == "—")
        #expect(Format.celsius(nil) == "—")
        #expect(Format.rate(nil) == "—")
    }

    @Test func numbersReadTheWayTheOtherSurfacesWriteThem() {
        #expect(Format.percent(43.4) == "43 %")
        #expect(Format.bytes(1536, locale: en) == "1.5 KB")
        #expect(Format.bytes(1536, locale: ro) == "1,5 KB")
        #expect(Format.bytes(200 * 1024 * 1024, locale: en) == "200 MB")
        #expect(Format.watts(7.5, locale: en) == "7.5 W")
        #expect(Format.duration(90_061) == "1d 1h")
    }

    @Test func thresholdsMatchTheDesktop() {
        #expect(Level.cpu(84.9) == .ok)
        #expect(Level.cpu(85) == .warn)
        #expect(Level.cpu(95) == .danger)
        #expect(Level.gpuTemp(80) == .warn)
        #expect(Level.cpu(nil) == .unknown)
    }
}

struct SeededRNG: RandomNumberGenerator {
    var state: UInt64
    init(seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state = state &* 6364136223846793005 &+ 1442695040888963407
        return state
    }
}
