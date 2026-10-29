// The WatchConnectivity delegate both sides share. WCSession calls its
// delegate on a background queue, so this object holds no state of its own:
// it pulls the one Sendable thing out of each callback (the snapshot bytes,
// or the fact that something changed) and hands it to a @MainActor handler.
// The session itself never crosses an actor boundary; the main-actor side
// reads `WCSession.default` again when it needs it.
//
// Not compiled on macOS (WatchConnectivity does not exist there), which keeps
// `swift test` on a Mac without a Simulator working.

#if canImport(WatchConnectivity)
import Foundation
import WatchConnectivity

public final class WatchSessionRelay: NSObject, WCSessionDelegate, Sendable {
    public enum Event: Sendable {
        /// Activation finished, reachability or the paired watch changed.
        case changed
        /// The watch app opened and asks for the newest snapshot now.
        case request
        /// An encoded WatchSnapshot arrived (message or application context).
        case snapshot(Data)
    }

    private let handler: @MainActor @Sendable (Event) -> Void

    public init(_ handler: @escaping @MainActor @Sendable (Event) -> Void) {
        self.handler = handler
    }

    private func post(_ event: Event) {
        let handler = handler
        Task { @MainActor in handler(event) }
    }

    private func route(_ dict: [String: Any]) {
        if let data = dict[WatchSnapshot.payloadKey] as? Data { post(.snapshot(data)) }
        if dict[WatchSnapshot.requestKey] != nil { post(.request) }
    }

    public func session(
        _ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState,
        error: (any Error)?
    ) {
        post(.changed)
    }

    public func sessionReachabilityDidChange(_ session: WCSession) { post(.changed) }

    public func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        route(message)
    }

    public func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        route(applicationContext)
    }

    #if os(iOS)
    public func sessionDidBecomeInactive(_ session: WCSession) {}

    /// Switching to another watch deactivates the session; Apple's guidance is
    /// to activate again so the new watch is served.
    public func sessionDidDeactivate(_ session: WCSession) { session.activate() }

    public func sessionWatchStateDidChange(_ session: WCSession) { post(.changed) }
    #endif
}
#endif
