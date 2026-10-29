import Foundation
import Testing
@testable import VitalsCore

/// StubProtocol's shape, with its own table: `.serialized` orders tests
/// within a suite, not across suites, so sharing ClientTests' statics would
/// let the two suites overwrite each other's replies.
final class HealthStub: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var replies: [String: StubProtocol.Reply] = [:]
    nonisolated(unsafe) static var seenStore: [URLRequest] = []
    static let lock = NSLock()

    static var seen: [URLRequest] { lock.withLock { seenStore } }

    static func reset(_ r: [String: StubProtocol.Reply]) {
        lock.withLock {
            replies = r
            seenStore = []
        }
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let path = request.url.map { $0.path + ($0.query.map { "?\($0)" } ?? "") } ?? ""
        let reply = Self.lock.withLock { () -> StubProtocol.Reply? in
            Self.seenStore.append(request)
            return Self.replies[path]
        }
        guard let reply, let url = request.url,
              let resp = HTTPURLResponse(url: url, statusCode: reply.status, httpVersion: "HTTP/1.1", headerFields: reply.headers)
        else {
            client?.urlProtocol(self, didFailWithError: URLError(.cannotConnectToHost))
            return
        }
        if (300..<400).contains(reply.status), let loc = reply.headers["Location"], let to = URL(string: loc) {
            client?.urlProtocol(self, wasRedirectedTo: URLRequest(url: to), redirectResponse: resp)
        }
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: reply.body)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    static func session() -> URLSession {
        let c = URLSessionConfiguration.ephemeral
        c.protocolClasses = [HealthStub.self]
        return URLSession(configuration: c, delegate: NoRedirects(), delegateQueue: nil)
    }
}

// Serialised: the stub table is shared by every test in this suite.
@Suite(.serialized) struct HealthTests {
    let base = "https://vmui.example.org"
    let token = "vmd_" + String(repeating: "A", count: 32)

    func client() -> HealthClient { HealthClient(session: HealthStub.session()) }

