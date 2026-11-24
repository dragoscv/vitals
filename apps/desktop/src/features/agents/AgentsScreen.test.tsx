import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AgentqAction, AgentsSnapshot } from '@vitals/protocol';
import { i18n, initI18n } from '@vitals/i18n';

import { AgentsScreen } from './AgentsScreen';
import { NOW, makeSession, makeSnapshot, makeUsageSnapshot } from './fixtures.test-utils';
import { registerAgentsStrings } from './strings';
import type { AgentsSource } from './useAgents';

beforeAll(async () => {
  await initI18n();
  registerAgentsStrings();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function source(first: AgentsSnapshot) {
  let push: ((s: AgentsSnapshot) => void) | null = null;
  const src: AgentsSource & { push: (s: AgentsSnapshot) => void } = {
    subscribe: vi.fn(async () => ({ id: 7, snapshot: first })),
    unsubscribe: vi.fn(async () => undefined),
    listen: vi.fn(async (cb: (s: AgentsSnapshot) => void) => {
      push = cb;
      return () => {
        push = null;
      };
    }),
    run: vi.fn(async (_a: AgentqAction) => ({ ok: true, code: 0, output: '' })),
    usage: vi.fn(async () => makeUsageSnapshot()),
    push: (s) => push?.(s),
  };
  return src;
}

describe('AgentsScreen', () => {
  it('subscribes on mount, unsubscribes on unmount, and shows the live session', async () => {
    const src = source(makeSnapshot());
    const view = render(<AgentsScreen source={src} />);
    expect(await screen.findByText('Fix the queue')).toBeTruthy();
    expect(src.subscribe).toHaveBeenCalledWith(null);
    view.unmount();
    await waitFor(() => expect(src.unsubscribe).toHaveBeenCalledWith(7));
  });

  it('a pushed snapshot replaces the shown state without a reload', async () => {
    const src = source(makeSnapshot());
    render(<AgentsScreen source={src} />);
    await screen.findByText('Fix the queue');
    act(() => src.push(makeSnapshot({ sessions: [makeSession({ title: 'Ship the release' })] })));
    expect(await screen.findByText('Ship the release')).toBeTruthy();
  });

  it('reads the overview down one column while what agentq runs starts at the top beside it', async () => {
    // The order is the point of the layout: the machine-wide caps sit right
    // under the reads they explain, the queues those builds sit in stay under
    // the caps instead of being left alone at the bottom, and agentq's own
    // execution is the right-hand column from the top rather than a block
    // below the reads.
    const src = source(
      makeSnapshot({
        machine: {
          cap: 2,
          min: 1,
          max: 4,
          cpu: 12.5,
          freeRamGb: 20,
          freeCommitGb: 30,
          why: 'two cores are idle',
          atMs: NOW,
        },
      }),
    );
    const view = render(<AgentsScreen source={src} />);
    await screen.findByText('Resources in use');

    const first = (text: string): Element => {
      const el = screen.getAllByText(text)[0];
      if (el === undefined) throw new Error(`nothing on the page says ${text}`);
      return el;
    };
    const precedes = (a: string, b: string): boolean =>
      (first(a).compareDocumentPosition(first(b)) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;

    expect(precedes('Resources in use', 'Build slots on this machine')).toBe(true);
    // build:machine is a queue row, and it belongs to the caps directly above
    // it rather than to the bottom of the page where it used to sit.
    expect(precedes('Build slots on this machine', 'build:machine')).toBe(true);
    // 'Fix the queue' is a session title, so this is the agents following the
    // queues down the same column.
    expect(precedes('build:machine', 'Fix the queue')).toBe(true);
    expect(precedes('Fix the queue', 'What agentq is running')).toBe(true);

    // The section order above is what the left column reads down; the column
    // itself is the layout. Both columns are children of one grid, and the
    // agentq column is the last of them, which is what makes it start at the
    // top beside the reads rather than after them.
    const columns = document.querySelector('[class*="lg:grid-cols"]');
    expect(columns).not.toBeNull();
    expect(columns?.contains(first('Resources in use'))).toBe(true);
    expect(columns?.contains(first('Build slots on this machine'))).toBe(true);
    expect(columns?.lastElementChild?.contains(first('What agentq is running'))).toBe(true);
    view.unmount();
  });

  it('names the build section in a heading above its card, as the other sections do', async () => {
    // The card's title used to be its first line, inside the accent surface, so
    // the one section on the page without a heading above its surface read as a
    // stray panel between the sections instead of as one of them.
    const src = source(
      makeSnapshot({
        machine: {
          cap: 2,
          min: 1,
          max: 4,
          cpu: 12.5,
          freeRamGb: 20,
          freeCommitGb: 30,
          why: 'two cores are idle',
          atMs: NOW,
        },
      }),
    );
    const view = render(<AgentsScreen source={src} />);
    await screen.findByText('Build slots on this machine');

    const heading = document.getElementById('machine-heading');
    expect(heading?.tagName).toBe('H3');
    const section = heading?.closest('section') ?? null;
    expect(section).not.toBeNull();
    // The accent surface stays; it just no longer carries the name, so the
    // boundary and the title are read the same way as everywhere else.
    const card = section?.querySelector('[class*="accent-subtle"]') ?? null;
    expect(card).not.toBeNull();
    expect(card?.textContent ?? '').not.toContain('Build slots on this machine');
    // One name for the section, not a heading plus a copy inside the card.
    expect(screen.getAllByText('Build slots on this machine')).toHaveLength(1);
    view.unmount();
  });

  it('session cards keep their places when only last activity reorders the pushed list', async () => {
    const a = makeSession({ id: 'a', title: 'Alpha task' });
    const b = makeSession({ id: 'b', title: 'Bravo task' });
    const src = source(makeSnapshot({ sessions: [a, b] }));
    render(<AgentsScreen source={src} />);
    await screen.findByText('Alpha task');
    const titles = () =>
      screen.getAllByText(/^(Alpha|Bravo|Charlie) task$/).map((el) => el.textContent);
    const before = titles();
    // The desktop re-sorts by last activity: b is now more recent than a.
    act(() =>
      src.push(
        makeSnapshot({
          sessions: [
            { ...b, turns: 13 },
            { ...a, turns: 14 },
          ],
        }),
      ),
    );
    await waitFor(() => expect(screen.getAllByText('14').length).toBeGreaterThan(0));
    expect(titles()).toEqual(before);
  });

  it('a session that stops working still moves below the working ones at once', async () => {
    const a = makeSession({ id: 'a', title: 'Alpha task' });
    const b = makeSession({ id: 'b', title: 'Bravo task' });
    const src = source(makeSnapshot({ sessions: [a, b] }));
    render(<AgentsScreen source={src} />);
    fireEvent.mouseDown(await screen.findByRole('tab', { name: /Sessions/ }));
    fireEvent.click(screen.getByRole('tab', { name: /Sessions/ }));
    await screen.findByText('Alpha task');
    act(() =>
      src.push(
        makeSnapshot({
          sessions: [b, { ...a, state: 'idle', currentTool: null }],
        }),
      ),
    );
    await waitFor(() =>
      expect(screen.getAllByText(/^(Alpha|Bravo) task$/).map((el) => el.textContent)).toEqual([
        'Bravo task',
        'Alpha task',
      ]),
    );
  });

  it('session cards under the pointer do not move even when a state changes, until it leaves', async () => {
    const a = makeSession({ id: 'a', title: 'Alpha task' });
    const b = makeSession({ id: 'b', title: 'Bravo task' });
    const src = source(makeSnapshot({ sessions: [a, b] }));
    render(<AgentsScreen source={src} />);
    fireEvent.mouseDown(await screen.findByRole('tab', { name: /Sessions/ }));
    fireEvent.click(screen.getByRole('tab', { name: /Sessions/ }));
    const alpha = await screen.findByText('Alpha task');
    const titles = () => screen.getAllByText(/^(Alpha|Bravo) task$/).map((el) => el.textContent);
    fireEvent.pointerEnter(alpha);
    act(() =>
      src.push(
        makeSnapshot({ sessions: [b, { ...a, state: 'idle', currentTool: null, turns: 21 }] }),
      ),
    );
    await waitFor(() => expect(screen.getAllByText('21').length).toBeGreaterThan(0));
    expect(titles(), 'nothing may slide out from under the pointer').toEqual([
      'Alpha task',
      'Bravo task',
    ]);
    fireEvent.pointerLeave(alpha);
    await waitFor(() => expect(titles()).toEqual(['Bravo task', 'Alpha task']));
  });

  it('removing a dead ticket asks for a reason and sends it with the ticket id', async () => {
    const src = source(makeSnapshot());
    render(<AgentsScreen source={src} />);
    fireEvent.mouseDown(await screen.findByRole('tab', { name: /Queues/ }));
    fireEvent.click(screen.getByRole('tab', { name: /Queues/ }));
    fireEvent.click(await screen.findByRole('button', { name: 'Remove ticket t2' }));
    const run = await screen.findByRole('button', { name: 'Run' });
    expect((run as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'holder pid is dead' } });
    fireEvent.click(run);
    await waitFor(() =>
      expect(src.run).toHaveBeenCalledWith({
        kind: 'break',
        resource: 'build:vitals',
        ticket: 't2',
        reason: 'holder pid is dead',
      }),
    );
  });

  it('says so when agentq is not installed instead of showing empty queues as fact', async () => {
    const src = source(makeSnapshot({ agentqInstalled: false, resources: [] }));
    render(<AgentsScreen source={src} />);
    expect(await screen.findByText(/agentq is not installed/)).toBeTruthy();
  });

  it('asks the backend for resource use and shows it, on the overview and on its own tab', async () => {
    const src = source(makeSnapshot());
    render(<AgentsScreen source={src} />);
    // For the table the sampler already holds, not a second enumeration.
    await waitFor(() => expect(src.usage).toHaveBeenCalled());
    // No Tauri host in a test, so there is no metrics hub to fold: the rows
    // arrive from the injected source instead and the panel draws them.
    expect(await screen.findByText('Resources in use')).toBeTruthy();
    fireEvent.click(screen.getByRole('tab', { name: /Resources/ }));
    expect(await screen.findByText('Claude')).toBeTruthy();
    // Both the overview and this tab draw a panel, so the exact counts live
    // in the panel's own test; here it is enough that the rows arrived.
    expect(screen.getAllByText('VS Code Copilot Chat').length).toBeGreaterThan(0);
    expect(screen.getAllByText('3 processes').length).toBeGreaterThanOrEqual(3);
  });
});
