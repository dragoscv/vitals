import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';

import { expect, test, type Page } from '@playwright/test';

import { installFakeHost } from './fakeHost';
import { openRoute, routes } from './support';

/**
 * Layout with content on screen: no text spills out of its box, and the wheel
 * is never trapped by an inner panel that has nothing left to scroll (S22-04).
 *
 * layout.spec.ts proves the document never scrolls, but against the no-host
 * fallback, where every screen is an empty state. The two defects this file
 * exists for only show with data: Romanian labels a third longer than the
 * English ones, and a Storage page whose cards each swallowed the wheel.
 *
 * `VITALS_SHOTS=<dir>` also saves a screenshot per page, width and locale.
 */

const widths = [900, 1280, 1920] as const;
const locales = ['en', 'ro'] as const;
const shots = process.env['VITALS_SHOTS'];

interface Spill {
  readonly what: string;
  readonly text: string;
  readonly by: number;
}

/**
 * Every visible element whose own text is wider than its box and is not
 * deliberately truncated, plus every text element that pokes out of the card
 * or panel it sits in.
 *
 * "Deliberately truncated" means `text-overflow: ellipsis`: the text is cut
 * with a visible ellipsis and (by convention here) a `title`. A box that clips
 * text with no ellipsis, or lets it paint over its neighbour, is the defect.
 */
async function findSpills(page: Page): Promise<Spill[]> {
  return page.evaluate(() => {
    const root = document.querySelector('[data-route-visible]') ?? document.body;
    const out: { what: string; text: string; by: number }[] = [];
    const describe = (el: Element) => {
      const cls =
        typeof el.className === 'string' ? el.className.split(/\s+/).slice(0, 4).join('.') : '';
      return `${el.tagName.toLowerCase()}${cls ? `.${cls}` : ''}`;
    };
    const ownText = (el: Element) =>
      Array.from(el.childNodes)
        .filter((n) => n.nodeType === Node.TEXT_NODE)
        .map((n) => n.textContent ?? '')
        .join('')
        .trim();
    const boxed = (el: Element): Element | null => {
      // The nearest ancestor that draws a box (border or background) or
      // clips: what a reader sees as "the container".
      for (let p = el.parentElement; p !== null && p !== root.parentElement; p = p.parentElement) {
        const s = getComputedStyle(p);
        if (
          s.overflowX !== 'visible' ||
          s.borderLeftWidth !== '0px' ||
          s.backgroundColor !== 'rgba(0, 0, 0, 0)'
        )
          return p;
      }
      return null;
    };
    for (const el of Array.from(root.querySelectorAll('*'))) {
      if (el.closest('[aria-hidden="true"], svg, canvas, [role="tooltip"]')) continue;
      const text = ownText(el);
      if (text === '') continue;
      const rect = el.getBoundingClientRect();
      // Visually hidden text (`sr-only`, Radix's VisuallyHidden) is a 1 px
      // clipped box by design: it is read aloud, never seen.
      if (rect.width <= 2 || rect.height <= 2) continue;
      const style = getComputedStyle(el);
      if (style.visibility === 'hidden' || style.position === 'fixed' || style.clip !== 'auto')
        continue;
      const truncated = style.textOverflow === 'ellipsis';
      // An ellipsis is fine on a long name; on a title cut to a few letters
      // ("ZON…" for "Zone termice") it is a layout that gave the text no room.
      const fontPx = Number.parseFloat(style.fontSize) || 12;
      if (truncated && el.scrollWidth > el.clientWidth + 1 && el.clientWidth < fontPx * 5) {
        out.push({
          what: `${describe(el)} squeezed`,
          text: text.slice(0, 60),
          by: el.scrollWidth - el.clientWidth,
        });
        continue;
      }
      if (!truncated && el.scrollWidth > el.clientWidth + 1 && style.display !== 'inline') {
        out.push({
          what: describe(el),
          text: text.slice(0, 60),
          by: el.scrollWidth - el.clientWidth,
        });
        continue;
      }
      const box = boxed(el);
      if (box === null) continue;
      const b = box.getBoundingClientRect();
      // Content scrolled out of a scroller is not spilling.
      if (getComputedStyle(box).overflowX !== 'visible' && box.scrollWidth > box.clientWidth)
        continue;
      const by = Math.max(rect.right - b.right, b.left - rect.left);
      if (by > 1 && rect.top < b.bottom && rect.bottom > b.top) {
        out.push({
          what: `${describe(el)} out of ${describe(box)}`,
          text: text.slice(0, 60),
          by: Math.round(by),
        });
      }
    }
    return out;
  });
}

