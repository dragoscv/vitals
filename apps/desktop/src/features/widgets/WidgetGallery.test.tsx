/**
 * Guards the widget gallery.
 *
 * The properties asserted here are the ones that decide whether the gallery is
 * usable rather than whether it looks right: a tile per kind so nothing is
 * hidden, a place button named for its own kind so a screen reader — or a
 * voice — can tell six identical-looking tiles apart, the disabled state
 * carrying its reason to assistive technology rather than only to a tooltip,
 * and the rule the whole product rests on: an unmeasured reading is an em dash,
 * never a confident zero.
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { makeSystem } from '../dashboard/test-fixtures';
import { EMPTY_WIDGET_INPUT, type WidgetInput } from './describe';
import { widgetKinds, type WidgetKind } from './model';
import { WidgetGallery, type WidgetGalleryProps } from './WidgetGallery';

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

const GIB = 1024 ** 3;

/**
 * The English kind names, written out rather than read back from the locale.
 *
 * Read back, the test would pass against a component that used the wrong key,
 * because both sides would be wrong the same way. Spelled out, it pins the
 * contract the tiles are judged against.
 */
const KIND_LABEL: Record<WidgetKind, string> = {
  cpu: 'CPU',
  memory: 'Memory',
  disk: 'Disk',
  network: 'Network',
  thermal: 'Thermals',
  overview: 'Overview',
};

/**
 * The verb, spelled out for the same reason as the kind names above.
 *
 * Accessible, not visible: the button reads "Add a widget" on screen and is
 * named by its tile's heading plus those words, so this is the half of the name
 * that has to be right for six tiles to be tellable apart.
 */
const ADD_WIDGET = 'Add a widget';

function input(overrides: Partial<WidgetInput> = {}): WidgetInput {
  return { ...EMPTY_WIDGET_INPUT, system: makeSystem(), ...overrides };
}

/** Renders the gallery, defaulting everything but the place callback. */
function show(overrides: Omit<Partial<WidgetGalleryProps>, 'onAdd'> = {}): {
  readonly onAdd: ReturnType<typeof vi.fn<(kind: WidgetKind) => void>>;
} {
  const onAdd = vi.fn<(kind: WidgetKind) => void>();
  render(<WidgetGallery locale="en" input={EMPTY_WIDGET_INPUT} onAdd={onAdd} {...overrides} />);
  return { onAdd };
}

/** The place button for one kind. */
function placeButton(kind: WidgetKind): HTMLButtonElement {
  return screen.getByRole<HTMLButtonElement>('button', {
    name: `${KIND_LABEL[kind]} ${ADD_WIDGET}`,
  });
}

describe('the widget gallery', () => {
  it('offers one tile per kind, each place button named for its own kind', () => {
    show();

    // One button per kind and no more: a kind without a tile is a widget the
    // user cannot discover, and a spare button is a tile that names nothing.
    expect(screen.getAllByRole('button')).toHaveLength(widgetKinds.length);

    for (const kind of widgetKinds) {
      // Reached through the composite name, so this fails if the heading is
      // missing or the tile stops pointing at it — six buttons all called
      // "Add a widget" would be unusable by screen reader.
      expect(placeButton(kind).textContent?.trim()).toBe(ADD_WIDGET);
      // The heading is the half of that name which names the kind: one heading
      // per tile, each reading its own kind.
      expect(screen.getByRole('heading', { name: KIND_LABEL[kind] })).toBeTruthy();
    }
  });

  it('places exactly the kind whose tile the user pressed', () => {
    const { onAdd } = show();

    for (const kind of widgetKinds) {
      onAdd.mockClear();
      fireEvent.click(placeButton(kind));

      // "Exactly that kind": one call, carrying its own kind, not the first or
      // a shared one — an off-by-one mapping is the failure this catches.
      expect(onAdd).toHaveBeenCalledTimes(1);
      expect(onAdd).toHaveBeenCalledWith(kind);
    }
  });

  it('disables every place button when capped and exposes the reason to assistive tech', () => {
    show({ disabled: true, disabledReason: 'Up to 12 at once.' });

    const buttons = screen.getAllByRole<HTMLButtonElement>('button');
    expect(buttons).toHaveLength(widgetKinds.length);
    expect(buttons.every((button) => button.disabled)).toBe(true);

    for (const button of buttons) {
      // A tooltip is not an accessible name: the reason must be a real node the
      // button points at, so a screen reader reads it on focus. `aria-describedby`
      // rather than `title` alone is what makes that true.
      expect(button.getAttribute('title')).toBe('Up to 12 at once.');
      const describedBy = button.getAttribute('aria-describedby');
      expect(describedBy).toBeTruthy();
      const reason = document.getElementById(describedBy ?? '');
      expect(reason?.textContent).toBe('Up to 12 at once.');
    }
  });

  it('draws an em dash for a machine that reports nothing, never a zero', () => {
    show({ input: EMPTY_WIDGET_INPUT });

    // Asserted on the thermals tile: a machine with no temperature sensor. Any
    // reading of 0 °C here would be a confident lie about a cold processor.
    // (The CPU tile is not used for this because its default `ring` layout has
    // no arc to draw and so renders no value at all — that is `WidgetPreview`'s
    // business, not the gallery's, and asserting on it would test the wrong
    // component.)
    const tile = placeButton('thermal').closest('li');
    expect(tile?.textContent).toContain('—');
    expect(tile?.textContent).not.toContain('0%');
    expect(tile?.textContent).not.toContain('0 %');
    expect(tile?.textContent).not.toContain('0 °');
  });

  it('formats its numbers in the locale it was given, not the app default', () => {
    // The app is left on English and the gallery is asked for Romanian: the
    // commas can only come from the `locale` prop, which is the seam a
    // Romanian user's screen depends on.
    show({
      locale: 'ro',
      input: input({ system: makeSystem({ memory: { total: 32 * GIB, used: 16.5 * GIB } }) }),
    });

    const text = document.body.textContent ?? '';
    expect(text).toContain('16,5 GB');
    expect(text).not.toContain('16.5 GB');
  });
});
