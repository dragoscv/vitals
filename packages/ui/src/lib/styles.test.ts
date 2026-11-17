/**
 * The overlay motion contract.
 *
 * These assertions are about a behaviour no render can show: whether Radix will
 * *wait* for an overlay to finish leaving, or unmount it in the same tick the
 * close starts. Presence decides that by reading the element's computed
 * `animation-name` — so the guarantee is "the closed state names a keyframe
 * animation, and a different one from the open state", and it is asserted here
 * on the class and the stylesheet that carry it. happy-dom has no CSS engine, so
 * a computed-style assertion would pass whatever the stylesheet said; reading
 * the source is the only way to make this test able to fail.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { menuContent } from './menuStyles';
import { overlayMotion, overlayScrim } from './styles';

// Resolved from the package root rather than from `import.meta.url`, which is
// not a `file:` URL under vitest, so `new URL(..., import.meta.url)` throws.
// Vitest's working directory is the package that owns its config.
const css = readFileSync(resolve('src/styles/theme.css'), 'utf8');

/** The declaration body of the block whose selector starts at `from`. */
function bracedBody(open: number): string {
  let depth = 0;
  for (let i = open; i < css.length; i += 1) {
    if (css[i] === '{') depth += 1;
    else if (css[i] === '}') {
      depth -= 1;
      if (depth === 0) return css.slice(open + 1, i);
    }
  }
  throw new Error('unterminated block in theme.css');
}

/** The body of the first rule whose selector is `selector`. */
function ruleBody(selector: string): string {
  const at = css.indexOf(selector);
  expect(at, `${selector} is missing from theme.css`).toBeGreaterThanOrEqual(0);
  return bracedBody(css.indexOf('{', at));
}

/** The body of the last rule whose selector is `selector` — for rules repeated
 *  in more than one `@media`, such as the reduced-motion collapse. */
function lastRuleBody(selector: string): string {
  return bracedBody(css.indexOf('{', css.lastIndexOf(selector)));
}

/** The body of a top-level rule, found by its selector starting a line. Needed
 *  where an earlier `@media` override mentions the same class. */
function baseRuleBody(selector: string): string {
  const at = css.indexOf(`\n${selector}`);
  expect(at, `${selector} has no top-level rule in theme.css`).toBeGreaterThanOrEqual(0);
  return bracedBody(css.indexOf('{', at));
}

function animationName(state: 'open' | 'closed'): string | undefined {
  return /animation:\s*([\w-]+)/.exec(
    ruleBody(`.vitals-overlay-motion[data-state='${state}']`),
  )?.[1];
}

