/**
 * Behavioural tests for the Processes screen.
 *
 * These assert what a user experiences, not how it is implemented: that a
 * forbidden action offers no way to proceed, that a critical warning says
 * something different from a safe one, that rows do not move while the
 * pointer is over them, and that a keyboard-only user can reach and use the
 * context menu.
 */

import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import type { ActionPlan, ProcessActionsApi } from './actions';
import { makeSystem } from '../dashboard/test-fixtures';
import { createIconStore } from './icons';
import { ProcessesScreen } from './ProcessesScreen';
import { makeMap, makeProcess } from './test-fixtures';
import { registerProcessesStrings } from './strings';
import { createManualSnapshotSource, INITIAL_SNAPSHOT, NO_SAMPLER } from './useProcessSnapshot';

beforeAll(async () => {
  await initI18n('en');
  registerProcessesStrings();
});

afterEach(cleanup);

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  };
}

function plan(overrides: Partial<ActionPlan> = {}): ActionPlan {
  return {
    risk: 'safe',
    consequence: 'process.confirm.safe',
    needsConfirmation: true,
    elevationMightHelp: true,
    ...overrides,
  };
}

function stubActions(overrides: Partial<ProcessActionsApi> = {}): ProcessActionsApi {
  return {
    planTerminate: vi.fn(async () => plan()),
    planSuspend: vi.fn(async () => plan()),
    terminate: vi.fn(async () => undefined),
    suspend: vi.fn(async () => undefined),
    resume: vi.fn(async () => undefined),
    setPriority: vi.fn(async () => undefined),
    setAffinity: vi.fn(async () => undefined),
    getEfficiencyMode: vi.fn(async () => null),
    setEfficiencyMode: vi.fn(async () => undefined),
    getHandles: vi.fn(async () => []),
    getModules: vi.fn(async () => []),
    getExecutablePath: vi.fn(async () => null),
    bringToFront: vi.fn(async () => true),
    openFileLocation: vi.fn(async () => undefined),
    showFileProperties: vi.fn(async () => undefined),
    runAsAdmin: vi.fn(async () => undefined),
    ...overrides,
  };
}

const PROCESSES = [
  makeProcess({ pid: 100, name: 'explorer.exe', kind: 'app', cpu: 5 }),
  makeProcess({ pid: 200, name: 'csrss.exe', kind: 'system', cpu: 1, protection: 'full' }),
  makeProcess({ pid: 300, name: 'chrome.exe', kind: 'app', cpu: 12 }),
];

function mountScreen(actions: ProcessActionsApi = stubActions()) {
  const source = createManualSnapshotSource({
    ...INITIAL_SNAPSHOT,
    processes: makeMap(PROCESSES),
    pending: false,
  });
  const view = render(
    <ProcessesScreen source={source} actions={actions} storage={memoryStorage()} />,
  );
  return { source, view, actions };
}

/**
 * Opens a row's context menu the way a mouse user would.
 *
 * The `act` flush is required because Radix mounts the menu into a portal on
 * a microtask; without it the assertion runs against a DOM that has not
 * received the menu yet.
 */
async function openMenuFor(pid: number): Promise<void> {
  const row = await screen.findByTestId(`process-row-${pid}`);
  fireEvent.contextMenu(row, { button: 2 });
  await act(async () => {});
  await screen.findByRole('menu');
}

