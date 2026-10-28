// The LAN API of one PC (docs/api/openapi.yaml), over URLSession.
//
// Same rules as the Android client:
// - The token is only ever sent in the Authorization header.
// - Redirects are never followed: a LAN API never redirects, and following
//   one would replay the bearer token at whatever host the answer names.
// - Short timeouts: on a LAN a PC answers in milliseconds or is not there.
// - Authenticated calls are refused off the LAN (`isOnLan` false), so a
//   token never travels over mobile data, even through a VPN.

import Foundation

public enum ApiFailure: Error, Equatable, Sendable {
    case unreachable
    case unauthorised
    case offLan
    case incompatible(modelVersion: Int)
    case notReady
    case http(Int)
    case refused(ControlFailure)
}

public struct PairGrant: Sendable, Equatable {
    public let token: String
    public let scope: Scope
}

/// Shared answer to "may this device talk to a PC right now?". The app sets
/// it from NWPathMonitor (Wi-Fi or wired, not cellular).
public final class LanGate: @unchecked Sendable {
    public static let shared = LanGate()
    private let lock = NSLock()
    private var onLan = true

    public var isOnLan: Bool {
        get { lock.withLock { onLan } }
        set { lock.withLock { onLan = newValue } }
    }
}

final class NoRedirects: NSObject, URLSessionTaskDelegate, Sendable {
    func urlSession(
        _ session: URLSession, task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest
    ) async -> URLRequest? { nil }
}

public struct VitalsClient: Sendable {
    public static let supportedModelVersion = 1

    public let baseUrl: String
    let token: String
    let session: URLSession
    let gate: LanGate

    static let sharedSession: URLSession = {
        let c = URLSessionConfiguration.ephemeral
        c.timeoutIntervalForRequest = 10
        c.timeoutIntervalForResource = 15
        c.waitsForConnectivity = false
        c.httpShouldSetCookies = false
        c.urlCache = nil
        return URLSession(configuration: c, delegate: NoRedirects(), delegateQueue: nil)
    }()

    public init(baseUrl: String, token: String, session: URLSession? = nil, gate: LanGate = .shared) {
        self.baseUrl = baseUrl
        self.token = token
        self.session = session ?? Self.sharedSession
        self.gate = gate
    }

    public func health() async -> Result<Health, ApiFailure> {
        let r: Result<Health, ApiFailure> = await get("/api/v1/health", authorised: false)
        if case let .success(h) = r, h.modelVersion != Self.supportedModelVersion {
            return .failure(.incompatible(modelVersion: h.modelVersion))
        }
        return r
    }

    public func summary(top: Int = 5) async -> Result<Summary, ApiFailure> {
        await get("/api/v1/summary?top=\(top)")
    }

    public func snapshot() async -> Result<Frame, ApiFailure> { await get("/api/v1/snapshot") }
    public func host() async -> Result<HostInfo, ApiFailure> { await get("/api/v1/host") }
    public func alerts() async -> Result<[Alert], ApiFailure> { await get("/api/v1/alerts") }
    public func sensors() async -> Result<[SensorLine], ApiFailure> { await get("/api/v1/sensors") }

    public func history(seconds: Int) async -> Result<[MachineSample], ApiFailure> {
        await get("/api/v1/history?seconds=\(seconds)")
    }