    static let full = #"""
    {
      "at": 1793200000000,
      "extraTopLevel": {"ignored": true},
      "health": {
        "device": "phone", "steps": 8123, "stepsAt": 1793200000000, "distanceKm": 6.1, "floors": 4,
        "activeKcal": 410, "totalKcal": 2310, "heartRate": 72, "heartRateAt": 1793200000000,
        "restingHr": 58, "hrv": 41, "spo2": 97, "respiratoryRate": 14, "sleepMin": 412,
        "sleepAt": 1793200000000, "weightKg": 81.4, "weightAt": 1793200000000, "bodyFat": 18.2,
        "impedance": 500,
        "body": {"bmi": 27.2, "fatPct": 21.0, "fatKg": 17.1, "waterKg": 46.0, "waterPct": 56.5,
                 "leanKg": 64.3, "muscleKg": 35.0, "bmrKcal": 1780, "boneKg": 3.2, "ageYears": 32,
                 "bmiBand": "over"},
        "bmr": 1790, "vo2max": 44, "bloodPressure": {"sys": 121, "dia": 79},
        "hrHistory": [{"t": 1793100000000, "v": 71}, {"t": 1793150000000, "v": 75}, {"t": null, "v": 80}],
        "somethingNew": 1
      },
      "nutrition": {
        "day": "2026-10-28",
        "targets": {"weightKg": 81, "weightSource": "scale", "ageYears": 32, "bmr": 1780, "tdee": 2400,
                    "calories": 2100, "protein": 160, "carbs": 220, "fats": 70, "fiber": 25},
        "today": {"day": "2026-10-28", "calories": 1460, "protein": 98, "carbs": 150, "fats": 50,
                  "fiber": 18, "meals": 3},
        "remaining": {"calories": 640, "protein": 62, "carbs": 70, "fats": -20},
        "balance": -350, "activeKcal": 410,
        "water": {"day": "2026-10-28", "ml": 1500, "glasses": 6, "targetMl": 2500, "remainingMl": 1000,
                  "lastAt": null, "underPace": true, "entries": [], "week": [{"day": "2026-10-27", "ml": 2100}]},
        "week": []
      }
    }
    """#

    static let sparse = #"""
    {"at": 1793200000000,
     "health": {"device": "phone", "steps": null, "heartRate": "n/a", "body": null, "bloodPressure": {"sys": 120},
                "hrHistory": null},
     "nutrition": null}
    """#

    // MARK: Address

    @Test func onlyHttpsOrLocalHttpIsAccepted() {
        #expect(HealthClient.normalise("https://vmui.example.org/") == "https://vmui.example.org")
        #expect(HealthClient.normalise("  HTTPS://VMUI.example.org:8443/base// ") == "https://vmui.example.org:8443/base")
        #expect(HealthClient.normalise("vmui.example.org") == "https://vmui.example.org")
        #expect(HealthClient.normalise("http://localhost:3000") == "http://localhost:3000")
        #expect(HealthClient.normalise("http://127.0.0.1:3000") == "http://127.0.0.1:3000")
        #expect(HealthClient.normalise("http://vmui.example.org") == nil)
        #expect(HealthClient.normalise("http://192.168.1.20") == nil)
        #expect(HealthClient.normalise("ftp://vmui.example.org") == nil)
        #expect(HealthClient.normalise("https://user:pw@vmui.example.org") == nil)
        #expect(HealthClient.normalise("") == nil)
    }

    @Test func aPlainHttpAddressNeverOpensASocket() async {
        HealthStub.reset([:])
        let r = await client().fetch(base: "http://vmui.example.org", token: token)
        #expect(r.failureValue == .badAddress)
        #expect(HealthStub.seen.isEmpty)
    }

    // MARK: Decoding

    @Test func aFullBodyDecodesEveryReading() async throws {
        HealthStub.reset(["/api/v1/me/health": .init(status: 200, body: Data(Self.full.utf8))])
        let me = try await client().fetch(base: base, token: token).get()
        let h = try #require(me.health)
        #expect(me.at == 1_793_200_000_000)
        #expect(h.steps == 8123)
        #expect(h.distanceKm == 6.1)
        #expect(h.heartRate == 72)
        #expect(h.bloodPressure?.sys == 121 && h.bloodPressure?.dia == 79)
        #expect(h.body?.bmi == 27.2)
        #expect(h.body?.bmiBand == "over")
        #expect(h.vo2max == 44)
        // The point without a time is dropped, not drawn at zero.
        #expect(h.hrHistory.count == 2)
        let n = try #require(me.nutrition)
        #expect(n.targets?.calories == 2100)
        #expect(n.today?.protein == 98)
        #expect(n.remaining?.fats == -20)
        #expect(n.water?.targetMl == 2500)
        #expect(n.water?.lastAt == nil)
    }

    @Test func nullsMissingKeysAndWrongTypesBecomeNilNotZero() async throws {
        HealthStub.reset(["/api/v1/me/health": .init(status: 200, body: Data(Self.sparse.utf8))])
        let me = try await client().fetch(base: base, token: token).get()
        let h = try #require(me.health)
        #expect(h.steps == nil)
        #expect(h.heartRate == nil)
        #expect(h.distanceKm == nil)
        #expect(h.body == nil)
        #expect(h.bloodPressure?.dia == nil)
        #expect(HealthFormat.pressure(h.bloodPressure) == Format.dash)
        #expect(h.hrHistory.isEmpty)
        #expect(me.nutrition == nil)
        #expect(HealthFormat.value(h.steps, "") == Format.dash)
    }

    @Test func aNullHealthIsNotAFailure() async throws {
        HealthStub.reset(["/api/v1/me/health": .init(status: 200, body: Data(#"{"at":1,"health":null,"nutrition":null}"#.utf8))])
        let me = try await client().fetch(base: base, token: token).get()
        #expect(me.health == nil && me.nutrition == nil)
    }

    @Test func aBodyThatIsNotTheContractIsInvalid() async {
        HealthStub.reset(["/api/v1/me/health": .init(status: 200, body: Data("<html>".utf8))])
        #expect(await client().fetch(base: base, token: token).failureValue == .invalid)
    }

    // MARK: Status codes

    @Test func unauthorisedIsSignedOutAndForbiddenIsNotOwner() async {
        HealthStub.reset(["/api/v1/me/health": .init(status: 401)])
        #expect(await client().fetch(base: base, token: token).failureValue == .signedOut)
        HealthStub.reset(["/api/v1/me/health": .init(status: 403)])
        #expect(await client().fetch(base: base, token: token).failureValue == .notOwner)
        HealthStub.reset(["/api/v1/me/health": .init(status: 500)])
        #expect(await client().fetch(base: base, token: token).failureValue == .unreachable)
        HealthStub.reset([:])
        #expect(await client().fetch(base: base, token: token).failureValue == .unreachable)
    }

    // MARK: Token handling

    @Test func theTokenTravelsOnlyInTheAuthorizationHeader() async throws {
        HealthStub.reset(["/api/v1/me/health": .init(status: 200, body: Data(Self.full.utf8))])
        _ = await client().fetch(base: base, token: token)
        let req = try #require(HealthStub.seen.first)
        #expect(req.value(forHTTPHeaderField: "Authorization") == "Bearer \(token)")
        #expect(!(req.url?.absoluteString.contains(token) ?? true))
        #expect(!(req.url?.absoluteString.contains("vmd_") ?? true))
    }

    @Test func aRedirectIsNeverFollowed() async throws {
        HealthStub.reset([
            "/api/v1/me/health": .init(status: 302, headers: ["Location": "https://evil.example.net/steal"]),
            "/steal": .init(status: 200, body: Data(Self.full.utf8)),
        ])
        let r = await client().fetch(base: base, token: token)
        #expect(HealthStub.seen.allSatisfy { $0.url?.host != "evil.example.net" }, "followed the redirect")
        if case .success = r { Issue.record("a redirect must not produce data") }
    }

    // MARK: Pairing

    @Test func codePairingStartsWithoutATokenAndNamesThePlatform() async throws {
        HealthStub.reset([
            "/api/devices/pair": .init(
                status: 200,
                body: Data(#"{"id":"0123456789abcdef","token":"vmd_x","code":"1234","status":"pending"}"#.utf8)),
        ])
        let t = try await client().pairStart(base: base, name: "iPhone").get()
        #expect(t.code == "1234" && t.id == "0123456789abcdef")
        let req = try #require(HealthStub.seen.first)
        #expect(req.httpMethod == "POST")
        #expect(req.value(forHTTPHeaderField: "Authorization") == nil)
        let body = try #require(Self.body(req))
        #expect(body.contains("\"platform\":\"vitals-ios\""))
        #expect(!body.contains("password"))
    }

    @Test func pollingReadsApprovalRejectionAndExpiry() async {
        let path = "/api/devices/pair?id=0123456789abcdef"
        HealthStub.reset([path: .init(status: 200, body: Data(#"{"status":"pending"}"#.utf8))])
        #expect(await client().pairPoll(base: base, id: "0123456789abcdef").successValue == .pending)
        HealthStub.reset([path: .init(status: 200, body: Data(#"{"status":"approved"}"#.utf8))])
        #expect(await client().pairPoll(base: base, id: "0123456789abcdef").successValue == .approved)
        HealthStub.reset([path: .init(status: 200, body: Data(#"{"status":"rejected"}"#.utf8))])
        #expect(await client().pairPoll(base: base, id: "0123456789abcdef").successValue == .rejected)
        HealthStub.reset([path: .init(status: 404)])
        #expect(await client().pairPoll(base: base, id: "0123456789abcdef").successValue == .expired)
        HealthStub.reset([path: .init(status: 429)])
        #expect(await client().pairPoll(base: base, id: "0123456789abcdef").failureValue == .slowDown)
    }

    @Test func signInReturnsTheTokenAndWrongCredentialsAreNamed() async throws {
        HealthStub.reset([
            "/api/devices/pair": .init(status: 200, body: Data(#"{"id":"a","token":"vmd_ok","status":"approved"}"#.utf8)),
        ])
        let token = try await client().signIn(base: base, email: "me@example.org", password: "pw").get()
        #expect(token == "vmd_ok")
        HealthStub.reset(["/api/devices/pair": .init(status: 401)])
        #expect(await client().signIn(base: base, email: "me@example.org", password: "bad").failureValue == .wrongCredentials)
    }

    // MARK: Formatting

    @Test func formattingNeverTurnsMissingIntoZero() {
        let en = Locale(identifier: "en_GB")
        #expect(HealthFormat.value(nil, "bpm") == Format.dash)
        #expect(HealthFormat.value(72, "bpm", locale: en) == "72 bpm")
        #expect(HealthFormat.value(6.1, "km", digits: 1, locale: en) == "6.1 km")
        #expect(HealthFormat.of(nil, nil, "g") == Format.dash)
        #expect(HealthFormat.of(98, 160, "g", locale: en) == "98 / 160 g")
        #expect(HealthFormat.of(nil, 160, "g", locale: en) == "\(Format.dash) / 160 g")
        #expect(HealthFormat.sleep(412) == "6h 52m")
        #expect(HealthFormat.sleep(nil) == Format.dash)
        #expect(HealthFormat.fraction(nil, 100) == nil)
        #expect(HealthFormat.fraction(50, 0) == nil)
        #expect(HealthFormat.fraction(50, 100) == 0.5)
    }

    /// URLProtocol often sees the body as a stream rather than httpBody.
    static func body(_ req: URLRequest) -> String? {
        if let d = req.httpBody { return String(decoding: d, as: UTF8.self) }
        guard let s = req.httpBodyStream else { return nil }
        s.open()
        defer { s.close() }
        var out = Data()
        var buf = [UInt8](repeating: 0, count: 4096)
        while s.hasBytesAvailable {
            let n = s.read(&buf, maxLength: buf.count)
            if n <= 0 { break }
            out.append(buf, count: n)
        }
        return String(decoding: out, as: UTF8.self)
    }
}

extension Result {
    var successValue: Success? {
        if case let .success(s) = self { return s }
        return nil
    }
}
