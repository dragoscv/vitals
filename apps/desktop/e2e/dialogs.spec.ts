import { expect, test, type Locator, type Page } from '@playwright/test';

/**
 * The widest a dialog may be.
 *
 * Mirrors `DIALOG_MAX_WIDTH_REM` in packages/ui/src/components/Dialog.tsx.
 * Written out rather than imported for the reason given in `support.ts`:
 * importing the UI package by value would drag React, Tailwind's CSS and the
 * whole component tree into the Playwright process for one number. The unit
 * test in Dialog.test.tsx is what keeps that constant honest, and this file is
 * what proves the number is what the browser actually renders — a class list
 * cannot show that, because the app redefines the theme tokens those classes
 * resolve through (see the `--container-3xl` note in src/styles.css).
 */
const MAX_DIALOG_WIDTH_PX = 48 * 16;

/** The dialog is capped to the window with 1rem of clearance on each side. */
const WINDOW_MARGIN_PX = 16;

/** One pixel of slack for sub-pixel rounding of fractional rem sizes. */
const SLACK_PX = 1;

/**
 * The ultrawide end is the regression: the container scale was widened for
 * 1920 px-and-up panels, and that is the same token `max-w-3xl` used to read,
 * so a fullscreen window was where the settings dialog stretched.
 */
const viewports = [
  { width: 1280, height: 720 },
  { width: 1920, height: 1080 },
  { width: 3440, height: 900 },
] as const;

interface DialogBox {
  readonly width: number;
  readonly height: number;
  readonly top: number;
  readonly bottom: number;
  readonly left: number;
  readonly right: number;
  readonly viewportWidth: number;
  readonly viewportHeight: number;
  readonly documentVerticalOverflow: number;
  readonly documentHorizontalOverflow: number;
  readonly bodyVerticalOverflow: number;
}

/**
 * The app, freshly loaded. Kept separate from opening the dialog because
 * without a Tauri host nothing is persisted: a `goto` reloads the shell and
 * resets the sidebar to expanded, which would undo a collapse a test is in the
 * middle of exercising.
 */
async function gotoApp(page: Page): Promise<void> {
  await page.goto('/');
  await page.waitForSelector('[data-nav-item]');
}

/**
 * The Settings dialog: the widest in the app, and the one that was reported.
 * Opened the way a user opens it, through the sidebar, whose button is named
 * "Settings" expanded and "Open settings" when the sidebar has collapsed.
 */
async function openViaSidebar(page: Page): Promise<Locator> {
  // One name matcher for both states rather than `.or(...).first()`: the
  // combined locator resolved to a box and accepted the click without the
  // button ever receiving it, and the dialog never opened.
  const trigger = page.getByRole('button', { name: /^(Settings|Open settings)$/ });
  const dialog = page.getByRole('dialog');

  // The dialog is behind a dynamic import, so the first click of a run waits on
  // Vite transforming the whole settings tree (ten panels plus the LAN, MQTT
  // and health clients), and a click landing in the first few hundred
  // milliseconds after the sidebar appears can be lost outright: measured
  // against the dev server, ready at 440 ms and the dialog at 1464 ms, but
  // "never opened" on several cold runs. Retrying separates that from a real
  // layout failure rather than reporting the assertion timeout as one — with
  // the default 15 s budget this test would be measuring the transform cache.
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    await trigger.click();
    const opened = await dialog
      .waitFor({ state: 'visible', timeout: 20_000 })
      .then(() => true)
      .catch(() => false);
    if (opened) return dialog;
    await page.waitForSelector('[data-nav-item]');
  }
  await expect(dialog, 'the settings dialog never opened').toBeVisible({ timeout: 30_000 });
  return dialog;
}

async function openSettings(page: Page): Promise<Locator> {
  await gotoApp(page);
  return openViaSidebar(page);
}

async function readBox(page: Page): Promise<DialogBox> {
  return page.evaluate(() => {
    const dialog = document.querySelector('[role="dialog"]');
    if (dialog === null) throw new Error('no dialog is open');
    const rect = dialog.getBoundingClientRect();
    const body = document.querySelector('[data-dialog-body]');
    const root = document.scrollingElement ?? document.documentElement;
    return {
      width: rect.width,
      height: rect.height,
      top: rect.top,
      bottom: rect.bottom,
      left: rect.left,
      right: rect.right,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      documentVerticalOverflow: root.scrollHeight - root.clientHeight,
      documentHorizontalOverflow: root.scrollWidth - root.clientWidth,
      bodyVerticalOverflow: body === null ? 0 : body.scrollHeight - body.clientHeight,
    };
  });
}

