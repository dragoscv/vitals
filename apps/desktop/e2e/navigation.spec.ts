import { readFileSync } from 'node:fs';
import path from 'node:path';

import { expect, test } from '@playwright/test';

import {
  expectNoProblems,
  openRoute,
  routes,
  settle,
  visibleRoute,
  watchProblems,
} from './support';

test('the e2e route list names exactly the routes the app declares', () => {
  // Read as text rather than imported: navigation.ts pulls lucide-react into
  // Node for no reason. A route added there without a row in support.ts
  // would otherwise simply never be tested, and nothing would say so.
  const source = readFileSync(
    path.resolve(import.meta.dirname, '../src/shell/navigation.ts'),
    'utf8',
  );
  const block = /export const routeIds = \[([\s\S]*?)\] as const/.exec(source)?.[1];
  expect(block, 'routeIds declaration not found in navigation.ts').toBeDefined();
  const declared = [...(block ?? '').matchAll(/'([A-Za-z]+)'/g)].map((m) => m[1]);
  expect(routes.map((r) => r.id)).toEqual(declared);
});

for (const route of routes) {
  test(`${route.id} renders its own screen with no console or page errors`, async ({ page }) => {
    const problems = watchProblems(page);
    await openRoute(page, route.id);

    await expect(route.landmark(visibleRoute(page))).toBeVisible();
    // The shell names the window after the active section; a mismatch means
    // the sidebar highlighted one route while the store committed another.
    await expect(page).toHaveTitle(`${route.label} — Vitals`);
    expectNoProblems(problems);
  });
}

test('visiting every section in one session keeps each screen intact and error-free', async ({
  page,
}) => {
  // Visited screens stay mounted, hidden, inside `<Activity>` (routes.tsx).
  // A screen that breaks only when another is mounted beside it, or when it
  // is hidden and shown again, passes every per-route test above.
  const problems = watchProblems(page);
  await page.goto('/');
  for (const route of [...routes, ...routes]) {
    await page.locator(`[data-nav-item="${route.id}"]`).click();
    await settle(page);
    await expect(route.landmark(visibleRoute(page))).toBeVisible();
    // Exactly one route visible: two would mean a hidden screen leaked out
    // of its Activity boundary.
    await expect(visibleRoute(page)).toHaveCount(1);
  }
  expectNoProblems(problems);
});