for (const locale of locales) {
  for (const width of widths) {
    test.describe(`${locale} at ${String(width)}px`, () => {
      test.use({ viewport: { width, height: 800 } });

      for (const route of routes) {
        test(`${route.id} keeps every label inside its box`, async ({ page }) => {
          await installFakeHost(page, { locale });
          await openRoute(page, route.id);
          // `settle` looks for the English crash text; this runs in Romanian too.
          await expect(
            page
              .locator('[data-route-visible]')
              .getByText('Această secțiune a încetat să funcționeze'),
          ).toHaveCount(0);
          if (route.id === 'storage') await scanFirstDrive(page);
          if (shots !== undefined) {
            const dir = resolve(shots, locale);
            mkdirSync(dir, { recursive: true });
            await page.screenshot({ path: resolve(dir, `${route.id}-${String(width)}.png`) });
          }
          const spills = await findSpills(page);
          expect(
            spills.map((s) => `${s.what} | ${s.text} | +${String(s.by)}px`),
            'text wider than its box, or outside its container',
          ).toEqual([]);
        });
      }
    });
  }
}

async function scanFirstDrive(page: Page): Promise<void> {
  const route = page.locator('[data-route-visible]');
  await route.getByRole('button', { name: /^(Scan this drive|Scanează această unitate)$/ }).click();
  await expect(route.locator('canvas[role="img"]')).toBeVisible();
}

test.describe('the wheel over an inner panel', () => {
  test.use({ viewport: { width: 1280, height: 720 } });

  test('scrolls the Storage page when the panel under the pointer has nothing to scroll', async ({
    page,
  }) => {
    await installFakeHost(page, { locale: 'en' });
    await openRoute(page, 'storage');
    await scanFirstDrive(page);
    const route = page.locator('[data-route-visible]');
    const body = route.locator('.screen-body');
    const top = () => body.evaluate((el) => el.scrollTop);

    // The Reclaimable-space card before a check: a short card body that is a
    // scroll container with no overflow. It must pass the wheel on.
    const card = route.locator('.pane-stack .pane-scroll').first();
    await card.scrollIntoViewIfNeeded();
    expect(await card.evaluate((el) => el.scrollHeight - el.clientHeight)).toBeLessThanOrEqual(1);
    await body.evaluate((el) => {
      el.scrollTop = Math.max(0, el.scrollTop - 200);
    });
    const box = await card.boundingBox();
    expect(box, 'the clean-up card is on screen').not.toBeNull();
    if (box === null) return;
    await page.mouse.move(box.x + box.width / 2, Math.min(Math.max(box.y + 10, 5), 715));
    const before = await top();
    await page.mouse.wheel(0, 400);
    await expect.poll(top).toBeGreaterThan(before);
  });

  test('scrolls the Storage page once a long list is at its end', async ({ page }) => {
    await installFakeHost(page, { locale: 'en' });
    await openRoute(page, 'storage');
    await scanFirstDrive(page);
    const route = page.locator('[data-route-visible]');
    const body = route.locator('.screen-body');
    const list = route.locator('.pane-scroll:has(ul)').first();
    await list.scrollIntoViewIfNeeded();
    await list.evaluate((el) => {
      el.scrollTop = el.scrollHeight;
    });
    await body.evaluate((el) => {
      el.scrollTop = Math.max(0, el.scrollTop - 200);
    });
    const box = await list.boundingBox();
    expect(box, 'the folder list is on screen').not.toBeNull();
    if (box === null) return;
    await page.mouse.move(box.x + box.width / 2, Math.min(Math.max(box.y + 10, 5), 715));
    const before = await body.evaluate((el) => el.scrollTop);
    await page.mouse.wheel(0, 400);
    await expect.poll(() => body.evaluate((el) => el.scrollTop)).toBeGreaterThan(before);
  });
});