describe('rendering', () => {
  it('shows the sampled processes', async () => {
    mountScreen();
    expect(await screen.findByText('chrome.exe')).toBeTruthy();
    expect(screen.getByText('explorer.exe')).toBeTruthy();
  });

  it('shows skeletons rather than an empty table before the first frame', () => {
    // No grid at all until data arrives: an empty table would claim, wrongly,
    // that the machine is running no processes.
    const source = createManualSnapshotSource(INITIAL_SNAPSHOT);
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);
    expect(screen.queryByRole('grid')).toBeNull();
    expect(document.querySelector('[aria-busy="true"]')).not.toBeNull();
  });

  it('explains a dead sampler instead of blaming the user filter', async () => {
    // "No process matches — clear the search" is only true when a filter
    // excluded them. With no sampler there is nothing to match in the first
    // place, and that message sends the user hunting for a mistake they did
    // not make. Found live: the table sat at "0 of 0 processes" forever.
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      error: NO_SAMPLER,
    });
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);

    expect(await screen.findByText('No readings are arriving')).toBeTruthy();
    expect(screen.queryByText('No process matches')).toBeNull();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('still blames the filter when a filter is genuinely responsible', async () => {
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      processes: makeMap([]),
    });
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);

    expect(await screen.findByText('No process matches')).toBeTruthy();
  });

  it('renders an em-dash where a reading is absent, never a zero', async () => {
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      processes: makeMap([makeProcess({ pid: 7, name: 'x.exe', gpu: null })]),
    });
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);
    const row = await screen.findByTestId('process-row-7');
    expect(within(row).getAllByText('—').length).toBeGreaterThan(0);
  });

  it('puts each program’s own icon beside its name, and a kind glyph where it has none', async () => {
    const icons = createIconStore((keys) =>
      Promise.resolve(keys.map((k) => (k.pid === 300 ? 'data:image/png;base64,AAAA' : null))),
    );
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      processes: makeMap(PROCESSES),
      pending: false,
    });
    render(
      <ProcessesScreen
        source={source}
        actions={stubActions()}
        storage={memoryStorage()}
        icons={icons}
      />,
    );
    const chrome = await screen.findByTestId('process-row-300');
    await waitFor(() => expect(within(chrome).getByTestId('process-icon')).toBeTruthy());
    expect(within(chrome).getByTestId('process-icon').getAttribute('src')).toContain('data:image');
    const csrss = screen.getByTestId('process-row-200');
    expect(within(csrss).queryByTestId('process-icon')).toBeNull();
    expect(within(csrss).getByTestId('process-glyph')).toBeTruthy();
  });

  it('shows machine-wide totals in the headers, and a dash for one it cannot measure', async () => {
    const base = makeSystem();
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      processes: makeMap(PROCESSES),
      system: {
        ...base,
        cpu: { ...base.cpu, total: 61.4 },
        memory: { ...base.memory, total: 100, used: 32 },
        gpus: [],
      },
      pending: false,
    });
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);

    expect((await screen.findByTestId('column-total-cpu')).textContent).toBe('61%');
    expect(screen.getByTestId('column-total-memory').textContent).toBe('32%');
    expect(screen.getByTestId('column-total-gpu').textContent).toBe('—');
    // Identity columns carry no total.
    expect(screen.queryByTestId('column-total-name')).toBeNull();
    // Said to a screen reader too, not only drawn.
    expect(screen.getByRole('button', { name: /CPU.*61%/ })).toBeTruthy();
  });
});

describe('search and filters', () => {
  it('narrows the table to matching processes', async () => {
    mountScreen();
    const search = screen.getByRole('searchbox');
    fireEvent.change(search, { target: { value: 'chrome' } });
    await waitFor(() => {
      expect(screen.queryByText('explorer.exe')).toBeNull();
    });
    expect(screen.getByText('chrome.exe')).toBeTruthy();
  });

  it('matches on PID, because people arrive with a number from a crash log', async () => {
    mountScreen();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '200' } });
    await waitFor(() => expect(screen.queryByText('chrome.exe')).toBeNull());
    expect(screen.getByText('csrss.exe')).toBeTruthy();
  });

  it('shows an empty state, not a blank area, when nothing matches', async () => {
    mountScreen();
    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'zzzz' } });
    expect(await screen.findByText('No process matches')).toBeTruthy();
  });

  it('filters to Windows processes by kind', async () => {
    mountScreen();
    fireEvent.click(screen.getByRole('radio', { name: 'Windows processes' }));
    await waitFor(() => expect(screen.queryByText('chrome.exe')).toBeNull());
    expect(screen.getByText('csrss.exe')).toBeTruthy();
  });
});

