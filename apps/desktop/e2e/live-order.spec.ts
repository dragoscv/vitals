import { expect, test, type Locator, type Page } from '@playwright/test';

import { installFakeHost, pushAgents } from './fakeHost';
import { openRoute } from './support';

/**
 * Live lists keep their places between ticks (S22-10).
 *
 * The owner's report: on Performance the readings "move from a row to
 * another", and on the AI page the agent cards "move too much". Both were
 * correct orders recomputed every second — a reading that skipped a tick
 * dropped its row and shifted every cell after it; the session list was
 * re-sorted by last activity on every push. A unit test sees one frame; only
 * a sequence of real frames on a laid-out page shows the movement.
 */

type Box = { readonly x: number; readonly y: number; readonly w: number; readonly h: number };

async function boxesOf(items: Locator, label: (el: Locator) => Promise<string>) {
  const out = new Map<string, Box>();
  for (const el of await items.all()) {
    const b = await el.boundingBox();
    if (b === null) continue;
    out.set(await label(el), {
      x: Math.round(b.x),
      y: Math.round(b.y),
      w: Math.round(b.width),
      h: Math.round(b.height),
    });
  }
  return out;
}

const NOW = Date.now();

function session(id: string, title: string, over: Record<string, unknown> = {}) {
  return {
    id,
    client: 'vscodeCopilot',
    host: 'local',
    title,
    workspace: 'E:\\gh\\vitals',
    repo: 'vitals',
    branch: 'main',
    model: 'codai',
    state: 'working',
    startedMs: NOW - 3_600_000,
    lastActivityMs: NOW - 5_000,
    activeMs: 900_000,
    turns: 12,
    userMessages: 3,
    toolCalls: 40,
    toolFailures: 0,
    currentTool: 'read_file',
    topTools: [],
    tokensIn: null,
    tokensOut: null,
    pid: null,
    ...over,
  };
}

function snapshot(sessions: readonly unknown[]) {
  return {
    takenAtMs: Date.now(),
    agentqInstalled: true,
    resources: [],
    leases: [],
    releases: [],
    jobs: [],
    marks: [],
    registry: [],
    cloudDown: [],
    machine: null,
    journal: [],
    stats: {
      windowHours: 24,
      runs: 0,
      failures: 0,
      commits: 0,
      cloudRuns: 0,
      waitP50Sec: null,
      waitP90Sec: null,
      holdP50Sec: null,
      holdP90Sec: null,
      waitTotalSec: 0,
      holdTotalSec: 0,
      byRepo: [],
      runsPerHour: Array.from({ length: 24 }, () => 0),
    },
    sessions,
  };
}

const TITLES = ['Alpha task', 'Bravo task', 'Charlie task'] as const;

async function cardBoxes(page: Page) {
  const route = page.locator('[data-route-visible]');
  return boxesOf(
    route.locator('li').filter({ hasText: /^(Alpha|Bravo|Charlie) task/ }),
    async (el) => (await el.locator('p').first().textContent()) ?? '',
  );
}

