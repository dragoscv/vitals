import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AgentqAction, AgentsSnapshot } from '@vitals/protocol';
import { i18n, initI18n } from '@vitals/i18n';

import { AgentsScreen } from './AgentsScreen';
import { makeSession, makeSnapshot } from './fixtures.test-utils';
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
});
