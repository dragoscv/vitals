import type { AgentSession, AgentqTicket, AgentsSnapshot } from '@vitals/protocol';

export const NOW = Date.parse('2026-10-17T03:30:00Z');

export function makeSession(over: Partial<AgentSession> = {}): AgentSession {
  return {
    id: 's1',
    client: 'vscodeCopilot',
    host: 'local',
    title: 'Fix the queue',
    workspace: 'e:\\gh\\vitals',
    repo: 'vitals',
    branch: 'dev',
    model: 'codai',
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
    tokensIn: null,
    tokensOut: null,
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
