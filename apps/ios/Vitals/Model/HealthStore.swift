// The owner's health from vmui (docs/api/vmui-health.md). The token lives in
// the Keychain only (this device, never backed up or synced); the base URL is
// not secret and sits in UserDefaults. The health answer itself is kept in
// memory only: never written to disk, logs or caches.

import Foundation
import Observation
import Security
import VitalsCore

enum HealthIssue: Equatable {
    case signedOut, notOwner, unreachable, invalid
}

enum HealthPairing: Equatable {
    case idle
    case starting
    case waiting(code: String)
    case signingIn
    case failed(HealthPairFailure)
}

enum HealthPairFailure: Equatable {
    case badAddress, unreachable, rejected, expired, timedOut, wrongCredentials, slowDown, invalid
}

@MainActor @Observable
final class HealthStore {
    private(set) var baseUrl: String
    private(set) var token: String?
    private(set) var latest: MeHealth?
    private(set) var issue: HealthIssue?
    private(set) var loading = false
    private(set) var pairing: HealthPairing = .idle

    var isConnected: Bool { token != nil }
    /// The first 8 characters, the only part of a token ever shown.
    var tokenPrefix: String? { token.map { String($0.prefix(8)) } }

    @ObservationIgnored private let client = HealthClient()
    @ObservationIgnored private var poller: Task<Void, Never>?
    @ObservationIgnored private var pairTask: Task<Void, Never>?

    private static let service = "app.vitals.health"
    private static let account = "token"
    private static let baseKey = "healthBaseUrl"

    init() {
        baseUrl = UserDefaults.standard.string(forKey: Self.baseKey) ?? ""
        token = Self.readToken()
    }

    // MARK: Polling

    /// Every 60 s while the scene is active; stopped when it is not.
    func setActive(_ active: Bool) {
        if active, isConnected {
            guard poller == nil else { return }
            poller = Task { [weak self] in
                while !Task.isCancelled {
                    guard let self else { return }
                    await self.refresh()
                    try? await Task.sleep(for: .seconds(60))
                }
            }
        } else {
            poller?.cancel()
            poller = nil
        }
    }

    func refresh() async {
        guard let token else { return }
        loading = true
        defer { loading = false }
        let result = await client.fetch(base: baseUrl, token: token)
        // Disconnected, re-paired or hidden while the request was in flight:
        // a cancelled request is not "unreachable".
        guard self.token == token, !Task.isCancelled else { return }
        switch result {
        case let .success(h):
            latest = h
            issue = nil
        case let .failure(f):
            switch f {
            case .signedOut:
                // Keep the base URL; the token is dead.
                issue = .signedOut
                latest = nil
            case .notOwner:
                issue = .notOwner
                latest = nil
            case .invalid:
                issue = .invalid
            // Keep the last answer in memory and show it as stale.
            case .unreachable, .badAddress, .wrongCredentials, .slowDown:
                issue = .unreachable
            }
        }
    }

    // MARK: Pairing

    func setBaseUrl(_ raw: String) {
        baseUrl = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        UserDefaults.standard.set(baseUrl, forKey: Self.baseKey)
    }

    /// Code pairing: shows the 4-digit code, then asks every 3 s for up to
    /// 10 minutes whether the owner approved it in vmui.
    func pairWithCode(rawBase: String, deviceName: String) {
        guard let base = HealthClient.normalise(rawBase) else {
            pairing = .failed(.badAddress)
            return
        }
        setBaseUrl(base)
        pairTask?.cancel()
        pairing = .starting
        pairTask = Task { [weak self, client] in
            let started = await client.pairStart(base: base, name: deviceName)
            guard !Task.isCancelled, let self else { return }
            let ticket: HealthPairTicket
            switch started {
            case let .success(t): ticket = t
            case let .failure(f):
                self.pairing = .failed(Self.pairFailure(f))
                return
            }
            self.pairing = .waiting(code: ticket.code)
            let deadline = ContinuousClock.now + .seconds(600)
            var delay = Duration.seconds(3)
            while ContinuousClock.now < deadline {
                try? await Task.sleep(for: delay)
                if Task.isCancelled { return }
                switch await client.pairPoll(base: base, id: ticket.id) {
                case .success(.approved):
                    self.connect(token: ticket.token)
                    return
                case .success(.rejected):
                    self.pairing = .failed(.rejected)
                    return
                case .success(.expired):
                    self.pairing = .failed(.expired)
                    return
                case .success(.pending):
                    delay = .seconds(3)
                case .failure(.slowDown):
                    delay = .seconds(10)
                case .failure:
                    // A dropped poll is not the end of the attempt.
                    delay = .seconds(3)
                }
            }
            if !Task.isCancelled { self.pairing = .failed(.timedOut) }
        }
    }

    /// Instant pairing with the vmui account. The password is passed through
    /// once and never kept.
    func signIn(rawBase: String, email: String, password: String, deviceName: String) {
        guard let base = HealthClient.normalise(rawBase) else {
            pairing = .failed(.badAddress)
            return
        }
        setBaseUrl(base)
        pairTask?.cancel()
        pairing = .signingIn
        let mail = email.trimmingCharacters(in: .whitespacesAndNewlines)
        pairTask = Task { [weak self, client] in
            let r = await client.signIn(base: base, name: deviceName, email: mail, password: password)
            guard !Task.isCancelled, let self else { return }
            switch r {
            case let .success(token): self.connect(token: token)
            case let .failure(f): self.pairing = .failed(Self.pairFailure(f))
            }
        }
    }

    func cancelPairing() {
        pairTask?.cancel()
        pairTask = nil
        pairing = .idle
    }

    func disconnect() {
        cancelPairing()
        poller?.cancel()
        poller = nil
        Self.deleteToken()
        token = nil
        latest = nil
        issue = nil
    }

    private func connect(token newToken: String) {
        guard Self.writeToken(newToken) else {
            pairing = .failed(.invalid)
            return
        }
        token = newToken
        latest = nil
        issue = nil
        pairing = .idle
        pairTask = nil
        poller?.cancel()
        poller = nil
        // Polling starts when the Health tab is shown (RootView).
    }

    private static func pairFailure(_ f: HealthFailure) -> HealthPairFailure {
        switch f {
        case .badAddress: .badAddress
        case .wrongCredentials: .wrongCredentials
        case .slowDown: .slowDown
        case .invalid: .invalid
        case .signedOut, .notOwner, .unreachable: .unreachable
        }
    }

    // MARK: Keychain

    private static func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private static func readToken() -> String? {
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess,
              let data = out as? Data,
              let s = String(data: data, encoding: .utf8), !s.isEmpty else { return nil }
        return s
    }

    private static func writeToken(_ token: String) -> Bool {
        let query = baseQuery()
        let attrs: [String: Any] = [
            kSecValueData as String: Data(token.utf8),
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var add = query
            add.merge(attrs) { $1 }
            return SecItemAdd(add as CFDictionary, nil) == errSecSuccess
        }
        return status == errSecSuccess
    }

    private static func deleteToken() {
        SecItemDelete(baseQuery() as CFDictionary)
    }
}