    /// `POST /api/v1/control`: 204 is success; anything else carries a reason.
    public func control(_ request: ControlRequest) async -> Result<Void, ApiFailure> {
        guard gate.isOnLan else { return .failure(.offLan) }
        guard let url = URL(string: baseUrl + "/api/v1/control") else { return .failure(.unreachable) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        do {
            req.httpBody = try JSONEncoder().encode(request)
            let (data, response) = try await session.data(for: req)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            switch status {
            case 204: return .success(())
            case 401: return .failure(.unauthorised)
            default:
                if let f = Self.controlFailure(data) { return .failure(.refused(f)) }
                return .failure(.http(status))
            }
        } catch {
            return .failure(.unreachable)
        }
    }

    static func controlFailure(_ data: Data) -> ControlFailure? {
        struct Wire: Decodable { let kind: String; let message: String? }
        guard let w = try? JSONDecoder().decode(Wire.self, from: data) else { return nil }
        switch w.kind {
        case "forbidden": return .forbidden
        case "access-denied": return .accessDenied
        case "not-found": return .notFound
        case "unsupported": return .unsupported(w.message ?? "")
        case "internal": return .internalError(w.message ?? "")
        default: return nil
        }
    }

    /// Trades a six-digit code for a token. Unauthenticated: the code is the
    /// credential; wrong, expired and burnt codes all answer the same 403.
    public static func redeem(
        baseUrl: String, code: String, label: String, session: URLSession? = nil
    ) async -> Result<PairGrant, ApiFailure> {
        guard let url = URL(string: baseUrl + "/api/v1/pair") else { return .failure(.unreachable) }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        struct Body: Encodable { let code: String; let label: String }
        struct Reply: Decodable { let token: String; let scope: String }
        do {
            req.httpBody = try JSONEncoder().encode(Body(code: code, label: String(label.prefix(64))))
            let (data, response) = try await (session ?? sharedSession).data(for: req)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            guard status == 200 else { return .failure(.http(status)) }
            guard let r = try? JSONDecoder().decode(Reply.self, from: data) else {
                return .failure(.incompatible(modelVersion: -1))
            }
            let scope: Scope = r.scope == "control" ? .control : r.scope == "read" ? .read : .unknown
            return .success(PairGrant(token: r.token, scope: scope))
        } catch {
            return .failure(.unreachable)
        }
    }

    func get<T: Decodable>(_ path: String, authorised: Bool = true) async -> Result<T, ApiFailure> {
        if authorised, !gate.isOnLan { return .failure(.offLan) }
        guard let url = URL(string: baseUrl + path) else { return .failure(.unreachable) }
        var req = URLRequest(url: url)
        if authorised { req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        do {
            let (data, response) = try await session.data(for: req)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            switch status {
            case 200:
                do { return .success(try JSONDecoder.vitals.decode(T.self, from: data)) }
                catch { return .failure(.incompatible(modelVersion: -1)) }
            case 204: return .failure(.notReady)
            // A read refused with 403 is a token the PC no longer honours.
            case 401, 403: return .failure(.unauthorised)
            default: return .failure(.http(status))
            }
        } catch {
            return .failure(.unreachable)
        }
    }

    // MARK: Live frames

    public enum StreamEnd: Error, Equatable, Sendable {
        case offLan, unreadable, closed(Int), failed
    }

    /// Live frames over the WebSocket; the first is always a complete,
    /// current keyframe. The stream finishes with a `StreamEnd` so the caller
    /// can back off and reconnect; cancelling the consumer closes the socket.
    public func frames() -> AsyncThrowingStream<Frame, Error> {
        AsyncThrowingStream { continuation in
            guard gate.isOnLan else {
                continuation.finish(throwing: StreamEnd.offLan)
                return
            }
            let wsBase = baseUrl.hasPrefix("https") ? "wss" + baseUrl.dropFirst(5) : "ws" + baseUrl.dropFirst(4)
            guard let url = URL(string: wsBase + "/api/v1/ws") else {
                continuation.finish(throwing: StreamEnd.failed)
                return
            }
            var req = URLRequest(url: url)
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
            let task = session.webSocketTask(with: req)
            let pinger = Task {
                while !Task.isCancelled {
                    try? await Task.sleep(for: .seconds(20))
                    task.sendPing { _ in }
                }
            }
            let reader = Task {
                do {
                    while !Task.isCancelled {
                        let message = try await task.receive()
                        let data: Data
                        switch message {
                        case let .string(s): data = Data(s.utf8)
                        case let .data(d): data = d
                        @unknown default: continue
                        }
                        // Control replies share the socket and carry no payload.
                        guard data.range(of: Data("\"payload\"".utf8)) != nil else { continue }
                        guard let frame = try? JSONDecoder.vitals.decode(Frame.self, from: data) else {
                            continuation.finish(throwing: StreamEnd.unreadable)
                            return
                        }
                        continuation.yield(frame)
                    }
                } catch {
                    let code = task.closeCode.rawValue
                    continuation.finish(throwing: code == URLSessionWebSocketTask.CloseCode.invalid.rawValue
                        ? StreamEnd.failed : StreamEnd.closed(code))
                }
            }
            continuation.onTermination = { _ in
                reader.cancel()
                pinger.cancel()
                task.cancel(with: .normalClosure, reason: nil)
            }
            task.resume()
        }
    }
}

// MARK: Pairing checks

public enum PairCheck {
    public enum Outcome: Sendable {
        case ok(Pairing, Summary)
        case refused(PairRefusal, status: Int?)
    }

    /// `/health` proves something that speaks Vitals is there (no token);
    /// `/summary` proves the token works. Only then is the pairing written,
    /// so a list never holds a PC that was wrong from the start. Re-pairing
    /// the same PC keeps its id and creation time.
    public static func withToken(
        rawUrl: String, rawToken: String, label: String?, existing: [Pairing],
        scope: Scope = .unknown, gate: LanGate = .shared, session: URLSession? = nil,
        now: Int64 = Int64(Date().timeIntervalSince1970 * 1000)
    ) async -> Outcome {
        guard let baseUrl = PairingRules.normalise(rawUrl) else { return .refused(.badAddress, status: nil) }
        guard LanAddress.isPrivate(baseUrl) else { return .refused(.notPrivate, status: nil) }
        let token = rawToken.trimmingCharacters(in: .whitespacesAndNewlines)
        guard PairingRules.isToken(token) else { return .refused(.badToken, status: nil) }
        let client = VitalsClient(baseUrl: baseUrl, token: token, session: session, gate: gate)
        if case let .failure(f) = await client.health() { return refusal(f) }
        let summary: Summary
        switch await client.summary(top: 1) {
        case let .success(s): summary = s
        case let .failure(f): return refusal(f)
        }
        let hostName: String? = if case let .success(h) = await client.host() { h.hostname } else { nil }
        let trimmed = label?.trimmingCharacters(in: .whitespaces) ?? ""
        let name = !trimmed.isEmpty ? trimmed : hostName ?? URLComponents(string: baseUrl)?.host ?? baseUrl
        let previous = existing.first { $0.baseUrl == baseUrl }
        let pairing = Pairing(
            id: previous?.id ?? Pairing.newId(),
            label: name,
            baseUrl: baseUrl,
            token: token,
            scope: scope,
            mac: wakeMac(summary) ?? previous?.mac,
            createdMs: previous?.createdMs ?? now)
        return .ok(pairing, summary)
    }

    /// Exchanges a six-digit code for a token, then checks it like any other.
    /// The code is checked locally first so a typo never spends one of the
    /// PC's five attempts; it is never redeemed off the LAN.
    public static func withCode(
        rawUrl: String, rawCode: String, deviceLabel: String, pcLabel: String?, existing: [Pairing],
        gate: LanGate = .shared, session: URLSession? = nil
    ) async -> Outcome {
        guard let baseUrl = PairingRules.normalise(rawUrl) else { return .refused(.badAddress, status: nil) }
        guard LanAddress.isPrivate(baseUrl) else { return .refused(.notPrivate, status: nil) }
        let code = PairingRules.cleanCode(rawCode)
        guard code.count == 6 else { return .refused(.badCode, status: nil) }
        guard gate.isOnLan else { return .refused(.offLan, status: nil) }
        switch await VitalsClient.redeem(baseUrl: baseUrl, code: code, label: deviceLabel, session: session) {
        case let .success(grant):
            return await withToken(
                rawUrl: baseUrl, rawToken: grant.token, label: pcLabel, existing: existing,
                scope: grant.scope, gate: gate, session: session)
        case let .failure(f):
            return refusal(f, codeRefused: true)
        }
    }

    static func wakeMac(_ s: Summary) -> String? {
        let c = s.system.networks.filter { $0.connected && $0.mac != nil }
        return (c.first { $0.kind == .ethernet } ?? c.first)?.mac
    }

    static func refusal(_ f: ApiFailure, codeRefused: Bool = false) -> Outcome {
        switch f {
        case .unreachable: .refused(.unreachable, status: nil)
        case .unauthorised, .refused: .refused(.unauthorised, status: nil)
        case .offLan: .refused(.offLan, status: nil)
        case .incompatible: .refused(.incompatible, status: nil)
        case .notReady: .refused(.notReady, status: nil)
        // 403 from /pair is the one answer for wrong, expired and burnt codes;
        // 404 means this PC has no pairing route, which is NOT a bad code.
        case let .http(s): codeRefused && s == 403 ? .refused(.badCode, status: s) : .refused(.failed, status: s)
        }
    }
}
