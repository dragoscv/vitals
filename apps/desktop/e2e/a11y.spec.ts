import AxeBuilder from '@axe-core/playwright';
import { expect, test } from '@playwright/test';

import { openRoute, routes } from './support';

/** WCAG 2.2 AA, which the repo's accessibility rule commits to. */
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];

/**
 * Serious and critical only. Moderate and minor findings are real, but they
 * are mostly best-practice advice, and a gate that fails on them gets
 * disabled rather than fixed. The blocking set is what stops someone using
 * the screen at all: missing names, unreadable contrast, broken ARIA.
 */
const BLOCKING = new Set(['serious', 'critical']);

for (const route of routes) {
  test(`${route.id} has no serious or critical WCAG 2.2 AA violations`, async ({ page }) => {
    await openRoute(page, route.id);

    // The whole document, not only the screen: the sidebar and title bar are
    // on every screen, and a contrast failure there is a failure everywhere.
    const results = await new AxeBuilder({ page }).withTags(TAGS).analyze();
    const blocking = results.violations.filter(
      (violation) => violation.impact != null && BLOCKING.has(violation.impact),
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
  });
}
