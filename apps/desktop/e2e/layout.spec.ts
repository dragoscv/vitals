import { expect, test } from '@playwright/test';

import { openRoute, routes } from './support';

/**
 * The window sizes the layout rules were written against: the minimum width
 * (tauri.conf.json allows 720 × 520; 560 leaves room for the title bar the
 * webview does not draw), the force-collapsed sidebar below 900 px, the size
 * most laptops open at, and a full-HD monitor.
 */
const viewports = [
  { width: 720, height: 560 },
  { width: 1024, height: 640 },
  { width: 1280, height: 800 },
  { width: 1920, height: 1080 },
] as const;

for (const viewport of viewports) {
  test.describe(`at ${viewport.width}×${viewport.height}`, () => {
    test.use({ viewport });

    for (const route of routes) {
      test(`${route.id} fits the window so the document itself never scrolls`, async ({ page }) => {
        // The app's rule (styles.css, styles.test.ts): the title and toolbar
        // stay put and only a screen's own body scrolls. A document that
        // scrolls means some wrapper lost its height and the whole page grew
        // to its content — the Installed apps regression that scrolled the
        // search box away.
        await openRoute(page, route.id);

        const overflow = await page.evaluate(() => {
          const root = document.scrollingElement ?? document.documentElement;
          return {
            vertical: root.scrollHeight - root.clientHeight,
            horizontal: root.scrollWidth - root.clientWidth,
          };
        });

        // One pixel of slack for sub-pixel rounding of fractional rem sizes.
        expect(
          overflow.vertical,
          'document scrolls vertically by this many px',
        ).toBeLessThanOrEqual(1);
        expect(
          overflow.horizontal,
          'document scrolls horizontally by this many px',
        ).toBeLessThanOrEqual(1);
      });
    }
  });
}
