// Is this phone on a LAN (Wi-Fi or wired)? Authenticated calls are refused
// otherwise, so a pairing token never travels over mobile data — not even
// through a VPN running on top of it. Plus Bonjour discovery of `_vitals._tcp`.

import Foundation
import Network
import Observation
import VitalsCore

@MainActor @Observable
final class LanMonitor {
    private(set) var onLan = true
    private let monitor = NWPathMonitor()

    init() {
        monitor.pathUpdateHandler = { [weak self] path in
            let lan = path.status == .satisfied
                && (path.usesInterfaceType(.wifi) || path.usesInterfaceType(.wiredEthernet))
                && !path.usesInterfaceType(.cellular)
            LanGate.shared.isOnLan = lan
            Task { @MainActor in self?.onLan = lan }
        }
        monitor.start(queue: DispatchQueue(label: "app.vitals.lan"))
    }
}

/// A PC advertising the LAN server, resolved to a private address.
struct NearbyPC: Identifiable, Hashable, Sendable {
    let name: String
    let baseUrl: String
    var id: String { baseUrl }
}

@MainActor @Observable
final class Discovery {
    private(set) var found: [NearbyPC] = []
    private(set) var running = false
    private var browser: NWBrowser?
    private var resolving: [String: NWConnection] = [:]

    func start() {
        guard browser == nil else { return }
        found = []
        let b = NWBrowser(for: .bonjour(type: "_vitals._tcp", domain: nil), using: .tcp)
        b.browseResultsChangedHandler = { [weak self] results, _ in
            Task { @MainActor in self?.handle(results) }
        }
        b.start(queue: .main)
        browser = b
        running = true
    }

    func stop() {
        browser?.cancel()
        browser = nil
        resolving.values.forEach { $0.cancel() }
        resolving = [:]
        running = false
    }

    private func handle(_ results: Set<NWBrowser.Result>) {
        for r in results {
            guard case let .service(name, _, _, _) = r.endpoint, resolving[name] == nil,
                  !found.contains(where: { $0.name == name }) else { continue }
            // Resolve by connecting briefly and reading the remote address.
            let c = NWConnection(to: r.endpoint, using: .tcp)
            resolving[name] = c
            c.stateUpdateHandler = { [weak self] state in
                guard case .ready = state else {
                    if case .failed = state { c.cancel() }
                    return
                }
                let endpoint = c.currentPath?.remoteEndpoint
                c.cancel()
                Task { @MainActor in self?.resolved(name: name, endpoint: endpoint) }
            }
            c.start(queue: .main)
        }
    }

    private func resolved(name: String, endpoint: NWEndpoint?) {
        resolving[name] = nil
        guard case let .hostPort(host, port) = endpoint else { return }
        var h: String
        switch host {
        case let .ipv4(a): h = "\(a)"
        case let .ipv6(a): h = "[\("\(a)".split(separator: "%").first ?? "")]"
        case let .name(n, _): h = n
        @unknown default: return
        }
        h = h.split(separator: "%").first.map(String.init) ?? h
        let url = "http://\(h):\(port.rawValue)"
        guard LanAddress.isPrivate(url) else { return }
        if !found.contains(where: { $0.baseUrl == url }) {
            found.append(NearbyPC(name: name, baseUrl: url))
        }
    }
}
