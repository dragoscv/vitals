/**
 * The widget window's accessibility guarantees.
 *
 * The window is a borderless, always-on-top overlay with no app chrome: no
 * menu, no title bar, no way to reach it except the pointer or the Tab key. That
 * makes four things load-bearing, and each one is easy to break from a distance:
 *
 *   1. A reading is a label and a number that belong together. Named per row in
 *      the renderer, but the LEAD reading in the `split` and `ring` layouts is
 *      drawn with no group around it, so the window names the surface itself.
 *   2. An unmeasurable reading is an em dash. Never a zero — the app-wide rule —
 *      and a screen reader must be told the same thing the eye is.
 *   3. Every animation is switchable off by the preference the user already set,
 *      including the app's own override rather than only the operating system's.
 *   4. Click-through takes the pointer away. The keyboard must not lose the
 *      controls with it, because that toggle is the only way back.
 *
 * The harness is deliberately local to this file: `WidgetApp.test.tsx` covers the
 * window's behaviour (drag versus click, the spec echo, the cost of a widget at
 * rest) and shares its fixtures with it only through the app's own modules.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Frame, SystemMetrics } from '@vitals/protocol';

import { makeSystem } from '../features/dashboard/test-fixtures';
import type { FrameSource } from '../features/widgets/live';
import { shouldAnimate, type WidgetWindowSpec } from '../features/widgets/model';
import { morphTransition } from '../features/widgets/WidgetPreview';
import type { WidgetBridge } from '../features/widgets/windows';
import type { WidgetWindow } from './lib/window';
import { WidgetApp } from './WidgetApp';

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(() => {
  // The reduced-motion override is read from the element the app writes it to,
  // so a test that sets it must not leave it set for the next one.
  delete document.documentElement.dataset['reduceMotion'];
});

const SPEC: WidgetWindowSpec = {
  id: 'cpu-1',
  kind: 'cpu',
  size: 'medium',
  style: 'plain',
  layout: 'stack',
  opacity: 1,
  alwaysOnTop: true,
  remoteConnectionId: null,
  clickThrough: false,
  x: 40,
  y: 40,
  width: 240,
  height: 150,
};

/** Every control the toolbar offers, by the name a screen reader reads. */
const TOOLBAR_NAMES = [
  'Stop keeping it on top',
  'Let clicks pass through',
  "Open this widget's settings",
  'Close this widget',
] as const;

interface Shown {
  /** Pushes a reading, as the `vitals://frame` broadcast would. */
  readonly frame: (system: SystemMetrics) => void;
}

function show(spec: WidgetWindowSpec | null = SPEC): Shown {
  const frames = new Set<(frame: Frame) => void>();

  const bridge: WidgetBridge = {
    sync: async () => [],
    get: async () => spec,
    close: async () => {},
    setFlags: async () => {},
    openSettings: async () => {},
    onMoved: () => () => {},
    onClosed: () => () => {},
    onChanged: () => () => {},
    onEdit: () => () => {},
    onConfig: () => () => {},
  };

  const win: WidgetWindow = {
    label: async () => null,
    startDragging: async () => {},
    setIgnoreCursorEvents: async () => {},
    setAlwaysOnTop: async () => {},
    close: async () => {},
  };

  const source: FrameSource = (onFrame) => {
    frames.add(onFrame);
    return () => {
      frames.delete(onFrame);
    };
  };

  render(<WidgetApp id="cpu-1" locale="en" bridge={bridge} window={win} source={source} />);

  return {
    frame: (system) => {
      act(() => {
        for (const listener of frames) {
          listener({
            seq: 1,
            timestampMs: 1_760_000_000_000,
            elapsedMs: 1000,
            payload: { kind: 'keyframe', system, processes: [] },
          });
        }
      });
    },
  };
}