for (const viewport of viewports) {
  test(`at ${viewport.width}×${viewport.height} a dialog stays within its documented maximum width`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await openSettings(page);
    const box = await readBox(page);

    expect(
      box.width,
      `the dialog rendered ${Math.round(box.width)}px wide, past the ${MAX_DIALOG_WIDTH_PX}px maximum`,
    ).toBeLessThanOrEqual(MAX_DIALOG_WIDTH_PX + SLACK_PX);
    expect(
      box.width,
      'the dialog is wider than the window minus its clearance, so it overhangs the edge',
    ).toBeLessThanOrEqual(box.viewportWidth - 2 * WINDOW_MARGIN_PX + SLACK_PX);
    expect(box.width, 'the dialog collapsed to nothing').toBeGreaterThan(0);
    expect(
      box.documentHorizontalOverflow,
      'opening the dialog made the document itself scroll sideways',
    ).toBeLessThanOrEqual(SLACK_PX);
    expect(
      box.documentVerticalOverflow,
      'opening the dialog made the document itself scroll vertically',
    ).toBeLessThanOrEqual(SLACK_PX);
  });
}

test('a dialog taller than the window scrolls its body instead of pushing its content off screen', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await openSettings(page);

  // Stand in for a long panel (drive clean-up, a settings tab with a table):
  // taller than the cap, so the dialog has to decide what gives.
  await page.evaluate(() => {
    const body = document.querySelector('[data-dialog-body]');
    if (body === null) throw new Error('no dialog body to fill');
    const filler = document.createElement('div');
    filler.style.height = '3000px';
    filler.setAttribute('data-filler', '');
    body.append(filler);
  });

  const box = await readBox(page);

  expect(
    box.height,
    `the dialog grew to ${Math.round(box.height)}px, taller than the ${box.viewportHeight}px window`,
  ).toBeLessThanOrEqual(box.viewportHeight - 2 * WINDOW_MARGIN_PX + SLACK_PX);
  expect(
    box.top,
    'the dialog starts above the window, so its header is cut off',
  ).toBeGreaterThanOrEqual(-SLACK_PX);
  expect(
    box.bottom,
    'the dialog runs past the bottom of the window, so its actions are off screen',
  ).toBeLessThanOrEqual(box.viewportHeight + SLACK_PX);
  expect(
    box.bodyVerticalOverflow,
    'the content did not scroll inside the dialog, so it must have been clipped or pushed out',
  ).toBeGreaterThan(0);
});

test('at 1280×720 collapsing the sidebar neither clips the dialog nor scrolls the document sideways', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await gotoApp(page);

  const boxes = [];
  for (const collapse of [false, true]) {
    if (collapse) {
      await page.getByRole('button', { name: 'Collapse sidebar' }).click();
    }
    await openViaSidebar(page);
    boxes.push({ collapse, box: await readBox(page) });
    await page.keyboard.press('Escape');
    await expect(page.getByRole('dialog')).toHaveCount(0);
  }

  for (const { collapse, box } of boxes) {
    const state = collapse ? 'collapsed' : 'expanded';
    expect(
      box.width,
      `with the sidebar ${state} the dialog is wider than its maximum`,
    ).toBeLessThanOrEqual(MAX_DIALOG_WIDTH_PX + SLACK_PX);
    expect(
      box.right,
      `with the sidebar ${state} the dialog overhangs the right edge`,
    ).toBeLessThanOrEqual(box.viewportWidth + SLACK_PX);
    expect(
      box.left,
      `with the sidebar ${state} the dialog overhangs the left edge`,
    ).toBeGreaterThanOrEqual(-SLACK_PX);
    expect(
      box.documentHorizontalOverflow,
      `with the sidebar ${state} the document scrolls sideways — a form would be cut off`,
    ).toBeLessThanOrEqual(SLACK_PX);
    expect(
      box.documentVerticalOverflow,
      `with the sidebar ${state} the document scrolls vertically`,
    ).toBeLessThanOrEqual(SLACK_PX);
  }
});
