import { describe, expect, it } from 'vitest';

import { bundles } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('agents translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((p) => p.replace(/_(one|few|many|other)$/, '')))].sort();
    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every client, session state and liveness the backend can emit', () => {
    const paths = new Set(keyPaths(bundles.en));
    for (const c of ['vscodeCopilot', 'copilotCli', 'codwen', 'codex', 'claudeCode', 'other']) {
      expect(paths).toContain(`client.${c}`);
    }
    for (const s of ['working', 'waiting', 'idle', 'ended']) expect(paths).toContain(`state.${s}`);
    for (const l of ['alive', 'stale', 'dead', 'unknown']) expect(paths).toContain(`liveness.${l}`);
  });

  it('names every resource row the backend can emit', () => {
    const paths = new Set(keyPaths(bundles.en));
    // One label per `AgentUsageKind` variant, in the order the Rust enum
    // declares them: an unlabelled kind renders its own key to the user.
    for (const k of [
      'agentq',
      'claude',
      'copilotCli',
      'vscodeCopilot',
      'codex',
      'hermes',
      'openClaw',
      'codwen',
      'cli',
    ]) {
      expect(paths).toContain(`usage.kind.${k}`);
    }
  });
});