/** The accessible name of the surface the window names for the reading. */
function readingName(): string | null {
  // The one group whose name carries a value: "CPU: 50%". The renderer names its
  // own rows, and those names are a bare label, so the separator is what tells
  // the two apart without depending on where either sits in the tree.
  const named = [...document.querySelectorAll('[role="group"][aria-label]')].map(
    (element) => element.getAttribute('aria-label') ?? '',
  );
  return named.find((name) => name.includes(': ')) ?? null;
}

/** Class tokens that make an element animate, ignoring the neutraliser itself. */
const ANIMATING = /^transition-(?!none$)/;

/**
 * Every element in the window that would animate on a state change, and whether
 * it carries the rule that switches it off under `prefers-reduced-motion`.
 *
 * Read from the classes rather than from computed style because the widget
 * stylesheet is not loaded in this environment; the class is what the browser
 * turns into that media query, so a missing pair here is a missing pair there.
 */
function animatingWithoutNeutraliser(): { readonly scanned: number; readonly offenders: string[] } {
  const offenders: string[] = [];
  let scanned = 0;
  for (const element of document.body.querySelectorAll('*')) {
    const classes = element.getAttribute('class') ?? '';
    const tokens = classes.split(/\s+/);
    if (!tokens.some((token) => ANIMATING.test(token))) continue;
    scanned += 1;
    if (!tokens.includes('motion-reduce:transition-none')) {
      offenders.push(element.outerHTML.slice(0, 120));
    }
  }
  return { scanned, offenders };
}

describe('the reading a widget window shows', () => {
  it('is announced as one sentence with its label and its value', async () => {
    const widget = show();
    await waitFor(() => {
      expect(readingName()).toBe('CPU: —');
    });

    widget.frame(makeSystem({ cpu: { total: 50, temperature: 61 } }));

    await waitFor(() => {
      expect(readingName()).toBe('CPU: 50%');
    });

    // Stated as two separate claims as well, so the test says what it means: a
    // name that is only the number ("50%") or only the label ("CPU") is the
    // defect this pins — a sighted user reads the pair, and a screen reader must
    // not be handed the number with nothing to attach it to.
    const name = readingName() ?? '';
    expect(name).toContain('CPU');
    expect(name).toContain('50%');
  });

  it('shows an em dash for a reading nothing has measured, and never a zero', async () => {
    // No frame has arrived at all: the machine has not been sampled, so there is
    // no reading — which is a different fact from a reading of zero, and the one
    // the whole repository is built around.
    show();

    await waitFor(() => {
      expect(readingName()).toBe('CPU: —');
    });

    const name = readingName() ?? '';
    expect(name).toContain('—');
    expect(name).not.toContain('0');
    // And the number is on screen as the dash too, rather than only in the name
    // a screen reader hears.
    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
    expect(screen.queryByText('0%')).toBeNull();
  });
});

describe('motion in a widget window', () => {
  it('leaves no animation the reduced-motion preference cannot switch off', async () => {
    // Both switches are exercised: the app's own override, which the stylesheet
    // reads off this attribute, and the operating system's, which the
    // `motion-reduce:` variant compiles to a media query. The widget window
    // cannot reach the settings store, so the OS preference is what it says it
    // honours — and a CSS transition that ignores it is a WCAG 2.2 failure.
    document.documentElement.dataset['reduceMotion'] = 'true';
    show();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Close this widget' })).toBeDefined();
    });

    const { scanned, offenders } = animatingWithoutNeutraliser();
    expect(offenders).toEqual([]);
    // Not vacuous: the toolbar's own fade is one of the animations being
    // inspected, so a renamed utility cannot quietly empty this test.
    expect(scanned).toBeGreaterThanOrEqual(5);

    // And the JavaScript seam the renderer's Morph uses agrees with the CSS.
    expect(shouldAnimate(true)).toBe(false);
    expect(morphTransition(true).duration).toBe(0);
    // Stated so the assertion above cannot pass for the wrong reason: with motion
    // allowed the morph is a real transition, so `duration: 0` is the preference
    // being honoured rather than a value that is always zero.
    expect(morphTransition(false).duration).toBeGreaterThan(0);
  });
});