describe('the confirmation flow', () => {
  it('turns an access denial into a one-click retry as administrator, for that process only', async () => {
    // The button existed for months with no handler: a SYSTEM-owned process
    // could be neither ended nor paused, and the only feedback was an error
    // line. Now the denial reopens the dialog with the retry as the action.
    const denied = { kind: 'access-denied', message: 'open process 300 requires elevation' };
    const actions = stubActions({
      planTerminate: vi.fn(async () => plan({ needsConfirmation: false })),
      terminate: vi.fn(async () => {
        throw denied;
      }),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));

    const dialog = await screen.findByTestId('risk-dialog');
    expect(within(dialog).getByText(/belongs to another account or to the system/i)).toBeTruthy();
    // The plain confirm would only fail again, so it is gone.
    expect(screen.queryByTestId('risk-confirm')).toBeNull();

    fireEvent.click(screen.getByTestId('risk-elevate'));
    await waitFor(() => expect(actions.runAsAdmin).toHaveBeenCalledOnce());
    expect(actions.runAsAdmin).toHaveBeenCalledWith(
      'terminate',
      expect.objectContaining({ name: 'chrome.exe' }),
      // The user clicked through the dialog: the elevated child must be told
      // so, or the execution-time gate refuses a critical process again.
      true,
    );
    await waitFor(() => expect(screen.queryByTestId('risk-dialog')).toBeNull());
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('reports a dismissed UAC prompt as the answer it is, not as a failure', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () => plan({ needsConfirmation: false })),
      terminate: vi.fn(async () => {
        throw { kind: 'access-denied', message: 'denied' };
      }),
      runAsAdmin: vi.fn(async () => {
        throw { kind: 'refused', message: 'declined' };
      }),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    fireEvent.click(await screen.findByTestId('risk-elevate'));

    const alert = await screen.findByRole('alert');
    expect(alert.textContent).toMatch(/approval was declined, so nothing was changed/i);
    expect(alert.textContent).not.toMatch(/object Object/);
  });

  it('never raises a UAC prompt the user did not ask for', async () => {
    // The denial offers the retry; it must not perform it. An elevation
    // prompt appearing unprompted is how people learn to click Yes blindly.
    const actions = stubActions({
      planTerminate: vi.fn(async () => plan({ needsConfirmation: false })),
      terminate: vi.fn(async () => {
        throw { kind: 'access-denied', message: 'denied' };
      }),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    await screen.findByTestId('risk-elevate');
    expect(actions.runAsAdmin).not.toHaveBeenCalled();
  });

  it('plans before it acts, and does not act until confirmed', async () => {
    const actions = stubActions();
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));

    await screen.findByTestId('risk-dialog');
    expect(actions.planTerminate).toHaveBeenCalledOnce();
    // Nothing has been killed yet. A plan is a question, not an action.
    expect(actions.terminate).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId('risk-confirm'));
    await waitFor(() => expect(actions.terminate).toHaveBeenCalledOnce());
  });

  it('states the specific consequence rather than a generic "are you sure?"', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () =>
        plan({ risk: 'critical', consequence: 'process.confirm.critical' }),
      ),
    });
    mountScreen(actions);
    await openMenuFor(200);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));

    const dialog = await screen.findByTestId('risk-dialog');
    expect(within(dialog).getByText(/crash Windows immediately/i)).toBeTruthy();
  });

  it('does not make a critical warning look like a safe one', async () => {
    const safe = stubActions();
    mountScreen(safe);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    const safeText = (await screen.findByTestId('risk-dialog')).textContent ?? '';
    cleanup();

    const critical = stubActions({
      planTerminate: vi.fn(async () =>
        plan({ risk: 'critical', consequence: 'process.confirm.critical' }),
      ),
    });
    mountScreen(critical);
    await openMenuFor(200);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    const criticalText = (await screen.findByTestId('risk-dialog')).textContent ?? '';

    expect(criticalText).not.toBe(safeText);
  });

  it('offers NO elevation for a forbidden action, because elevation cannot help', async () => {
    // This is the Task Manager behaviour we are deliberately fixing: it
    // offers "try again as administrator" for a protected process, the user
    // elevates, it fails identically, and they learn the prompt is a lie.
    const actions = stubActions({
      planTerminate: vi.fn(async () =>
        plan({
          risk: 'forbidden',
          consequence: 'process.confirm.forbidden',
          needsConfirmation: false,
          elevationMightHelp: false,
        }),
      ),
    });
    mountScreen(actions);
    await openMenuFor(200);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));

    const dialog = await screen.findByTestId('risk-dialog');
    expect(dialog.getAttribute('data-risk')).toBe('forbidden');
    // No control offering elevation. The word "administrator" does appear —
    // the consequence says Windows blocks this *even as* an administrator,
    // which is the honest explanation. What must not exist is a button
    // inviting the user to retry that way.
    expect(within(dialog).queryByRole('button', { name: /retry as administrator/i })).toBeNull();
    // And no way to proceed at all — the action is not merely discouraged.
    expect(screen.queryByTestId('risk-confirm')).toBeNull();
    expect(actions.terminate).not.toHaveBeenCalled();
  });

  it('does not terminate anything when a forbidden dialog is dismissed', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () =>
        plan({
          risk: 'forbidden',
          consequence: 'process.confirm.forbidden',
          needsConfirmation: false,
          elevationMightHelp: false,
        }),
      ),
    });
    mountScreen(actions);
    await openMenuFor(200);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    await screen.findByTestId('risk-dialog');
    // The dialog's own dismiss control, not the header's icon button.
    const dialog = screen.getByTestId('risk-dialog');
    const closers = within(dialog).getAllByRole('button', { name: 'Close' });
    fireEvent.click(closers[closers.length - 1] as HTMLElement);
    await waitFor(() => expect(screen.queryByTestId('risk-dialog')).toBeNull());
    expect(actions.terminate).not.toHaveBeenCalled();
  });

  it('cancelling leaves the process running', async () => {
    const actions = stubActions();
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    await screen.findByTestId('risk-dialog');
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByTestId('risk-dialog')).toBeNull());
    expect(actions.terminate).not.toHaveBeenCalled();
  });

  it('skips the dialog only when the backend says confirmation is unnecessary', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () => plan({ risk: 'safe', needsConfirmation: false })),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    await waitFor(() => expect(actions.terminate).toHaveBeenCalledOnce());
  });

  it('surfaces a backend failure instead of pretending the kill worked', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () => plan({ needsConfirmation: false })),
      terminate: vi.fn(async () => {
        throw { kind: 'internal', message: 'TerminateProcess failed: 87' };
      }),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    expect((await screen.findByRole('alert')).textContent).toContain('TerminateProcess failed');
  });

  it('surfaces a denial where elevation cannot help as an error, with no retry', async () => {
    const actions = stubActions({
      planTerminate: vi.fn(async () =>
        plan({ needsConfirmation: false, elevationMightHelp: false }),
      ),
      terminate: vi.fn(async () => {
        throw { kind: 'access-denied', message: 'denied' };
      }),
    });
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'End task' }));
    expect((await screen.findByRole('alert')).textContent).toContain('denied');
    expect(screen.queryByTestId('risk-elevate')).toBeNull();
  });

  it('suspending uses the suspend planner, not the terminate one', async () => {
    const actions = stubActions();
    mountScreen(actions);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Suspend' }));
    await screen.findByTestId('risk-dialog');
    expect(actions.planSuspend).toHaveBeenCalledOnce();
    expect(actions.planTerminate).not.toHaveBeenCalled();
  });
});

