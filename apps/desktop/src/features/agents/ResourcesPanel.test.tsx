/**
 * The resources panel's two honesty rules, both of which are easy to break by
 * "tidying up": a figure the machine does not report is a dash and never a
 * zero, and a tool that owns no process gets no row.
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { makeUsage, makeUsageSnapshot, makeWork } from './fixtures.test-utils';
import { ResourcesPanel } from './ResourcesPanel';
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
