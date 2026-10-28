// `GET /api/v1/agents` (ADR-0039), as far as the phone's card reads it.
// Deliberately partial and lenient, like Android's Agents.kt: the PC serves an
// allowlist that can drop free-text fields, so every field the card does not
// need is optional and a missing one hides a line, never fails the answer.

import Foundation

public struct AgentsSnapshot: Decodable, Sendable {
    public let takenAtMs: Int64
    public let agentqInstalled: Bool
    public let resources: [AgentqResource]
    public let sessions: [AgentSession]

    enum Keys: String, CodingKey { case takenAtMs, agentqInstalled, resources, sessions }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        takenAtMs = (try? c.decode(Int64.self, forKey: .takenAtMs)) ?? 0
        agentqInstalled = (try? c.decode(Bool.self, forKey: .agentqInstalled)) ?? false
        resources = (try? c.decode([AgentqResource].self, forKey: .resources)) ?? []
        sessions = (try? c.decode([AgentSession].self, forKey: .sessions)) ?? []
    }

    /// Sessions doing something, busiest first; at most four on the card.
    public var busySessions: [AgentSession] {
        Array(sessions.filter { $0.state == .working || $0.state == .waiting }.prefix(4))
    }

    public var busyQueues: [AgentqResource] { resources.filter { $0.waiting > 0 } }
}

public struct AgentqResource: Decodable, Sendable, Identifiable {
    public let name: String
    public let tickets: [AgentqTicket]
    public var id: String { name }
    public var holding: Int { tickets.filter(\.held).count }
    public var waiting: Int { tickets.filter { !$0.held }.count }

    enum Keys: String, CodingKey { case name, tickets }
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        name = try c.decode(String.self, forKey: .name)
        tickets = (try? c.decode([AgentqTicket].self, forKey: .tickets)) ?? []
    }
}

public struct AgentqTicket: Decodable, Sendable {
    public let held: Bool
}

public enum AgentClient: String, TolerantEnum {
    case vscodeCopilot, copilotCli, codai, codex, claudeCode, other, unknown
    public static let unknownCase = Self.unknown
}

public enum SessionState: String, TolerantEnum {
    case working, waiting, idle, ended, unknown
    public static let unknownCase = Self.unknown
}

public struct AgentSession: Decodable, Sendable, Identifiable {
    public let id: String
    public let client: AgentClient
    public let repo: String?
    public let state: SessionState
    public let currentTool: String?

    enum Keys: String, CodingKey { case id, client, repo, state, currentTool }
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        id = (try? c.decode(String.self, forKey: .id)) ?? UUID().uuidString
        client = (try? c.decode(AgentClient.self, forKey: .client)) ?? .unknown
        repo = try? c.decode(String.self, forKey: .repo)
        state = (try? c.decode(SessionState.self, forKey: .state)) ?? .unknown
        currentTool = try? c.decode(String.self, forKey: .currentTool)
    }
}

extension VitalsClient {
    /// 404 means the PC offers no AI page; the card hides, it is not an error.
    public func agents() async -> Result<AgentsSnapshot, ApiFailure> { await get("/api/v1/agents") }
}