test.describe('live lists keep their places between ticks', () => {
  test.use({ viewport: { width: 1280, height: 900 } });

  test('Performance readings that skip a tick keep their cell, so no row moves', async ({
    page,
  }) => {
    await installFakeHost(page, { locale: 'en', flicker: true });
    await openRoute(page, 'performance');
    const panel = page.locator('[data-route-visible] .perf-panel[data-category="cpu"]');
    const cells = panel.locator('dl > div');
    await expect(cells.filter({ hasText: 'Package power' })).toHaveCount(1);
    const label = async (el: Locator) => (await el.locator('dt').textContent()) ?? '';

    const first = await boxesOf(cells, label);
    const rail = page.locator('[data-route-visible] nav li');
    const railCount = await rail.count();
    // Six frames: three with the readings, three without.
    for (let i = 0; i < 6; i += 1) {
      await page.waitForTimeout(1000);
      expect(await boxesOf(cells, label), `frame ${String(i)}`).toEqual(first);
      expect(await rail.count(), 'the Thermals entry stays in the rail').toBe(railCount);
    }
    // The flicker really happened: the dash replaced the reading on screen.
    await expect(panel.getByLabel('Not reported by your hardware').first()).toBeVisible({
      timeout: 3000,
    });
  });

  test('AI session cards hold their places while last activity reorders the list', async ({
    page,
  }) => {
    const sessions = TITLES.map((title, i) => session(`s${String(i)}`, title));
    await installFakeHost(page, { locale: 'en', agents: snapshot(sessions) });
    await openRoute(page, 'agents');
    await expect(page.getByText('Charlie task')).toBeVisible();
    await page.waitForTimeout(600);
    const before = await cardBoxes(page);
    expect(before.size).toBe(3);

    // Four pushes, each re-sorted by "most recent first" with new figures.
    for (let tick = 1; tick <= 4; tick += 1) {
      const order = tick % 2 === 0 ? [2, 0, 1] : [1, 2, 0];
      const pushed = order.map((i) =>
        session(`s${String(i)}`, TITLES[i] ?? '', {
          lastActivityMs: Date.now() - i * 100,
          turns: 20 + tick,
        }),
      );
      await pushAgents(page, snapshot(pushed));
      await expect(page.getByText(String(20 + tick)).first()).toBeVisible();
      await page.waitForTimeout(400);
      expect(await cardBoxes(page), `push ${String(tick)}`).toEqual(before);
    }
  });

  test('a session that stops working still moves below the working ones', async ({ page }) => {
    const sessions = TITLES.map((title, i) => session(`s${String(i)}`, title));
    await installFakeHost(page, { locale: 'en', agents: snapshot(sessions) });
    await openRoute(page, 'agents');
    const route = page.locator('[data-route-visible]');
    await route.getByRole('tab', { name: /Sessions/ }).click();
    await expect(route.getByText('Alpha task')).toBeVisible();

    await pushAgents(
      page,
      snapshot([
        session('s1', 'Bravo task'),
        session('s2', 'Charlie task'),
        session('s0', 'Alpha task', { state: 'idle', currentTool: null }),
      ]),
    );
    await expect
      .poll(async () => {
        const boxes = await cardBoxes(page);
        const y = (t: string) => boxes.get(t)?.y ?? Number.NaN;
        return y('Alpha task') > y('Bravo task') && y('Alpha task') >= y('Charlie task');
      })
      .toBe(true);
  });

  test('a card under the pointer stays put even when its session stops working', async ({
    page,
  }) => {
    const sessions = TITLES.map((title, i) => session(`s${String(i)}`, title));
    await installFakeHost(page, { locale: 'en', agents: snapshot(sessions) });
    await openRoute(page, 'agents');
    const route = page.locator('[data-route-visible]');
    await route.getByRole('tab', { name: /Sessions/ }).click();
    await expect(route.getByText('Alpha task')).toBeVisible();
    await page.waitForTimeout(400);
    const before = await cardBoxes(page);
    await route.getByText('Alpha task').hover();

    await pushAgents(
      page,
      snapshot([
        session('s1', 'Bravo task'),
        session('s2', 'Charlie task'),
        session('s0', 'Alpha task', { state: 'idle', currentTool: null, turns: 31 }),
      ]),
    );
    await expect(route.getByText('31').first()).toBeVisible();
    await page.waitForTimeout(400);
    expect(await cardBoxes(page), 'nothing slides out from under the pointer').toEqual(before);

    await page.mouse.move(2, 2);
    await expect
      .poll(async () => {
        const boxes = await cardBoxes(page);
        return (boxes.get('Alpha task')?.y ?? 0) > (boxes.get('Bravo task')?.y ?? 0);
      })
      .toBe(true);
  });
});
