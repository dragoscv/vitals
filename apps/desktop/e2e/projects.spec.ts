import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

import { installFakeHost } from './fakeHost';
import { expectNoProblems, openRoute, settle, visibleRoute, watchProblems } from './support';

/** WCAG 2.2 AA, and serious/critical only — the same gate `a11y.spec.ts` runs. */
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
const BLOCKING = new Set(['serious', 'critical']);

/**
 * The Projects tab of the AI page, with a fake host behind it.
 *
 * The per-route specs run against the no-host fallback, where the AI page is a
 * single empty state and none of its tabs render. Here the fake host answers
 * `subscribe_agents` so the page draws, and the spec supplies `get_cloud_status`
 * itself — signed in, so the Projects tab shows the fixture the client ships
 * while `GET /v1/projects` is absent (the endpoint is not built yet; the tab is
 * a reader of a contract that is written down but not served).
 *
 * The suite runs against `vite dev`, so `import.meta.env.DEV` is true and the
 * fixture stands in for the endpoint. A production build returns the
 * `unavailable` state instead and shows no invented projects.
 */

const NOW = Date.now();

/** A minimal but complete AgentsSnapshot, so the AI page renders its tabs. */
function agentsSnapshot() {
  return {
    takenAtMs: NOW,
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
      cloudRuns: 3,
      waitP50Sec: null,
      waitP90Sec: null,
      holdP50Sec: null,
      holdP90Sec: null,
      waitTotalSec: 0,
      holdTotalSec: 0,
      byRepo: [],
      runsPerHour: Array.from({ length: 24 }, () => 0),
    },
    sessions: [],
  };
}

/**
 * Answers `get_cloud_status` for the page, wrapping the fake host's own
 * `invoke` so every other command is answered as before.
 */
async function answerCloudStatus(page: Page, signedIn: boolean): Promise<void> {
  await page.addInitScript((value: boolean) => {
    const internals = (
      window as unknown as {
        __TAURI_INTERNALS__?: {
          invoke?: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
        };
      }
    ).__TAURI_INTERNALS__;
    if (internals === undefined) return;
    const original = internals.invoke;
    internals.invoke = (cmd, args) =>
      cmd === 'get_cloud_status'
        ? Promise.resolve({ signedIn: value, hosts: [], error: null })
        : typeof original === 'function'
          ? original(cmd, args)
          : Promise.reject(new Error('unsupported'));
  }, signedIn);
}

test('shows a signed-in user their project list, detail and the devcloud panel', async ({
  page,
}) => {
  const problems = watchProblems(page);
  await installFakeHost(page, { locale: 'en', agents: agentsSnapshot() });
  await answerCloudStatus(page, true);
  await openRoute(page, 'agents');

  await page.getByRole('tab', { name: 'Projects' }).click();

  const panel = visibleRoute(page).getByRole('region', { name: 'Codwen projects' });
  // All four projects are listed, worst-first, and the first row is selected.
  for (const name of [
    'Invoice worker',
    'Checkout API',
    'Public status page',
    'Developer documentation and API reference site',
  ]) {
    await expect(panel.getByText(name).first()).toBeVisible();
  }
  const firstRow = panel.locator('button[aria-pressed]').first();
  await expect(firstRow).toHaveAttribute('aria-pressed', 'true');
  await expect(firstRow).toContainText('Invoice worker');
  await expect(firstRow).toContainText('Down');
  // Present is not painted: Playwright counts an element at `opacity: 0` as
  // visible, so a row whose entrance animation never resolved would pass every
  // assertion above while the user sees nothing.
  await expect
    .poll(() => firstRow.evaluate((el) => Number(getComputedStyle(el).opacity)), {
      timeout: 5_000,
    })
    .toBe(1);

  // A component the fixture did not measure renders an em dash, never 0 ms.
  await expect(panel.getByText(/Latency: —/).first()).toBeVisible();
  // The selected project's incident is shown.
  await expect(
    panel.getByText('Invoice worker has not answered a probe for six minutes.'),
  ).toBeVisible();

  const devcloud = panel.getByRole('region', { name: 'Devcloud workspaces' });
  await expect(devcloud).toBeVisible();
  await expect(devcloud.getByText('3 cloud runs in the last 24 h')).toBeVisible();

  expectNoProblems(problems);
});

