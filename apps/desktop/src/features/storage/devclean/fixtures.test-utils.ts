/** Shared scan fixtures for the developer clean-up tests. */

import type { Artefact, DevScan, Project } from './model';

export const GB = 1024 ** 3;

export function artefact(overrides: Partial<Artefact> & { readonly id: string }): Artefact {
  return {
    path: `E:\\gh\\${overrides.id}`,
    kind: 'nodeModules',
    size: GB,
    files: 1000,
    sharedWithPnpmStore: false,
    restore: 'pnpm install',
    preselected: false,
    ...overrides,
  };
}

export function project(overrides: Partial<Project> & { readonly name: string }): Project {
  return {
    path: `E:\\gh\\${overrides.name}`,
    isGit: true,
    idleDays: 3,
    artefacts: [],
    ...overrides,
  };
}

export function devScan(overrides: Partial<DevScan> = {}): DevScan {
  return {
    scanId: 7,
    roots: ['E:\\gh'],
    projects: [
      project({
        name: 'memorai',
        idleDays: 376,
        artefacts: [
          artefact({
            id: 'a-stale-nm',
            path: 'E:\\gh\\memorai\\node_modules',
            size: 4 * GB,
            preselected: true,
            sharedWithPnpmStore: true,
          }),
          artefact({
            id: 'a-stale-target',
            path: 'E:\\gh\\memorai\\target',
            kind: 'cargoTarget',
            size: null,
            restore: 'cargo build',
            preselected: true,
          }),
        ],
      }),
      project({
        name: 'codai',
        idleDays: 0,
        artefacts: [
          artefact({
            id: 'a-active-next',
            path: 'E:\\gh\\codai\\.next',
            kind: 'next',
            size: 2 * GB,
            restore: 'pnpm build',
          }),
        ],
      }),
    ],
    worktrees: [
      {
        id: 'w-removable',
        path: 'E:\\gh\\.wt\\codai\\old',
        repo: 'codai',
        branch: 'old',
        size: GB,
        state: 'removable',
        changes: 0,
        idleHours: 400,
      },
      {
        id: null,
        path: 'E:\\gh\\.wt\\brivio\\busy',
        repo: 'brivio',
        branch: 'busy',
        size: GB,
        state: 'dirty',
        changes: 408,
        idleHours: 2,
      },
      {
        id: null,
        path: 'E:\\gh\\.wt\\codai\\live',
        repo: 'codai',
        branch: 'live',
        size: GB,
        state: 'active',
        changes: 0,
        idleHours: 3,
      },
    ],
    caches: [
      {
        id: 'c-pnpm',
        kind: 'pnpm',
        path: 'C:\\Users\\me\\AppData\\Local\\pnpm\\store',
        size: 25 * GB,
        method: 'command',
        command: 'pnpm store prune',
        restore: 'pnpm install',
      },
      {
        id: null,
        kind: 'go',
        path: 'C:\\Users\\me\\go\\pkg\\mod',
        size: null,
        method: 'command',
        command: 'go clean -modcache',
        restore: 'go mod download',
      },
    ],
    docker: {
      state: 'ok',
      items: [
        {
          id: 'd-build',
          kind: 'buildCache',
          count: 12,
          reclaimable: 16 * GB,
          command: 'docker builder prune -f',
        },
        {
          id: null,
          kind: 'volumes',
          count: 9,
          reclaimable: 47 * GB,
          command: 'docker volume prune',
        },
      ],
    },
    vdisks: [
      {
        id: 'v-docker',
        path: 'C:\\Users\\me\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx',
        kind: 'docker',
        distro: null,
        size: 186 * GB,
      },
    ],
    elapsedMs: 42_000,
    ...overrides,
  };
}