describe('keyboard access', () => {
  it('exposes the grid as a single tab stop with arrow navigation', async () => {
    mountScreen();
    const grid = await screen.findByRole('grid');
    expect(grid.getAttribute('tabindex')).toBe('0');
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await waitFor(() => expect(document.querySelector('[data-focused="true"]')).not.toBeNull());
  });

  it('lets a keyboard-only user open the context menu and end a process', async () => {
    // Without this path the entire screen is mouse-only: every action lives
    // in the context menu.
    const actions = stubActions();
    mountScreen(actions);
    const row = await screen.findByTestId('process-row-300');
    // Shift+F10 and the Menu key do not reach the component as a keydown —
    // the browser converts them into a native `contextmenu` event with no
    // pointer coordinates. Asserting on that exact shape is what proves the
    // keyboard route works rather than only the mouse one.
    fireEvent.keyDown(row, { key: 'F10', shiftKey: true });
    fireEvent.contextMenu(row, { detail: 0, clientX: 0, clientY: 0, button: 0 });
    await act(async () => {});
    const menu = await screen.findByRole('menu');
    fireEvent.click(within(menu).getByRole('menuitem', { name: 'End task' }));
    await screen.findByTestId('risk-dialog');
    fireEvent.click(screen.getByTestId('risk-confirm'));
    await waitFor(() => expect(actions.terminate).toHaveBeenCalledOnce());
  });

  it('a left click selects the row and never opens its menu', async () => {
    // 2026-10-05: a left click opened the menu, pinned near the window's
    // corner. A contextmenu event that is neither a right click nor follows
    // the menu keys is that accident, and must be refused.
    mountScreen();
    const row = await screen.findByTestId('process-row-300');
    fireEvent.pointerDown(row, { button: 0, pointerType: 'touch' });
    fireEvent.contextMenu(row, { button: 0, clientX: 0, clientY: 0 });
    await act(async () => {});
    expect(screen.queryByRole('menu')).toBeNull();
    expect(row.getAttribute('aria-selected')).toBe('true');
  });

  it('a long press that is not a right click does not open the menu', async () => {
    // Radix opens after 700 ms of a non-mouse press; precision touchpads
    // report `touch`, so a slow left click on one was a menu.
    vi.useFakeTimers();
    try {
      mountScreen();
      const row = screen.getByTestId('process-row-300');
      fireEvent.pointerDown(row, { button: 0, pointerType: 'touch', clientX: 40, clientY: 40 });
      await act(async () => {
        vi.advanceTimersByTime(800);
      });
      expect(screen.queryByRole('menu')).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it('changes priority from the submenu without a confirmation dialog', async () => {
    // The submenu was inert for a long time — the backend command existed and
    // nothing exposed it — so this asserts the whole path, not just that the
    // items render.
    //
    // No dialog on purpose: priority is reversible and immediate, and putting
    // a modal in front of it would dilute the ones guarding actions that
    // destroy work.
    const actions = stubActions();
    mountScreen(actions);
    await openMenuFor(300);

    const menu = await screen.findByRole('menu');
    fireEvent.click(within(menu).getByRole('menuitem', { name: 'Priority' }));

    const item = await screen.findByRole('menuitem', { name: 'Below normal' });
    fireEvent.click(item);

    await waitFor(() => expect(actions.setPriority).toHaveBeenCalledOnce());
    expect(actions.setPriority).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'chrome.exe' }),
      'below-normal',
    );
    expect(screen.queryByTestId('risk-dialog')).toBeNull();
  });

  it('Delete goes through the same plan-and-confirm path as the menu', async () => {
    const actions = stubActions();
    mountScreen(actions);
    const grid = await screen.findByRole('grid');
    fireEvent.keyDown(grid, { key: 'ArrowDown' });
    fireEvent.keyDown(grid, { key: 'Delete' });
    await screen.findByTestId('risk-dialog');
    expect(actions.terminate).not.toHaveBeenCalled();
  });

  it('Home and End move the focused row to the extremes', async () => {
    mountScreen();
    const grid = await screen.findByRole('grid');
    fireEvent.keyDown(grid, { key: 'End' });
    await waitFor(() => expect(document.querySelector('[data-focused="true"]')).not.toBeNull());
    fireEvent.keyDown(grid, { key: 'Home' });
    await waitFor(() => expect(document.querySelector('[data-focused="true"]')).not.toBeNull());
  });
});

