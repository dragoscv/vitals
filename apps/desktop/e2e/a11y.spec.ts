import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

import { installFakeHost } from './fakeHost';
import { openRoute, routes, visibleRoute } from './support';

/** WCAG 2.2 AA, which the repo's accessibility rule commits to. */
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

/**
 * Serious and critical only. Moderate and minor findings are real, but they
 * are mostly best-practice advice, and a gate that fails on them gets
 * disabled rather than fixed. The blocking set is what stops someone using
 * the screen at all: missing names, unreadable contrast, broken ARIA.
 */
const BLOCKING = new Set(['serious', 'critical']);

/** A defect accepted on purpose, pinned so it cannot hide a second one. */
interface KnownViolation {
  /** axe rule id, e.g. `color-contrast`. */
  readonly id: string;
  /** How many offending nodes it must have, so a new node is not absorbed. */
  readonly nodes: number;
}

/**
 * Runs an axe scan and fails with every blocking violation it found.
 *
 * `include` narrows the scan to one element — a dialog, an overlay panel — so
 * a surface that is only part of the document is still checked on its own.
 * `allow` names a defect that is real but lives outside this suite's reach;
 * it is matched on rule AND node count, so a second node failing the same rule
 * is still a failure.
 * Kept in one place so the violation report is formatted once and every scan,
 * whole-page or scoped, asserts the same set.
 */
async function expectNoBlockingViolations(
  page: Page,
  options: { include?: string; allow?: readonly KnownViolation[] } = {},
): Promise<void> {
  let builder = new AxeBuilder({ page }).withTags(TAGS);
  if (options.include !== undefined) builder = builder.include(options.include);
  const results = await builder.analyze();
  const allow = options.allow ?? [];
  const blocking = results.violations.filter(
    (violation) =>
      violation.impact != null &&
      BLOCKING.has(violation.impact) &&
      !allow.some((known) => known.id === violation.id && known.nodes === violation.nodes.length),
  );

  const report = blocking
    .map((violation) => {
      const nodes = violation.nodes
        .map((node) => `    ${node.target.join(' ')}\n      ${node.failureSummary ?? ''}`)
        .join('\n');
      return `[${violation.impact ?? '?'}] ${violation.id}: ${violation.help}\n  ${violation.helpUrl}\n${nodes}`;
    })
    .join('\n\n');

  expect(blocking, report).toEqual([]);
}

for (const route of routes) {
  test(`${route.id} has no serious or critical WCAG 2.2 AA violations`, async ({ page }) => {
    await openRoute(page, route.id);

    // The whole document, not only the screen: the sidebar and title bar are
    // on every screen, and a contrast failure there is a failure everywhere.
    await expectNoBlockingViolations(page);
  });
}

/**
 * A verdict with something to act on: a CPU alert, two named contributors and
 * a secondary finding, so the "Show in Processes" buttons and the responsible
 * list are on screen for the scan. Field names mirror `Diagnosis` in
 * `@vitals/protocol`, which is generated from the Rust type.
 */
const DIAGNOSIS = {
  verdict: {
    alert: {
      kind: 'cpuSustained',
      severity: 'warning',
      subject: '',
      title: 'alert.cpuSustained.title',
      cause: 'alert.cpuSustained.cause',
      values: { percent: 91, seconds: 15 },
      route: 'processes',
      sinceSample: 1,
    },
    subsystem: 'cpu',
    contributors: [
      { name: 'chrome.exe', pid: 4242, share: 61, value: 5560 },
      { name: 'code.exe', pid: 77, share: 22, value: 2010 },
    ],
    diffuse: false,
  },
  also: [
    {
      kind: 'diskSpace',
      severity: 'info',
      subject: 'C:',
      title: 'alert.diskSpace.title',
      cause: 'alert.diskSpace.cause',
      values: { disk: 'C:', percent: 4 },
      route: 'storage',
      sinceSample: 1,
    },
  ],
};

test('the diagnosis dialog has no serious or critical WCAG 2.2 AA violations', async ({ page }) => {
  await installFakeHost(page, { locale: 'en', diagnosis: DIAGNOSIS });
  await openRoute(page, 'dashboard');

  // The question is asked from the dashboard's front door, which stays closed
  // until the first frame arrives — the fake host sends one, so it opens.
  const ask = visibleRoute(page).getByRole('button', { name: 'Why is my PC slow?' });
  await expect(ask).toBeEnabled();
  await ask.click();

  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  // The verdict, not the "Looking…" spinner: the dialog is only worth scanning
  // once the answer it exists to show has rendered.
  await expect(dialog.getByText('chrome.exe')).toBeVisible();

  await expectNoBlockingViolations(page, { include: '[role="dialog"]' });
});

test('the HUD overlay has no serious or critical WCAG 2.2 AA violations', async ({ page }) => {
  // The overlay subscribes to the same `vitals://frame` event the desktop does,
  // so the fake host gives it real readings rather than three em dashes.
  await installFakeHost(page, { locale: 'en' });
  await page.goto('/hud.html');
  // Three numbers and a toolbar that is hidden until hover — present, focusable
  // and readable to a screen reader even while the mouse is elsewhere.
  await expect(page.getByRole('button', { name: 'Hide the overlay' })).toBeVisible();

  // Both the overlay and its page are transparent on purpose (it floats over
  // the desktop), so axe has nothing to composite the 82 %-opaque panel against
  // and assumes white — a canvas the overlay never sits on. Paint a
  // representative dark desktop (the product's own base colour) so the contrast
  // rule measures the panel as it is actually seen.
  await page.addStyleTag({ content: 'html,body{background:#1c1c1f !important}' });

  await expectNoBlockingViolations(page);
});

test('the phone page with a paired PC has no serious or critical WCAG 2.2 AA violations', async ({
  page,
}) => {
  // The phone reads its pairings from localStorage rather than the Tauri host,
  // so the fixture installer seeds them: a pairing makes the page render a
  // machine card instead of its empty state. The PC itself is never reached —
  // the card shows its connection state, which is all the scan needs.
  await installFakeHost(page, {
    locale: 'en',
    pairings: [
      {
        id: 'http://127.0.0.1:9',
        baseUrl: 'http://127.0.0.1:9',
        token: 'a-pairing-token-for-the-accessibility-scan',
        name: 'DESK-01',
        addedAt: 0,
        readOnly: true,
      },
    ],
  });
  await page.goto('/mobile.html');
  await expect(page.getByRole('heading', { name: 'Machines' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'DESK-01' })).toBeVisible();

  // Nothing is allowed here on purpose. The active tab draws its label with
  // `--color-accent-text` rather than the fill accent, because the fill accent
  // measured 4.08:1 on `bg-subtle` in the light theme (`#22864a` on `#f0f2f4`) —
  // below AA's 4.5:1 at 11 px. Switching it back must fail this test, not be
  // absorbed by an allowance.
  await expectNoBlockingViolations(page);
});
