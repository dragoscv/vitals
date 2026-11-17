// Paired PCs, persisted in the Keychain (this device only, never backed up or
// synced): each record holds a bearer token for a PC on the user's network.

import Foundation
import Observation
import Security
import VitalsCore

@MainActor @Observable
final class PairingStore {
    private(set) var pairings: [Pairing] = []
    private(set) var loaded = false
    /// The last Keychain result. A pairing that silently fails to save is a
    /// paired PC that vanishes on the next launch; surfacing the status turns
    /// that into something the Settings screen can name.
    private(set) var keychainStatus: OSStatus = errSecSuccess
    /// True when the last Keychain operation did not succeed.
    var keychainProblem: Bool { keychainStatus != errSecSuccess }

    private let service = "app.vitals.ios.pairings"
    private let account = "all"

    init() { load() }

    func load() {
        defer { loaded = true }
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
          let status = SecItemCopyMatching(query as CFDictionary, &out)
          // A missing item is the normal first-launch case, not an error.
          if status != errSecItemNotFound { keychainStatus = status }
          guard status == errSecSuccess,
              let data = out as? Data,
              let list = try? JSONDecoder().decode([Pairing].self, from: data) else { return }
        pairings = list
    }

    func save(_ pairing: Pairing) {
        if let i = pairings.firstIndex(where: { $0.id == pairing.id }) {
            pairings[i] = pairing
        } else {
            pairings.append(pairing)
        }
        persist()
    }

    func remove(_ id: String) {
        pairings.removeAll { $0.id == id }
        persist()
    }

    func rename(_ id: String, to label: String) {
        guard let i = pairings.firstIndex(where: { $0.id == id }) else { return }
        let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        pairings[i].label = trimmed
        persist()
    }

    /// The first 403 forbidden on a control request makes the pairing read-only.
    func downgradeToRead(_ id: String) {
        guard let i = pairings.firstIndex(where: { $0.id == id }), pairings[i].scope != .read else { return }
        pairings[i].scope = .read
        persist()
    }

    func pairing(_ id: String) -> Pairing? { pairings.first { $0.id == id } }

    private func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    private func persist() {
        guard let data = try? JSONEncoder().encode(pairings) else {
            keychainStatus = errSecParam
            return
        }
        let query = baseQuery()
        let attrs: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query as CFDictionary, attrs as CFDictionary)
        if status == errSecItemNotFound {
            var add = query
            add.merge(attrs) { $1 }
            keychainStatus = SecItemAdd(add as CFDictionary, nil)
        } else {
            keychainStatus = status
        }
    }
}