describe('selection', () => {
  it('replaces the selection on a plain click', async () => {
    mountScreen();
    const a = await screen.findByTestId('process-row-300');
    fireEvent.pointerDown(a);
    await waitFor(() => expect(a.getAttribute('aria-selected')).toBe('true'));

    const b = screen.getByTestId('process-row-100');
    fireEvent.pointerDown(b);
    await waitFor(() => expect(b.getAttribute('aria-selected')).toBe('true'));
    expect(screen.getByTestId('process-row-300').getAttribute('aria-selected')).toBe('false');
  });

  it('adds to the selection with Ctrl', async () => {
    mountScreen();
    fireEvent.pointerDown(await screen.findByTestId('process-row-300'));
    fireEvent.pointerDown(screen.getByTestId('process-row-100'), { ctrlKey: true });
    await waitFor(() => {
      expect(screen.getByTestId('process-row-100').getAttribute('aria-selected')).toBe('true');
      expect(screen.getByTestId('process-row-300').getAttribute('aria-selected')).toBe('true');
    });
  });

  it('extends the selection with Shift', async () => {
    mountScreen();
    const rows = await screen.findAllByRole('row');
    // Row 0 is the header.
    const first = rows[1] as HTMLElement;
    const last = rows[rows.length - 1] as HTMLElement;
    fireEvent.pointerDown(first);
    fireEvent.pointerDown(last, { shiftKey: true });
    await waitFor(() => {
      const selectedCount = screen
        .getAllByRole('row')
        .filter((row) => row.getAttribute('aria-selected') === 'true').length;
      expect(selectedCount).toBeGreaterThan(1);
    });
  });
});

