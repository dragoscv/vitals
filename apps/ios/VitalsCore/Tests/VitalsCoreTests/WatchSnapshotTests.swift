import Foundation
import Testing
@testable import VitalsCore

@Suite struct WatchSnapshotTests {
    let pairing = Pairing(
        id: "pc-1", label: "Desk PC", baseUrl: "http://192.168.1.20:7331",
        token: "vt_secret_token_value_123", scope: .control, mac: "AA:BB:CC:DD:EE:FF",
        createdMs: 1)

    func system(_ name: String) throws -> SystemMetrics {
        try JSONDecoder.vitals.decode(Frame.self, from: fixture(name)).payload.system
    }

    @Test func aSnapshotSurvivesEncodingAndDecodingUnchanged() throws {
        let pc = WatchPc(
            pairing: pairing, state: .live, system: try system("keyframe_full.json"),
            alerts: 2, lastSeenMs: 1_700_000_000_000)
        let sample = WatchPc(
            id: "sample", label: "Sample PC", state: .live, cpuPercent: 22, memoryPercent: 41,
            tempC: nil, gpuPercent: nil, alerts: 0, lastSeenMs: 5, sample: true)
        let snap = WatchSnapshot(sentMs: 1_700_000_000_500, pcs: [pc, sample])
        let back = try WatchSnapshot.decoded(from: snap.encoded())
        #expect(back == snap)
        #expect(back.pcs[0].gpuPercent == 16)
        #expect(back.pcs[1].sample)
    }

    @Test func unmeasuredReadingsStayNilAcrossTheWireRatherThanBecomingZero() throws {
        let pc = WatchPc(
            pairing: pairing, state: .live, system: try system("keyframe_sparse.json"),
            alerts: 0, lastSeenMs: nil)
        let back = try WatchSnapshot.decoded(from: WatchSnapshot(sentMs: 0, pcs: [pc]).encoded())
        #expect(back.pcs[0].tempC == nil)
        #expect(back.pcs[0].lastSeenMs == nil)
    }

    @Test func aPcThatIsNotLiveSendsNoReadingsSoAStaleNumberNeverLooksCurrent() throws {
        let pc = WatchPc(
            pairing: pairing, state: .offline, system: try system("keyframe_full.json"),
            alerts: 1, lastSeenMs: 42)
        #expect(pc.cpuPercent == nil && pc.memoryPercent == nil && pc.tempC == nil && pc.gpuPercent == nil)
        #expect(pc.alerts == 1 && pc.lastSeenMs == 42)
    }

    @Test func nanAndInfinityAreTreatedAsUnmeasuredSoTheSnapshotStillEncodes() throws {
        let pc = WatchPc(
            id: "x", label: "x", state: .live, cpuPercent: .nan, memoryPercent: .infinity,
            tempC: 50, gpuPercent: nil, alerts: -3, lastSeenMs: nil, sample: false)
        #expect(pc.cpuPercent == nil && pc.memoryPercent == nil && pc.tempC == 50)
        #expect(pc.alerts == 0)
        _ = try WatchSnapshot(sentMs: 0, pcs: [pc]).encoded()
    }

    @Test func theTokenAddressAndMacNeverAppearInTheEncodedPayload() throws {
        let pc = WatchPc(
            pairing: pairing, state: .live, system: try system("keyframe_full.json"),
            alerts: 0, lastSeenMs: 1)
        let text = try #require(String(data: WatchSnapshot(sentMs: 0, pcs: [pc]).encoded(), encoding: .utf8))
        for secret in [pairing.token, pairing.baseUrl, "192.168.1.20", "AA:BB:CC:DD:EE:FF"] {
            #expect(!text.contains(secret), "leaked \(secret)")
        }
    }

    @Test func theEncodedPcCarriesOnlyTheClosedListOfKeys() throws {
        let pc = WatchPc(
            pairing: pairing, state: .live, system: try system("keyframe_full.json"),
            alerts: 0, lastSeenMs: 1)
        let data = try WatchSnapshot(sentMs: 0, pcs: [pc]).encoded()
        let obj = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(Set(obj.keys) == ["version", "sentMs", "pcs"])
        let first = try #require((obj["pcs"] as? [[String: Any]])?.first)
        // JSONEncoder omits nil optionals, so the set is a subset of this list.
        let allowed: Set<String> = [
            "id", "label", "state", "cpuPercent", "memoryPercent", "tempC", "gpuPercent",
            "alerts", "lastSeenMs", "sample",
        ]
        #expect(Set(first.keys).isSubset(of: allowed))
    }

    @Test func aStateFromANewerPhoneDecodesAsUnknownInsteadOfDroppingTheSnapshot() throws {
        let json = #"{"version":2,"sentMs":1,"pcs":[{"id":"a","label":"A","state":"sleeping","alerts":0,"sample":false}]}"#
        let snap = try WatchSnapshot.decoded(from: Data(json.utf8))
        #expect(snap.pcs.first?.state == .unknown)
        #expect(snap.pcs.first?.cpuPercent == nil)
    }

    @Test func informationalAlertsAreNotCountedOnTheWrist() throws {
        let alerts = try JSONDecoder.vitals.decode([Alert].self, from: fixture("alerts.json"))
        #expect(WatchPc.seriousCount(alerts) == alerts.filter { $0.severity != .info }.count)
        #expect(WatchPc.seriousCount([]) == 0)
    }

    @Test func aSnapshotBecomesStaleOnlyAfterTwoMinutes() {
        let snap = WatchSnapshot(sentMs: 1_000, pcs: [])
        #expect(!snap.isStale(nowMs: 1_000 + 120_000))
        #expect(snap.isStale(nowMs: 1_000 + 120_001))
    }
}

@Suite struct WatchSendPolicyTests {
    let policy = WatchSendPolicy()
    let t0 = Date(timeIntervalSince1970: 1_000)

    @Test func theFirstUpdateIsSentAtOnceBothAsAMessageAndAsTheContext() {
        let d = policy.decide(now: t0, reachable: true, lastMessage: nil, lastContext: nil)
        #expect(d == .init(message: true, context: true))
    }

    @Test func aReachableWatchGetsAMessageAtMostEveryFiveSeconds() {
        let early = policy.decide(
            now: t0.addingTimeInterval(4.9), reachable: true, lastMessage: t0, lastContext: t0)
        let due = policy.decide(
            now: t0.addingTimeInterval(5), reachable: true, lastMessage: t0, lastContext: t0)
        #expect(!early.message)
        #expect(due.message)
    }

    @Test func anUnreachableWatchNeverGetsAMessageAndTheContextAtMostEveryMinute() {
        let early = policy.decide(
            now: t0.addingTimeInterval(59), reachable: false, lastMessage: nil, lastContext: t0)
        let due = policy.decide(
            now: t0.addingTimeInterval(60), reachable: false, lastMessage: nil, lastContext: t0)
        #expect(early == .nothing)
        #expect(due == .init(message: false, context: true))
    }
}
