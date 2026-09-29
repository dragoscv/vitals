import { expect, type Locator, type Page } from '@playwright/test';

/**
 * Every section of the desktop app with its English sidebar label.
 *
 * Kept in step with `routeIds` in src/shell/navigation.ts by the
 * `covers every route` test in navigation.spec.ts, which reads that file.
 * Imported by value it would drag lucide-react and the whole shell into the
 * Playwright process for the sake of twelve strings.
 *
 * `label` is the `nav.*` key in packages/i18n/src/locales/en.json, asserted
 * against `document.title`, which the shell sets from the active section.
 *
 * `landmark` is what proves the screen itself rendered rather than the
 * shell around an empty slot: the screen's own `<h2>` title from its
 * feature's strings.ts. Processes has no title — its toolbar is the top of
 * the screen — so its filter box, named by `aria-label`, stands in.
 */
export const routes = [
  { id: 'dashboard', label: 'Dashboard', landmark: heading('Dashboard') },
  { id: 'performance', label: 'Performance', landmark: heading('Performance') },
  {
    id: 'processes',
    label: 'Processes',
    landmark: (scope: Locator) =>
      scope.getByRole('textbox', { name: 'Filter processes' }).or(noHost(scope)).first(),
  },
  { id: 'startup', label: 'Startup', landmark: heading('Startup apps') },
  { id: 'services', label: 'Services', landmark: heading('Services') },
  { id: 'appHistory', label: 'App history', landmark: heading('App history') },
  { id: 'users', label: 'Users', landmark: heading('Users') },
  { id: 'network', label: 'Network', landmark: heading('Network connections') },
  { id: 'storage', label: 'Storage', landmark: heading('Storage') },
  { id: 'installedApps', label: 'Installed apps', landmark: heading('Installed apps') },
  { id: 'devices', label: 'Devices & sensors', landmark: heading('Devices & sensors') },
  { id: 'benchmarks', label: 'Benchmarks', landmark: heading('Benchmarks') },
] as const;

function heading(name: string): (scope: Locator) => Locator {
  // Level 2 and exact: several screens carry card headings that contain
  // the section's name ("Performance" vs a card called "CPU performance").
  // `.first()`: the dashboard keeps its title AND shows the status below it.
  return (scope) =>
    scope.getByRole('heading', { level: 2, name, exact: true }).or(noHost(scope)).first();
}

/**
 * The screen's own "no readings" state. Without a Tauri host most screens
 * replace their whole body — title included — with this status, by design
 * (every read answers at once with a documented absence). Seen live on
 * Storage, 2026-09-29. It is still the screen rendering, not the shell's
 * crash fallback, which `settle` rules out separately.
 */
function noHost(scope: Locator): Locator {
  // The body sentence, not the title: Users says "No sessions are arriving"
  // where the rest say "No readings", but every one explains itself with
  // this phrase.
  return scope.getByRole('status').filter({ hasText: 'Vitals cannot reach the part of itself' });
}

export type RouteId = (typeof routes)[number]['id'];

/**
 * Console output that is not a defect, each with the reason it is tolerated.
 *
 * Empty on purpose. Add an entry only with a comment naming why the message
 * is expected in the no-host fallback; a pattern broad enough to hide a real
 * failure is worse than a red test.
 */
const BENIGN_CONSOLE: readonly RegExp[] = [];

export interface PageProblems {
  /** Every `console.error` and uncaught exception so far, in order. */
  readonly messages: string[];
}

/**
 * Starts recording console errors and uncaught page errors.
 *
 * Attach before `goto`: the errors most worth catching are the ones thrown
 * while the entry chunk boots, and a listener added afterwards never sees them.
 */
export function watchProblems(page: Page): PageProblems {
  const messages: string[] = [];
  page.on('console', (message) => {
    if (message.type() !== 'error') return;
    const text = message.text();
    if (BENIGN_CONSOLE.some((pattern) => pattern.test(text))) return;
    const where = message.location();
    messages.push(`console.error: ${text} (${where.url}:${where.lineNumber})`);
  });
  page.on('pageerror', (error) => {
    messages.push(`pageerror: ${error.name}: ${error.message}\n${error.stack ?? ''}`);
  });
  return { messages };
}

/** Fails with every recorded message, so one run shows all of them. */
export function expectNoProblems(problems: PageProblems): void {
  expect(problems.messages, problems.messages.join('\n\n')).toEqual([]);
}

/** The one route on screen. Hidden, visited routes stay mounted beside it. */
export function visibleRoute(page: Page): Locator {
  return page.locator('[data-route-visible]');
}

/**
 * Opens the app and moves to `route` the way a user does, through the sidebar.
 *
 * The sidebar rather than any URL: the shell keeps the route in its settings
 * store, not in the address bar, and without a Tauri host nothing persists,
 * so every fresh page starts on the dashboard. `data-nav-item` is used rather
 * than the button's name because a collapsed sidebar (below 900 px) names it
 * with `aria-label` and hides the text, and the tests run at both widths.
 */
export async function openRoute(page: Page, route: RouteId): Promise<void> {
  await page.goto('/');
  const item = page.locator(`[data-nav-item="${route}"]`);
  await item.click();
  await expect(item).toHaveAttribute('aria-current', 'page');
  await settle(page);
}

/**
 * Waits until the visible screen has finished loading.
 *
 * A skeleton (`aria-busy`) is still standing in for the content; measuring
 * or scanning it would test the placeholder rather than the screen. The
 * shell's crash fallback is checked here too, because it renders a perfectly
 * accessible, perfectly laid-out empty state that every other assertion would
 * happily pass.
 */
export async function settle(page: Page): Promise<void> {
  const route = visibleRoute(page);
  await expect(route).toBeVisible();
  // 15 s, not the 5 s default: a lazy chunk transformed cold by Vite on a
  // machine at 85 % CPU took longer than 5 s to replace its skeleton, while
  // the same screen settles in under a second idle.
  await expect(route.locator('[aria-busy="true"]')).toHaveCount(0, { timeout: 15_000 });
  await expect(route.getByText('This section stopped working')).toHaveCount(0);
}