describe('sort stability in the mounted table', () => {
  it('holds row positions while the pointer is over the table', async () => {
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      processes: makeMap([
        makeProcess({ pid: 1, name: 'a.exe', cpu: 10 }),
        makeProcess({ pid: 2, name: 'b.exe', cpu: 9 }),
      ]),
    });
    render(<ProcessesScreen source={source} actions={stubActions()} storage={memoryStorage()} />);

    const order = (): readonly string[] =>
      screen
        .getAllByRole('row')
        .slice(1)
        .map((row) => row.getAttribute('data-testid') ?? '');

    await screen.findByTestId('process-row-1');
    const before = order();

    // Pointer enters: the user is aiming at a row.
    fireEvent.pointerEnter(screen.getByRole('grid').parentElement as HTMLElement);

    // A tick in which b decisively overtakes a. Unfrozen this would swap the
    // two rows out from under the cursor mid-click.
    source.push({
      processes: makeMap([
        makeProcess({ pid: 1, name: 'a.exe', cpu: 1 }),
        makeProcess({ pid: 2, name: 'b.exe', cpu: 90 }),
      ]),
    });

    await waitFor(() => expect(order()).toEqual(before));
  });

  it('pointing at the table changes nothing in the toolbar, so the rows cannot jump', async () => {
    // The old "Order held" pill appeared on hover and wrapped Export onto a
    // second line, pushing the table down a row under the cursor.
    mountScreen();
    const grid = await screen.findByRole('grid');
    const toolbar = screen.getByRole('button', { name: /columns/i }).parentElement as HTMLElement;
    const before = toolbar.childElementCount;
    fireEvent.pointerEnter(grid.parentElement as HTMLElement);
    expect(screen.queryByTestId('order-held')).toBeNull();
    expect(toolbar.childElementCount).toBe(before);
  });
});