describe('the toolbar in a widget window', () => {
  it('keeps every control named, reachable and at least 24 CSS pixels', async () => {
    show();

    for (const name of TOOLBAR_NAMES) {
      const button = await screen.findByRole('button', { name });
      // Reachable: an always-on-top window that took its controls out of the tab
      // order would leave the keyboard with no way to use them.
      expect(button.tabIndex).toBeGreaterThanOrEqual(0);
      const classes = button.getAttribute('class') ?? '';
      // 24x24 is the floor WCAG 2.2 sets for a pointer target. happy-dom computes
      // no layout, so the utility that sets it is the property that can be
      // checked here; the glyph inside it is half that, which is the point.
      expect(classes).toContain('size-6');
      expect(classes).toContain('min-h-6');
      expect(classes).toContain('min-w-6');
      // And a focus ring that does not depend on the pointer being anywhere near.
      expect(classes).toContain('focus-visible:outline-2');
    }
  });

  it('sits on the opaque backing token, so its icons never depend on the wallpaper', async () => {
    show();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Close this widget' })).toBeDefined();
    });

    // Over the translucent `glass` panel fill a muted icon measures 1.64:1
    // against a light wallpaper — see the measurement in `styles.css`. The
    // toolbar is held off that layer by the overlay token, which is what this
    // asserts; the arithmetic itself belongs in the stylesheet, not in a test.
    const toolbar = document.querySelector('[class*="bg-[var(--color-bg-overlay)]"]');
    expect(toolbar).not.toBeNull();
    for (const name of TOOLBAR_NAMES) {
      expect(toolbar?.contains(screen.getByRole('button', { name }))).toBe(true);
    }
  });

  it('marks a window whose clicks pass through, and leaves the keyboard its controls', async () => {
    show({ ...SPEC, clickThrough: true });

    await waitFor(() => {
      expect(document.querySelector('[class*="ring-[var(--color-accent)]"]')).not.toBeNull();
    });

    // Click-through takes the pointer away, and that toggle is the only way back
    // out of it. Hiding the toolbar from the keyboard as well would make the
    // state impossible to leave without a mouse the window is ignoring — so the
    // controls stay reachable, and nothing here traps them.
    //
    // Enumerated rather than named: the two toggles rename themselves to the
    // action they now offer, so the four controls of this state are not the four
    // names of the idle one.
    const buttons = screen.getAllByRole('button');
    expect(buttons).toHaveLength(TOOLBAR_NAMES.length);
    for (const button of buttons) {
      expect(button.getAttribute('aria-label')).not.toBeNull();
      expect(button.tabIndex).toBeGreaterThanOrEqual(0);
      expect(button.getAttribute('aria-hidden')).toBeNull();
      expect(button.closest('[aria-hidden="true"]')).toBeNull();
      expect(button.closest('[inert]')).toBeNull();
    }
    expect(document.querySelector('[aria-modal="true"]')).toBeNull();

    const root = document.querySelector('[data-tauri-drag-region]');
    expect(root).not.toBeNull();
    expect(fireEvent.keyDown(root!, { key: 'Tab' })).toBe(true);
    expect(fireEvent.keyDown(root!, { key: 'Escape' })).toBe(true);
  });

  it('dismisses the expanded panel on Escape without swallowing the key', async () => {
    show();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Close this widget' })).toBeDefined();
    });

    // Collapsed, the widget shows its lead reading only.
    const collapsed = (): boolean => screen.queryByText('Temperature') === null;
    expect(collapsed()).toBe(true);

    const root = document.querySelector('[data-tauri-drag-region]');
    expect(root).not.toBeNull();
    fireEvent.click(root!);

    await waitFor(() => {
      expect(collapsed()).toBe(false);
    });

    // The key is not consumed: this window is always-on-top and borderless, so a
    // cancelled key here would strand the user's keyboard on top of their work.
    expect(fireEvent.keyDown(root!, { key: 'Escape' })).toBe(true);

    await waitFor(() => {
      expect(collapsed()).toBe(true);
    });
  });
});
