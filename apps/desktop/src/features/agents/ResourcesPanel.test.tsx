/**
 * The resources panel's two honesty rules, both of which are easy to break by
 * "tidying up": a figure the machine does not report is a dash and never a
 * zero, and a tool that owns no process gets no row.
 */

import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { makeUsage, makeUsageSnapshot } from './fixtures.test-utils';
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

  it('lists agentq first and then the clients, with the process count', () => {
    render(<ResourcesPanel usage={makeUsageSnapshot()} now={Date.now()} />);
    const headings = [...document.querySelectorAll('li span.font-medium')].map(
      (n) => n.textContent,
    );
    expect(headings).toEqual(['agentq', 'Claude', 'VS Code Copilot Chat']);
    expect(screen.getAllByText('3 processes')).toHaveLength(3);
  });
});
