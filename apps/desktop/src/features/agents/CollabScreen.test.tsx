/**
 * The collaboration page renders the three sections from agentq's state and
 * re-reads it on a timer, read-only throughout.
 */

import { act, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CollabSnapshot } from '@vitals/protocol';
import { i18n, initI18n } from '@vitals/i18n';

import { CollabScreen } from './CollabScreen';
import {
  makeCollab,
  makeCollabAgent,
  makeCollabJob,
  makeCollabThread,
} from './fixtures.test-utils';
import { registerAgentsStrings } from './strings';
import type { CollabSource } from './useCollab';

beforeAll(async () => {
  await initI18n();
  registerAgentsStrings();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

/** An injectable source whose answer can be swapped mid-test. */
function source(first: CollabSnapshot): CollabSource & { set: (s: CollabSnapshot) => void } {
  let current = first;
  return {
    fetch: vi.fn(async () => current),
    set: (s) => {
      current = s;
    },
  };
}

describe('CollabScreen', () => {
  it('renders the three live sections from a single snapshot', async () => {
    const src = source(makeCollab());
    render(<CollabScreen source={src} />);
    expect(await screen.findByText('Agents present')).toBeTruthy();
    expect(screen.getByText('Conversations')).toBeTruthy();
    expect(screen.getByText('Work in progress')).toBeTruthy();
    // Every count line, with the variable it interpolates: these are the
    // strings that render an unsubstituted placeholder if the call site and
    // the key disagree about the variable's name (found live, 2026-11-25).
    expect(screen.getByText('1 agent')).toBeTruthy();
    expect(screen.getByText('1 present · a call in the last 30 min')).toBeTruthy();
    expect(screen.getByText('1 job')).toBeTruthy();
    expect(screen.getByText('1 running')).toBeTruthy();
    expect(screen.getByText('term-32064')).toBeTruthy();
    expect(screen.getByText(/May I take build:vitals/)).toBeTruthy();
    expect(screen.getByText('land-ownframes')).toBeTruthy();
  });

  it('shows each answer under its thread with the author who wrote it', async () => {
    const src = source(
      makeCollab({
        threads: [
          makeCollabThread({
            state: 'answered',
            answers: [
              {
                session: 'term-9',
                repo: 'agentq',
                user: 'vladu',
                message: 'yes, I released it a minute ago',
                atMs: Date.parse('2026-10-17T03:29:00Z'),
              },
            ],
          }),
        ],
      }),
    );
    render(<CollabScreen source={src} />);
    expect(await screen.findByText('Answered')).toBeTruthy();
    expect(screen.getByText(/yes, I released it a minute ago/)).toBeTruthy();
  });

  it('an agent whose repository and model are not recorded shows an em dash rather than a blank', async () => {
    const src = source(
      makeCollab({
        agents: [makeCollabAgent({ repo: null, model: null, lastPurpose: null, lastVerb: null })],
      }),
    );
    render(<CollabScreen source={src} />);
    // The repo/model line reads `client · — · — ...`: the unknown fields are
    // dashes, never empty strings that read as a value.
    expect(await screen.findByText(/—/)).toBeTruthy();
  });

  it('re-reads on a timer so a thread sent mid-session appears without a reload', async () => {
    vi.useFakeTimers();
    try {
      const src = source(makeCollab({ threads: [makeCollabThread()] }));
      render(<CollabScreen source={src} />);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(screen.getByText(/May I take build:vitals/)).toBeTruthy();

      src.set(
        makeCollab({
          threads: [
            makeCollabThread({
              id: 'bbbbbbbb',
              message: 'deploying now, please hold build:machine',
            }),
          ],
        }),
      );
      await act(async () => {
        await vi.advanceTimersByTimeAsync(4_100);
      });
      expect(screen.getByText(/deploying now, please hold build:machine/)).toBeTruthy();
      expect(src.fetch).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('lists finished work beside running work so the section says what is in flight', async () => {
    const src = source(
      makeCollab({
        jobs: [
          makeCollabJob({ id: 'run', state: 'running', label: 'run-tests' }),
          makeCollabJob({ id: 'done', state: 'done', label: 'bake-image', exitCode: 0 }),
        ],
      }),
    );
    render(<CollabScreen source={src} />);
    expect(await screen.findByText('run-tests')).toBeTruthy();
    expect(screen.getByText('bake-image')).toBeTruthy();
  });
});
