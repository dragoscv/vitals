import Foundation
import Testing
@testable import VitalsCore

/// Answers requests from a table and records what was asked, so the client's
/// guarantees are tested without a PC.
final class StubProtocol: URLProtocol, @unchecked Sendable {
    struct Reply: Sendable {
        var status: Int
        var body: Data = Data()
        var headers: [String: String] = [:]
    }

    nonisolated(unsafe) static var replies: [String: Reply] = [:]
    nonisolated(unsafe) static var seen: [URLRequest] = []
    static let lock = NSLock()

    static func reset(_ r: [String: Reply]) {
        lock.withLock {
            replies = r
            seen = []
        }
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let path = request.url.map { $0.path + ($0.query.map { "?\($0)" } ?? "") } ?? ""
        let reply = Self.lock.withLock { () -> Reply? in
            Self.seen.append(request)
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
        c.protocolClasses = [StubProtocol.self]
        return URLSession(configuration: c, delegate: NoRedirects(), delegateQueue: nil)
    }
}

// Serialised: the stub table is shared.
@Suite(.serialized) struct ClientTests {
    let token = String(repeating: "b", count: 43)
    let base = "http://192.168.1.20:7331"

    func health() throws -> Data { Data(#"{"ok":true,"version":"1.1.1","modelVersion":1}"#.utf8) }

    @Test func theTokenTravelsOnlyInTheAuthorizationHeader() async throws {
        StubProtocol.reset(["/api/v1/summary?top=1": .init(status: 200, body: try fixture("summary.json"))])
        let c = VitalsClient(baseUrl: base, token: token, session: StubProtocol.session(), gate: LanGate())
        _ = await c.summary(top: 1)
        let req = try #require(StubProtocol.seen.first)
        #expect(req.value(forHTTPHeaderField: "Authorization") == "Bearer \(token)")
        #expect(!(req.url?.absoluteString.contains(token) ?? true))
    }

    @Test func aRedirectIsNeverFollowed() async throws {
        StubProtocol.reset([
            "/api/v1/summary?top=1": .init(status: 302, headers: ["Location": "http://8.8.8.8/steal"]),
            "/steal": .init(status: 200, body: try fixture("summary.json")),
        ])
        let c = VitalsClient(baseUrl: base, token: token, session: StubProtocol.session(), gate: LanGate())
        let r = await c.summary(top: 1)
        #expect(StubProtocol.seen.allSatisfy { $0.url?.host != "8.8.8.8" }, "followed the redirect")
        if case .success = r { Issue.record("a redirect must not produce data") }
    }

    @Test func offTheLanNoAuthenticatedRequestLeaves() async throws {
        StubProtocol.reset([:])
        let gate = LanGate()
        gate.isOnLan = false
        let c = VitalsClient(baseUrl: base, token: token, session: StubProtocol.session(), gate: gate)
        #expect(await c.summary().failureValue == .offLan)
        #expect(StubProtocol.seen.isEmpty)
    }

    @Test func pairingSavesOnlyAfterHealthAndSummaryAnswer() async throws {
        StubProtocol.reset([
            "/api/v1/health": .init(status: 200, body: try health()),
            "/api/v1/summary?top=1": .init(status: 200, body: try fixture("summary.json")),
            "/api/v1/host": .init(status: 200, body: try fixture("host.json")),
        ])
        let out = await PairCheck.withToken(
            rawUrl: "192.168.1.20", rawToken: token, label: nil, existing: [], gate: LanGate(),
            session: StubProtocol.session())
        guard case let .ok(p, _) = out else { Issue.record("refused: \(out)"); return }
        #expect(p.baseUrl == base)
        #expect(!p.label.isEmpty)
    }

    @Test func aWrongTokenIsUnauthorisedAndNothingIsSaved() async throws {
        StubProtocol.reset([
            "/api/v1/health": .init(status: 200, body: try health()),
            "/api/v1/summary?top=1": .init(status: 401),
        ])
        let out = await PairCheck.withToken(
            rawUrl: base, rawToken: token, label: nil, existing: [], gate: LanGate(), session: StubProtocol.session())
        guard case let .refused(reason, _) = out else { Issue.record("saved"); return }
        #expect(reason == .unauthorised)
    }

    @Test func aPublicAddressIsRefusedBeforeAnySocketOpens() async {
        StubProtocol.reset([:])
        let out = await PairCheck.withToken(
            rawUrl: "8.8.8.8", rawToken: token, label: nil, existing: [], gate: LanGate(), session: StubProtocol.session())
        guard case let .refused(reason, _) = out else { Issue.record("saved"); return }
        #expect(reason == .notPrivate)
        #expect(StubProtocol.seen.isEmpty)
    }

    @Test func aRejectedCodeIsBadCodeButAMissingRouteIsNot() async throws {
        StubProtocol.reset(["/api/v1/pair": .init(status: 403, body: Data(#"{"error":"invalid pairing code"}"#.utf8))])
        let refused = await PairCheck.withCode(
            rawUrl: base, rawCode: "482 913", deviceLabel: "iPhone", pcLabel: nil, existing: [], gate: LanGate(),
            session: StubProtocol.session())
        guard case let .refused(r1, _) = refused else { Issue.record("accepted"); return }
        #expect(r1 == .badCode)
        // The pair request carries no bearer token: the code is the credential.
        #expect(StubProtocol.seen.first?.value(forHTTPHeaderField: "Authorization") == nil)

        StubProtocol.reset(["/api/v1/pair": .init(status: 404)])
        let missing = await PairCheck.withCode(
            rawUrl: base, rawCode: "482913", deviceLabel: "iPhone", pcLabel: nil, existing: [], gate: LanGate(),
            session: StubProtocol.session())
        guard case let .refused(r2, status) = missing else { Issue.record("accepted"); return }
        #expect(r2 == .failed && status == 404)
    }

    @Test func aMistypedCodeNeverReachesThePc() async {
        StubProtocol.reset([:])
        let out = await PairCheck.withCode(
            rawUrl: base, rawCode: "48291", deviceLabel: "iPhone", pcLabel: nil, existing: [], gate: LanGate(),
            session: StubProtocol.session())
        guard case let .refused(reason, _) = out else { Issue.record("accepted"); return }
        #expect(reason == .badCode)
        #expect(StubProtocol.seen.isEmpty)
    }

    @Test func aControlRefusalCarriesItsReason() async throws {
        StubProtocol.reset(["/api/v1/control": .init(status: 403, body: Data(#"{"kind":"forbidden"}"#.utf8))])
        let c = VitalsClient(baseUrl: base, token: token, session: StubProtocol.session(), gate: LanGate())
        let r = await c.control(.terminate(ProcessKey(pid: 1, startTime: 2)))
        #expect(r.failureValue == .refused(.forbidden))
    }

    @Test func aNewerModelVersionIsIncompatible() async {
        StubProtocol.reset(["/api/v1/health": .init(status: 200, body: Data(#"{"ok":true,"version":"9","modelVersion":2}"#.utf8))])
        let c = VitalsClient(baseUrl: base, token: token, session: StubProtocol.session(), gate: LanGate())
        #expect(await c.health().failureValue == .incompatible(modelVersion: 2))
    }
}

extension Result {
    var failureValue: Failure? {
        if case let .failure(f) = self { return f }
        return nil
    }
}
