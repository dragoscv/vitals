/**
 * The Tracking screen, rendered.
 *
 * The model has its own tests; this file is about what the screen does with
 * its answer — which of the four states it draws, and what a person actually
 * reads on each. The seam is the screen's own `source` prop, so nothing here
 * needs a Tauri host: the only thing mocked is `hasTauriHost`, which is what
 * tells the hook whether a host exists at all.
 *
 * Assertions are made against the translation keys rather than English prose,
 * so a wording change does not fail a test about behaviour.
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { MotionRuntime } from '../../theme/MotionRuntime';
import { parseIndex, type TrackerIndex } from './model';
import { registerTrackingStrings, TRACKING_NS } from './strings';
import { TrackingScreen } from './TrackingScreen';
import type { TrackingSource } from './useTracking';

const host = vi.hoisted(() => ({ available: true }));
vi.mock('../../shell/host', () => ({ hasTauriHost: () => host.available }));

beforeAll(async () => {
  await initI18n();
  registerTrackingStrings();
});

afterEach(() => {
  host.available = true;
});

function t(key: string, options: Record<string, unknown> = {}): string {
  return String(i18n.t(key, { ns: TRACKING_NS, ...options }));
}

const HEAD = 'abcdef0123456789abcdef0123456789abcdef01';

/** A raw index in the shape the scanner writes — parsed rather than trusted. */
function rawIndex(drift: boolean): unknown {
  return {
    version: 1,
    generatedAt: '2026-11-28T00:00:00.000Z',
    repos: [
      {
        repo: 'vitals',
        root: 'E:\\gh\\vitals',
        // The scanner records the symbolic ref, as the real index showed.
        git: { branch: 'refs/heads/main', head: HEAD },
        trackers: [
          {
            id: 'slices',
            file: 'docs/tracker.csv',
            countsByStatus: { done: 1, 'in-progress': 1 },
            items: [
              { id: 'S26-32', title: 'Tracking page', status: 'done', group: 'S26' },
              { id: 'S26-34', title: 'Release identity', status: 'in-progress', group: 'S26' },
            ],
          },
        ],
        drift: drift
          ? [
              {
                kind: 'unmapped-status',
                tracker: 'slices',
                ref: 'not-a-status',
                detail: 'not one of the seven',
              },
            ]
          : [],
      },
      {
        repo: 'agentq',
        root: 'E:\\gh\\agentq',
        // A different commit, so each card's short head is its own.
        git: { branch: 'main', head: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' },
        trackers: [],
        drift: [],
      },
    ],
  };
}

function index(drift = false): TrackerIndex {
  const parsed = parseIndex(rawIndex(drift));
  if (parsed === null) throw new Error('the fixture is not an index the parser accepts');
  return parsed;
}

function source(overrides: Partial<TrackingSource> = {}): TrackingSource {
  return {
    readIndex: () => Promise.resolve(index()),
    signedIn: () => Promise.resolve(false),
    ...overrides,
  };
}

function renderScreen(src: TrackingSource) {
  return render(
    // The shell provides these two providers at the root; `m.div` throws
    // without LazyMotion, which is the guard ADR-0017 relies on.
    <MotionRuntime reducedMotion="never">
      <TrackingScreen source={src} />
    </MotionRuntime>,
  );
}

describe('TrackingScreen', () => {
  it('says there is no data rather than drawing an empty catalogue it cannot fill', async () => {
    host.available = false;
    const readIndex = vi.fn(() => Promise.resolve(index()));

    renderScreen(source({ readIndex }));

    expect(await screen.findByText(t('noHost.title'))).toBeTruthy();
    // A browser has no file to read, so nothing may be read.
    expect(readIndex).not.toHaveBeenCalled();
  });

  it('offers the scanner command, and never the --all form, when nothing has scanned', async () => {
    renderScreen(source({ readIndex: () => Promise.resolve(null) }));

    expect(await screen.findByText(t('empty.title'))).toBeTruthy();
    expect(screen.getByText('node tools/tracker-lint.mjs --scan <dir> [<dir> ...]')).toBeTruthy();
    // `--scan --all` walks ~/gh, which on a machine without it silently means
    // "scan every directory in the home folder" — the wrong thing, quietly.
    expect(document.body.textContent ?? '').not.toContain('--scan --all');
  });

  it('draws a scanner index: the branch without its refs/heads prefix, and the rows', async () => {
    renderScreen(source());

    expect(await screen.findByText('vitals')).toBeTruthy();
    expect(screen.getByText('agentq')).toBeTruthy();
    // Eleven characters of prefix on every card is what the first real index
    // put on screen; `shortRef` drops that prefix and nothing else.
    expect(screen.queryByText(/refs\/heads/)).toBeNull();
    expect(screen.getAllByText('main').length).toBeGreaterThan(0);
    expect(screen.getByText(HEAD.slice(0, 7))).toBeTruthy();
    expect(screen.getByText('bbbbbbb')).toBeTruthy();
    expect(screen.getByText('S26-32')).toBeTruthy();
    expect(screen.getByText('S26-34')).toBeTruthy();
    // The repository card and the tracker block each print the count.
    expect(screen.getAllByText(t('tracker.items', { count: 2 }))).toHaveLength(2);
    expect(screen.getByText(t('kpi.items'))).toBeTruthy();
    expect(screen.getByText(t('warnings.none'))).toBeTruthy();
  });

  it('reports drift, and stops claiming there is none', async () => {
    renderScreen(source({ readIndex: () => Promise.resolve(index(true)) }));

    // Two places carry the count — the banner and the card that drifts — so
    // the banner is addressed by its own region rather than by the string.
    const banner = await screen.findByRole('region', { name: t('warnings.title') });
    expect(within(banner).getByText(t('warnings.drift', { count: 1 }))).toBeTruthy();
    expect(screen.queryByText(t('warnings.none'))).toBeNull();
    expect(screen.getByText('not one of the seven')).toBeTruthy();
  });

  it('shows the read failure instead of an empty catalogue, and keeps the message', async () => {
    renderScreen(source({ readIndex: () => Promise.reject(new Error('the file is not JSON')) }));

    expect(await screen.findByText(t('error.title'))).toBeTruthy();
    expect(screen.getByText('the file is not JSON')).toBeTruthy();
  });

  it('draws the account badge from the answer, not from an assumption', async () => {
    renderScreen(source({ signedIn: () => Promise.resolve(true) }));

    await waitFor(() => {
      expect(screen.getByLabelText(new RegExp(t('locality.connected')))).toBeTruthy();
    });
    expect(screen.queryByLabelText(new RegExp(t('locality.localOnly')))).toBeNull();
  });

  it('takes an unanswered account read as local-only rather than as connected', async () => {
    renderScreen(source({ signedIn: () => Promise.reject(new Error('no keyring')) }));

    expect(await screen.findByText('vitals')).toBeTruthy();
    expect(screen.getByLabelText(new RegExp(t('locality.localOnly')))).toBeTruthy();
  });

  it('re-reads the index when Reload is pressed', async () => {
    const readIndex = vi.fn(() => Promise.resolve(index()));

    renderScreen(source({ readIndex }));

    expect(await screen.findByText('vitals')).toBeTruthy();
    expect(readIndex).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: t('reload') }));

    await waitFor(() => {
      expect(readIndex).toHaveBeenCalledTimes(2);
    });
  });
});