describe('overlay motion', () => {
  it('names a different keyframe animation for the open and closed states, which is what makes the exit run', () => {
    const open = animationName('open');
    const closed = animationName('closed');

    expect(open, 'the open state names no animation, so nothing plays on entry').toBe(
      'vitals-overlay-in',
    );
    expect(
      closed,
      'the closed state names no animation: Radix reads an animation-name of "none" as "nothing is animating out" and unmounts the overlay before it can leave',
    ).toBe('vitals-overlay-out');
    expect(
      open,
      'both states name the same animation, so presence sees no animation *out* and skips the exit',
    ).not.toBe(closed);
  });

  it('defines both keyframes, and animates opacity and scale rather than transform', () => {
    for (const name of ['vitals-overlay-in', 'vitals-overlay-out']) {
      const frames = ruleBody(`@keyframes ${name}`);
      expect(frames, `${name} does not fade`).toContain('opacity');
      expect(frames, `${name} does not scale`).toContain('scale');
      expect(
        frames,
        `${name} animates transform, which would throw a Radix-popper menu and a centred dialog into the corner for the length of the animation`,
      ).not.toContain('transform');
    }
  });

  it('fades the scrim without scaling it, so the dimmed edges stay on the window', () => {
    for (const name of ['vitals-overlay-scrim-in', 'vitals-overlay-scrim-out']) {
      const frames = ruleBody(`@keyframes ${name}`);
      expect(frames).toContain('opacity');
      expect(
        frames,
        `${name} scales a full-bleed element, which pulls its edges in and leaves a border of un-dimmed app around the dialog`,
      ).not.toContain('scale');
    }
  });

  it('stays a pair of animations and never goes back to a transition Radix cannot observe', () => {
    expect(overlayMotion).toBe('vitals-overlay-motion');
    expect(overlayScrim).toBe('vitals-overlay-scrim');
    expect(
      overlayMotion,
      'a transition leaves animation-name at "none"; presence reads that as "not animating" and removes the overlay in the same tick the close begins, which is exactly the bug this replaced',
    ).not.toMatch(/\btransition-/);
    expect(
      overlayScrim,
      'the scrim has the same problem: as a transition it vanishes with no fade',
    ).not.toMatch(/\btransition-/);
  });

  it('collapses the overlay animations outright when motion is reduced, rather than shortening them', () => {
    const reduced = lastRuleBody('@media (prefers-reduced-motion: reduce)');
    expect(
      reduced,
      'the media-query collapse does not name the overlays, so one frame of the enter/exit offset still plays',
    ).toContain('.vitals-overlay-motion');
    expect(reduced).toMatch(/animation:\s*none\s*!important/);

    expect(
      ruleBody("[data-reduce-motion='true'] .vitals-overlay-motion"),
      "the app's own reduced-motion setting leaves the overlay animations running",
    ).toMatch(/animation:\s*none\s*!important/);
    expect(ruleBody("[data-reduce-motion='true'] .vitals-tab-indicator")).toMatch(
      /transition:\s*none\s*!important/,
    );
    expect(
      reduced,
      'the collapsible is not named in the media-query collapse, so a panel still folds on the way out',
    ).toContain('.vitals-collapsible');
    expect(ruleBody("[data-reduce-motion='true'] .vitals-collapsible")).toMatch(
      /transition:\s*none\s*!important/,
    );
  });

  it('ships the entry as a starting style, so an engine that resolves it first cannot paint the open state', () => {
    const starting = ruleBody('@starting-style');
    expect(starting).toContain(".vitals-overlay-motion[data-state='open']");
    expect(starting).toContain(".vitals-overlay-scrim[data-state='open']");
  });

  it('slides the tab indicator instead of restarting an animation on it', () => {
    const indicator = baseRuleBody('.vitals-tab-indicator');
    expect(
      indicator,
      'the indicator is not transitioned on transform, so it will jump between tabs',
    ).toMatch(/transition:[\s\S]*transform/);
    expect(indicator).toMatch(/transition:[\s\S]*--duration-normal/);
  });

  it('drives the motion vocabulary from tokens that something actually consumes', () => {
    for (const token of ['--spring-snappy', '--spring-soft', '--stagger-step']) {
      expect(css, `${token} is used below but never declared`).toMatch(new RegExp(`${token}:`));
    }
    expect(
      baseRuleBody('.vitals-tab-indicator'),
      'the indicator falls back to an ad-hoc easing instead of the spring token',
    ).toContain('--spring-snappy');
    expect(
      baseRuleBody(".vitals-overlay-motion[data-state='open']"),
      'the overlay entrance ignores the softer spring',
    ).toContain('--spring-soft');
    expect(
      baseRuleBody('.vitals-stagger > *'),
      'the staggered entrance hard-codes its step instead of using the token',
    ).toContain('--stagger-step');
  });

  it('opens a collapsible with a starting-style transition and closes it with its own animation', () => {
    const base = baseRuleBody('.vitals-collapsible');
    expect(
      base,
      'the open direction has no transition on the row, so the panel appears already expanded',
    ).toMatch(/transition:[\s\S]*grid-template-rows/);
    expect(base, 'the panel does not start expanded').toContain('grid-template-rows: 1fr');

    const closed = baseRuleBody(".vitals-collapsible[data-state='closed']");
    expect(
      /animation:\s*([\w-]+)/.exec(closed)?.[1],
      'the closed state names no animation, so presence unmounts the panel before it can fold',
    ).toBe('vitals-collapse-out');

    const frames = ruleBody('@keyframes vitals-collapse-out');
    expect(frames, 'the collapse keyframes do not animate the row height').toContain(
      'grid-template-rows',
    );
    expect(frames, 'the collapse keyframes do not end collapsed').toMatch(/to\s*\{[\s\S]*0fr/);

    expect(
      baseRuleBody('.vitals-collapsible > *'),
      'nothing clips the collapsing row, so the content keeps its height and the panel does not fold',
    ).toMatch(/overflow:\s*hidden/);

    expect(
      lastRuleBody('@starting-style'),
      'the entry has no starting style, so the panel is painted open before the transition can run',
    ).toContain(".vitals-collapsible[data-state='open']");
  });

  it('caps a menu to the room left before the window edge, so its last items stay reachable', () => {
    expect(
      menuContent,
      'a menu taller than the window has no way to reach its last items: a popup is positioned, not scrollable',
    ).toContain('max-h-(--radix-popper-available-height)');
    expect(menuContent, 'the capped menu cannot be scrolled').toContain('overflow-y-auto');
  });

  it('gives the select popup the smaller of its own limit and the room before the window edge', () => {
    expect(
      baseRuleBody('.vitals-select-content'),
      'the select keeps its 18rem cap on a viewport that cannot show 18rem',
    ).toMatch(
      /max-height:\s*min\(\s*18rem\s*,\s*var\(--radix-select-content-available-height\)\s*\)/,
    );
  });
});
