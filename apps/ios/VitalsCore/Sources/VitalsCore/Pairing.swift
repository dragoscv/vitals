// Pairing: what a paired PC is, how an address and a QR code are checked,
// and why a pairing was refused. Mirrors Android core/pairing + net/LanAddress
// so the phones cannot disagree about what a valid pairing is.

import Foundation

public enum Scope: String, Codable, Sendable {
    /// No route reports a token's scope; a pairing starts here and becomes
    /// `.read` on the first 403 forbidden. Only `.read` hides controls.
    case unknown, read, control
}

public struct Pairing: Codable, Sendable, Identifiable, Hashable {
    public var id: String
    public var label: String
    public var baseUrl: String
    public var token: String
    public var scope: Scope
    public var mac: String?
    public var createdMs: Int64

    public init(
        id: String, label: String, baseUrl: String, token: String,
        scope: Scope = .unknown, mac: String? = nil, createdMs: Int64
    ) {
        self.id = id
        self.label = label
        self.baseUrl = baseUrl
        self.token = token
        self.scope = scope
        self.mac = mac
        self.createdMs = createdMs
    }

    /// 9 random bytes, base64url — same shape as Android's ids.
    public static func newId() -> String {
        var bytes = [UInt8](repeating: 0, count: 9)
        for i in bytes.indices { bytes[i] = UInt8.random(in: 0...255) }
        return Data(bytes).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

/// Why a PC was not saved. The app words each one.
public enum PairRefusal: String, Sendable, Equatable {
    case badAddress, notPrivate, badToken, badCode, unreachable, offLan, unauthorised
    case incompatible, notReady, failed
}

public enum PairingRules {
    /// 32 random bytes, base64url without padding: always 43 characters.
    public static func isToken(_ s: String) -> Bool {
        s.count == 43 && s.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") }
    }

    /// Digits only, as typed: spaces and the 3+3 grouping the desktop shows are ignored.
    public static func cleanCode(_ raw: String) -> String {
        String(raw.filter { $0.isASCII && $0.isNumber })
    }

    public static func isCode(_ raw: String) -> Bool { cleanCode(raw).count == 6 }

    /// `host[:port]` or a URL → `scheme://host:port` (default port 7331).
    public static func normalise(_ raw: String) -> String? {
        var text = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        while text.hasSuffix("/") { text.removeLast() }
        if text.isEmpty { return nil }
        let withScheme = (text.hasPrefix("http://") || text.hasPrefix("https://")) ? text : "http://\(text)"
        guard let c = URLComponents(string: withScheme), let scheme = c.scheme,
              let host = c.host, !host.isEmpty else { return nil }
        let port = c.port ?? 7331
        let hostPart = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
        return "\(scheme)://\(hostPart):\(port)"
    }
}

/// A PC on a private network only. The LAN API is plain HTTP by design, so
/// the app refuses to point a bearer token at a public address: a QR code
/// aimed at the internet is refused rather than obeyed. Only `localhost` and
/// mDNS `.local` names are accepted; any other name is refused even if it
/// resolves privately — the name's owner decides what it resolves to.
public enum LanAddress {
    public static func isPrivate(_ baseUrl: String) -> Bool {
        guard var host = URLComponents(string: baseUrl)?.host else { return false }
        host = host.trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        let lower = host.lowercased()
        if lower.hasSuffix(".local") || lower == "localhost" { return true }
        return isPrivateIPv4(host) || isPrivateIPv6(host)
    }

    static func isPrivateIPv4(_ host: String) -> Bool {
        let parts = host.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return false }
        var p: [Int] = []
        for part in parts {
            guard !part.isEmpty, part.count <= 3, part.allSatisfy({ $0.isASCII && $0.isNumber }),
                  let n = Int(part), n <= 255 else { return false }
            p.append(n)
        }
        return p[0] == 10
            || (p[0] == 172 && (16...31).contains(p[1]))
            || (p[0] == 192 && p[1] == 168)
            || (p[0] == 169 && p[1] == 254)
            || p[0] == 127
            || (p[0] == 100 && (64...127).contains(p[1]))  // CGNAT / Tailscale
    }

    static func isPrivateIPv6(_ host: String) -> Bool {
        let h = host.lowercased().split(separator: "%", maxSplits: 1).first.map(String.init) ?? ""
        guard h.contains(":"),
              h.allSatisfy({ ("0"..."9").contains($0) || ("a"..."f").contains($0) || $0 == ":" || $0 == "." })
        else { return false }
        if h == "::1" { return true }
        let first = h.split(separator: ":", omittingEmptySubsequences: false).first ?? ""
        guard !first.isEmpty, first.count <= 4, let n = Int(first, radix: 16) else { return false }
        return (n & 0xffc0) == 0xfe80 || (n & 0xfe00) == 0xfc00
    }
}

/// What the desktop's pairing QR encodes: `http://<ip>:<port>/mobile.html#t=<token>`.
/// The token rides in the fragment so a browser never sends it to the server.
public struct PairingLink: Equatable, Sendable {
    public let baseUrl: String
    public let token: String

    public static func parse(_ text: String) -> PairingLink? {
        guard let c = URLComponents(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = c.scheme, scheme == "http" || scheme == "https",
              let host = c.host, !host.isEmpty,
              let fragment = c.percentEncodedFragment else { return nil }
        var token: String?
        for part in fragment.split(separator: "&") {
            let kv = part.split(separator: "=", maxSplits: 1)
            if kv.count == 2, kv[0] == "t" { token = String(kv[1]); break }
        }
        guard let token, PairingRules.isToken(token) else { return nil }
        let port = c.port ?? 7331
        let hostPart = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
        return PairingLink(baseUrl: "\(scheme)://\(hostPart):\(port)", token: token)
    }
}

/// Reconnect delays: 500 ms doubling to 30 s with ±20 % jitter, so phones
/// woken by the same Wi-Fi reconnect do not stampede the PC (same curve as
/// packages/client and Android).
public struct Backoff: Sendable {
    let initialMs: Double
    let maxMs: Double
    public private(set) var failures = 0

    public init(initialMs: Double = 500, maxMs: Double = 30_000) {
        self.initialMs = initialMs
        self.maxMs = maxMs
    }

    public mutating func next<G: RandomNumberGenerator>(using rng: inout G) -> Double {
        let base = min(initialMs * pow(2, Double(min(failures, 16))), maxMs)
        failures += 1
        let jitter = base * 0.2
        return max(0, base + Double.random(in: -jitter...jitter, using: &rng))
    }

    public mutating func next() -> Double {
        var g = SystemRandomNumberGenerator()
        return next(using: &g)
    }

    public mutating func reset() { failures = 0 }
}
