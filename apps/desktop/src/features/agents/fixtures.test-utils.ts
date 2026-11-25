import type {
  AgentSession,
  AgentUsage,
  AgentUsageKind,
  AgentUsageSnapshot,
  AgentqTicket,
  AgentqWorkEstimate,
  AgentqWorkUsage,
  AgentsSnapshot,
} from '@vitals/protocol';

export const NOW = Date.parse('2026-10-17T03:30:00Z');

/**
 * One tool's resource row.
 *
 * Network and gpu default to `null` because that is what the backend sends on
 * Windows, and a fixture that quietly filled them in would let a test pass
 * while the shipped panel printed `0 B/s`.
 */
export function makeUsage(
  kind: AgentUsageKind = 'agentq',
  over: Partial<AgentUsage> = {},
): AgentUsage {
  return {
    kind,
    processes: 3,
    cpu: 42.5,
    memoryPrivate: 512 * 1024 * 1024,
    memoryWorkingSet: 640 * 1024 * 1024,
    diskRead: 1_048_576,
    diskWrite: 2_097_152,
    netRx: null,
    netTx: null,
    gpu: null,
    gpuMemory: null,
    ...over,
  };
}

export function makeUsageSnapshot(over: Partial<AgentUsageSnapshot> = {}): AgentUsageSnapshot {
  return {
    takenAtMs: NOW - 1_000,
    agentq: makeUsage('agentq'),
    clients: [makeUsage('claude', { cpu: 10 }), makeUsage('vscodeCopilot', { cpu: 5 })],
    // Empty by default: a snapshot with no work row is the common case, and a
    // fixture that invented one would make every panel test draw it.
    work: [],
    ...over,
  };
}

/**
 * One piece of agentq's executing work.
 *
 * `usage.kind` is always `agentq`, exactly as the backend sends it — the
 * work's own kind is the separate `kind` field, and a fixture that set them
 * the same would hide a panel that read the wrong one.
 */
export function makeWork(over: Partial<AgentqWorkUsage> = {}): AgentqWorkUsage {
  return {
    label: 'build:vitals',
    kind: 'build',
    resource: 'build:vitals',
    repo: 'E:\\gh\\vitals',
    purpose: 'why it was queued',
    command: 'pnpm build',
    session: 'term-1',
    pid: 4242,
    startedMs: NOW - 120_000,
    // No estimate by default: most work on a real machine has no history yet,
    // and a fixture that invented one would draw a bar in every panel test.
    estimate: null,
    usage: makeUsage('agentq', { processes: 2, cpu: 31.5 }),
    ...over,
  };
}

/** A measured or declared duration for [`makeWork`]. */
export function makeEstimate(over: Partial<AgentqWorkEstimate> = {}): AgentqWorkEstimate {
  return { expectedMs: 300_000, basis: 'measured', samples: 7, ...over };
}

export function makeSession(over: Partial<AgentSession> = {}): AgentSession {
  return {
    id: 's1',
    client: 'vscodeCopilot',
    host: 'local',
    title: 'Fix the queue',
    workspace: 'e:\\gh\\vitals',
    repo: 'vitals',
    branch: 'dev',
    model: 'Codwen',
    state: 'working',
    startedMs: NOW - 3_600_000,
    lastActivityMs: NOW - 10_000,
    activeMs: 900_000,
    turns: 12,
    userMessages: 3,
    toolCalls: 40,
    toolFailures: 1,
    currentTool: 'run_in_terminal',
    topTools: [{ name: 'read_file', count: 20 }],
    tokensIn: 1200,
    tokensOut: 340,
    tokensCached: 800,
    tokensReasoning: 64,
    costUsd: 0.0123,
    requests: 2,
    outputMs: 4000,
    ttftMs: 900,
    tokensPerSec: 85,
    pid: 4242,
    ...over,
  };
}

export function makeTicket(over: Partial<AgentqTicket> = {}): AgentqTicket {
  return {
    id: 't1',
    resource: 'build:vitals',
    seq: 1,
    held: true,
    lane: 'gate',
    priority: 0,
    pid: 100,
    session: 'term-1',
    purpose: 'cargo clippy',
    cmd: 'cargo clippy',
    repo: 'vitals',
    createdMs: NOW - 120_000,
    acquiredMs: NOW - 60_000,
    beatMs: NOW - 5_000,
    liveness: 'alive',
    yielded: false,
    wedged: false,
    ...over,
  };
}

export function makeSnapshot(over: Partial<AgentsSnapshot> = {}): AgentsSnapshot {
  return {
    takenAtMs: NOW - 1_000,
    agentqInstalled: true,
    resources: [
      {
        name: 'build:machine',
        kind: 'machine',
        capacity: 2,
        tickets: [makeTicket({ id: 'm1', resource: 'build:machine' })],
      },
      {
        name: 'build:vitals',
        kind: 'build',
        capacity: 1,
        tickets: [
          makeTicket(),
          makeTicket({ id: 't2', held: false, liveness: 'dead', purpose: 'vitest' }),
        ],
      },
    ],
    leases: [
      {
        leaseId: 'l1',
        repo: 'vitals',
        slot: 'task-1',
        role: 'task',
        path: 'E:\\gh\\.wt\\vitals\\task-1',
        branch: 'feat/ai',
        purpose: 'AI page',
        session: 'term-1',
        pid: 100,
        createdMs: NOW - 600_000,
        beatMs: NOW - 30_000,
        ownerAlive: true,
        stale: false,
      },
    ],
    releases: [],
    jobs: [],
    marks: [],
    registry: [
      {
        session: 'term-1',
        client: 'vscode-copilot',
        app: 'Code - Insiders',
        appPid: 4242,
        shellPid: 99,
        model: null,
        repo: 'vitals',
        cwd: 'E:\\gh\\vitals',
        lastVerb: 'run',
        lastPurpose: 'cargo clippy',
        calls: 3,
        firstSeenMs: NOW - 600_000,
        lastSeenMs: NOW - 5_000,
      },
    ],
    cloudDown: [],
    machine: null,
    journal: [],
    stats: {
      windowHours: 24,
      runs: 0,
      failures: 0,
      commits: 0,
      cloudRuns: 0,
      waitP50Sec: null,
      waitP90Sec: null,
      holdP50Sec: null,
      holdP90Sec: null,
      waitTotalSec: 0,
      holdTotalSec: 0,
      byRepo: [],
      runsPerHour: Array.from({ length: 24 }, () => 0),
    },
    sessions: [makeSession()],
    ...over,
  };
}