describe('shell actions', () => {
  it('hides the details panel on request and brings it back', async () => {
    mountScreen();
    await screen.findByRole('grid');
    expect(screen.getByRole('complementary')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Hide details panel' }));
    expect(screen.queryByRole('complementary')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Show details panel' }));
    expect(screen.getByRole('complementary')).toBeTruthy();
  });

  it('disables file location and properties when the path is unknown', async () => {
    // A protected process never yields its path. An enabled item that fails
    // every time reads as broken; disabled with a reason reads as honest.
    mountScreen(stubActions({ getExecutablePath: vi.fn(async () => null) }));
    await openMenuFor(300);
    const location = screen.getByRole('menuitem', { name: 'Open file location' });
    expect(location.getAttribute('data-disabled')).not.toBeNull();
    const properties = screen.getByRole('menuitem', { name: 'Properties' });
    expect(properties.getAttribute('data-disabled')).not.toBeNull();
  });

  it('opens the file location with the resolved path once it is known', async () => {
    const actions = stubActions({
      getExecutablePath: vi.fn(async () => 'C:\\Apps\\chrome.exe'),
    });
    mountScreen(actions);
    // Focus the row first so the path is read; the menu alone does not focus.
    const row = await screen.findByTestId('process-row-300');
    fireEvent.pointerDown(row);
    await waitFor(() => expect(actions.getExecutablePath).toHaveBeenCalledOnce());
    await openMenuFor(300);
    const location = await screen.findByRole('menuitem', { name: 'Open file location' });
    await waitFor(() => expect(location.getAttribute('data-disabled')).toBeNull());
    fireEvent.click(location);
    await waitFor(() =>
      expect(actions.openFileLocation).toHaveBeenCalledWith('C:\\Apps\\chrome.exe'),
    );
  });

  it('offers to bring an app with a window to the front, and only such an app', async () => {
    const actions = stubActions();
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      processes: makeMap([
        makeProcess({ pid: 300, name: 'chrome.exe', kind: 'app', flags: 1 << 6 }),
        makeProcess({ pid: 400, name: 'svc.exe', kind: 'service' }),
      ]),
    });
    render(<ProcessesScreen source={source} actions={actions} storage={memoryStorage()} />);

    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Bring to front' }));
    await waitFor(() => expect(actions.bringToFront).toHaveBeenCalledOnce());
    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });

    await openMenuFor(400);
    expect(screen.queryByRole('menuitem', { name: 'Bring to front' })).toBeNull();
  });

  it('says so when the window has gone instead of failing silently', async () => {
    const actions = stubActions({ bringToFront: vi.fn(async () => false) });
    const source = createManualSnapshotSource({
      ...INITIAL_SNAPSHOT,
      pending: false,
      processes: makeMap([makeProcess({ pid: 300, name: 'chrome.exe', flags: 1 << 6 })]),
    });
    render(<ProcessesScreen source={source} actions={actions} storage={memoryStorage()} />);
    await openMenuFor(300);
    fireEvent.click(screen.getByRole('menuitem', { name: 'Bring to front' }));
    expect(await screen.findByText(/has no window to show/)).toBeTruthy();
  });
});

describe('columns', () => {
  it('resizes a column by dragging its edge without sorting by the next one', async () => {
    mountScreen();
    await screen.findByRole('grid');
    const handle = screen.getByRole('separator', { name: 'PID' });
    const sortedBefore = document.querySelector('[aria-sort="descending"]')?.textContent;
    const before = Number(handle.getAttribute('aria-valuenow'));

    // happy-dom has no pointer capture; the handle must not depend on it
    // being honoured to receive its own moves.
    handle.setPointerCapture = () => undefined;
    handle.releasePointerCapture = () => undefined;
    fireEvent.pointerDown(handle, { button: 0, clientX: 100, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 160, pointerId: 1 });
    fireEvent.pointerUp(handle, { clientX: 160, pointerId: 1 });
    fireEvent.click(handle);

    await waitFor(() => expect(Number(handle.getAttribute('aria-valuenow'))).toBe(before + 60));
    expect(document.querySelector('[aria-sort="descending"]')?.textContent).toBe(sortedBefore);

    // Double-click returns it to its natural width.
    fireEvent.doubleClick(handle);
    await waitFor(() => expect(Number(handle.getAttribute('aria-valuenow'))).toBe(before));
  });
});
