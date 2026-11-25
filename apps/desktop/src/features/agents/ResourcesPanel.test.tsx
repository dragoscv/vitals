/**
 * The resources panel's two honesty rules, both of which are easy to break by
 * "tidying up": a figure the machine does not report is a dash and never a
 * zero, and a tool that owns no process gets no row.
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { NOW, makeEstimate, makeUsage, makeUsageSnapshot, makeWork } from './fixtures.test-utils';
import { AgentqWork, ResourcesPanel } from './ResourcesPanel';
import { registerAgentsStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerAgentsStrings();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

/** Every value cell, so an assertion cannot be satisfied by the footnote. */
function values(container: HTMLElement): string[] {
  return [...container.querySelectorAll('dd')].map((d) => d.textContent ?? '');
}

describe('ResourcesPanel', () => {
  it('shows a dash for a series the machine does not report, never 0 B/s', () => {
    const view = render(<ResourcesPanel usage={makeUsageSnapshot()} now={Date.now()} />);
    const cells = values(view.container);
    // The disk figures are real, so the dashes below come from the network
    // series being absent rather than from the panel drawing dashes at all.
    expect(cells.some((v) => v.includes('MB/s'))).toBe(true);
    expect(cells.some((v) => v.includes('—'))).toBe(true);
    expect(cells.some((v) => v.includes('0 B/s'))).toBe(false);
  });

  it('shows a measured network figure when the machine does report one', () => {
    const view = render(
      <ResourcesPanel
        usage={makeUsageSnapshot({
          agentq: makeUsage('agentq', { netRx: 2_048, netTx: 1_024 }),
        })}
        now={Date.now()}
      />,
    );
    expect(values(view.container).some((v) => v.includes('KB/s'))).toBe(true);
  });

  it('offers an empty state, not a list of zeroes, when no tool owns a process', () => {
    render(
      <ResourcesPanel usage={makeUsageSnapshot({ agentq: null, clients: [] })} now={Date.now()} />,
    );
    expect(screen.getByText('No agent process is running')).toBeTruthy();
    expect(values(document.body)).toHaveLength(0);
  });

  it('keeps the dash explanation behind the (i) beside the title, not under the cards', () => {
    render(<ResourcesPanel usage={makeUsageSnapshot()} now={Date.now()} />);
    const label = 'About the figures in this section';
    const heading = document.getElementById('resources-in-use-heading');
    expect(heading).not.toBeNull();
    // The heading is what names the section (`aria-labelledby`), so the button
    // must not be nested inside it, or the section's name becomes the label of
    // a control.
    expect(heading?.textContent ?? '').not.toContain(label);
    const section = heading?.closest('section') ?? null;
    expect(section?.textContent ?? '').not.toContain('Folded from the same process table');

    const info = screen.getByRole('button', { name: label });
    expect(info.getAttribute('aria-expanded')).toBe('false');
    fireEvent.click(info);
    expect(info.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText(/Folded from the same process table/)).toBeTruthy();
  });

  it('lists agentq first and then the clients, with the process count', () => {
    render(<ResourcesPanel usage={makeUsageSnapshot()} now={Date.now()} />);
    const headings = [...document.querySelectorAll('li span.font-medium')].map(
      (n) => n.textContent,
    );
    expect(headings).toEqual(['agentq', 'Claude', 'VS Code Copilot Chat']);
    expect(screen.getAllByText('3 processes')).toHaveLength(3);
  });

  it('breaks agentq’s own figure down into what it is executing, by pid', () => {
    render(
      <ResourcesPanel
        usage={makeUsageSnapshot({
          work: [
            makeWork(),
            makeWork({
              pid: 4243,
              label: 'land:codai',
              kind: 'land',
              purpose: null,
              command: 'pnpm land',
            }),
          ],
        })}
        now={Date.now()}
      />,
    );
    expect(screen.getByText('What agentq is running')).toBeTruthy();
    expect(screen.getByText('build:vitals')).toBeTruthy();
    expect(screen.getByText('land:codai')).toBeTruthy();
    // The kind badge reads the work's own kind, never the agentq usage kind.
    expect(screen.getByText('land')).toBeTruthy();
    // A purpose is preferred over the command, and a row without one falls
    // back to the command rather than showing nothing.
    expect(screen.getByText('why it was queued')).toBeTruthy();
    expect(screen.getByText('pnpm land')).toBeTruthy();
  });

  it('draws no work list at all when agentq is executing nothing', () => {
    render(<ResourcesPanel usage={makeUsageSnapshot()} now={Date.now()} />);
    expect(screen.queryByText('What agentq is running')).toBeNull();
  });
});

/**
 * The ETA and its bar: agentq's own profiles, drawn on the row.
 *
 * `now` is the fixture's clock rather than the wall clock, because an elapsed
 * time read against a real one would drift between the two assertions.
 */
describe('AgentqWork progress', () => {
  it('scales the bar to the whole expected duration and prints the ETA beside it', () => {
    render(
      <AgentqWork
        usage={makeUsageSnapshot({
          work: [makeWork({ estimate: makeEstimate({ expectedMs: 300_000, samples: 12 }) })],
        })}
        now={NOW}
      />,
    );
    const bar = screen.getByRole('progressbar', { name: 'Progress of build:vitals' });
    expect(bar.getAttribute('aria-valuenow')).toBe('120000');
    expect(bar.getAttribute('aria-valuemax')).toBe('300000');
    // The spoken value carries the basis, because the number alone does not
    // say whether the overrun is against a promise or against the usual.
    expect(bar.getAttribute('aria-valuetext')).toBe(
      'about 3m 00s left — the median of the last 12 runs of this resource',
    );
    expect(screen.getByText('about 3m 00s left')).toBeTruthy();
  });

  it('says how far past the estimate it is instead of a negative or zero ETA', () => {
    render(
      <AgentqWork
        usage={makeUsageSnapshot({
          work: [
            makeWork({
              startedMs: NOW - 400_000,
              estimate: makeEstimate({ expectedMs: 300_000, basis: 'declared', samples: 0 }),
            }),
          ],
        })}
        now={NOW}
      />,
    );
    const bar = screen.getByRole('progressbar', { name: 'Progress of build:vitals' });
    // Full, not overshot: the track is the expected duration, so the value is
    // clamped to it — `aria-valuenow` above `aria-valuemax` is invalid — and
    // the overrun is the sentence beside the bar.
    expect(bar.getAttribute('aria-valuenow')).toBe('300000');
    expect(bar.getAttribute('aria-valuetext')).toBe(
      '1m 40s past the expected time — what the job declared when it was launched',
    );
    expect(screen.getByText('1m 40s past the expected time')).toBeTruthy();
  });

  it('draws no bar for work nothing has measured', () => {
    render(
      <AgentqWork usage={makeUsageSnapshot({ work: [makeWork({ estimate: null })] })} now={NOW} />,
    );
    expect(screen.queryByRole('progressbar')).toBeNull();
    // The row itself is still drawn: no estimate is not "nothing running".
    expect(screen.getByText('build:vitals')).toBeTruthy();
  });
});