test('says to sign in, never an empty list, when no Codwen account is stored', async ({ page }) => {
  const problems = watchProblems(page);
  await installFakeHost(page, { locale: 'en', agents: agentsSnapshot() });
  await answerCloudStatus(page, false);
  await openRoute(page, 'agents');

  await page.getByRole('tab', { name: 'Projects' }).click();

  const panel = visibleRoute(page).getByRole('region', { name: 'Codwen projects' });
  await expect(panel.getByText(/Sign in to Codwen/)).toBeVisible();
  // An empty list would read as "no projects", which is a different fact.
  await expect(panel.getByText('No projects yet')).toHaveCount(0);
  expectNoProblems(problems);
});

test('the Projects tab has no serious or critical WCAG 2.2 AA violations', async ({ page }) => {
  // The scan is of the settled screen, and the rows fade in. A contrast reading
  // taken mid-fade is a reading of the fade: an element at 2 % opacity
  // composites to near-white on a near-white card and reports 1.01:1, which the
  // same screen passes a moment later. The shell maps the operating system's
  // preference onto Motion (`MotionConfig reducedMotion="user"` in
  // `theme/MotionProvider.tsx`, which is what `reduceMotion: null` means), so
  // asking for reduce-motion removes the transient rather than the check.
  //
  // It has to be asked for before the screen is opened, because the animation
  // starts with the click.
  await page.emulateMedia({ reducedMotion: 'reduce' });

  await installFakeHost(page, { locale: 'en', agents: agentsSnapshot() });
  await answerCloudStatus(page, true);
  await openRoute(page, 'agents');
  await page.getByRole('tab', { name: 'Projects' }).click();
  await expect(visibleRoute(page).getByText('Invoice worker').first()).toBeVisible();
  await settle(page);

  const panel = visibleRoute(page).getByRole('region', { name: 'Codwen projects' });
  const row = panel.locator('button[aria-pressed]').first();

  // Belt and braces for anything the preference did not remove - a colour
  // transition, a scroll animation - because measuring one is measuring the
  // transition.
  //
  // Polled, not awaited: `animation.finished` never resolves for a loop, and
  // this screen has three of them - the live pulses on the agent list and the
  // collaboration view are Motion transitions with `repeat: Infinity`. Awaiting
  // every animation spends the whole 60 s budget on something the scan does not
  // depend on, and reports a timeout instead of a contrast result.
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            document
              .getAnimations()
              .filter((animation) => animation.effect?.getComputedTiming().iterations !== Infinity)
              .filter((animation) => animation.playState === 'running').length,
        ),
      { timeout: 5_000 },
    )
    .toBe(0);

  // And the row is painted, not merely present. A scan taken while the row is
  // still fading reads the fade: the composited colours of a layer at 2 %
  // opacity are near-white on near-white, which is where the 1.01:1 ratios in
  // the first run of this test came from.
  await expect
    .poll(() => row.evaluate((el) => Number(getComputedStyle(el).opacity)), { timeout: 5_000 })
    .toBe(1);

  // The whole document, not only the panel: the sidebar and title bar are on
  // every screen, and a contrast failure there is a failure everywhere.
  const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
  const blocking = results.violations.filter(
    (violation) => violation.impact != null && BLOCKING.has(violation.impact),
  );
  const report = blocking
    .map(
      (violation) =>
        `[${violation.impact ?? '?'}] ${violation.id}: ${violation.help}\n  ${violation.helpUrl}\n${violation.nodes
          .map((node) => `    ${node.target.join(' ')}`)
          .join('\n')}`,
    )
    .join('\n\n');

  expect(blocking, report).toEqual([]);
});
