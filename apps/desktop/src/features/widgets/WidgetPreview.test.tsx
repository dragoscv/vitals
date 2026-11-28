/**
 * Guards the one renderer every widget is drawn by.
 *
 * The failures here are the kind that survive review because they look right:
 * a reading that is not measured drawn as a hard zero (the one rule AGENTS.md
 * puts above everything else), a layout that silently renders nothing, a morph
 * that animates for a user who asked for less motion. Each test drives the real
 * component with a real description, so what it asserts is what a widget shows.
 */

import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { makeSystem } from '../dashboard/test-fixtures';

import { describe as describeWidget, EMPTY_WIDGET_INPUT } from './describe';
import type { WidgetLayout } from './model';
import { WidgetPreview, morphTransition } from './WidgetPreview';

function spec(overrides: Partial<Parameters<typeof WidgetPreview>[0]['spec']> = {}) {
  return {
    id: 'cpu-1',
    kind: 'cpu' as const,
    size: 'medium' as const,
    style: 'plain' as const,
    layout: 'stack' as const,
    opacity: 1,
    alwaysOnTop: true,
    remoteConnectionId: null,
    clickThrough: false,
    x: 0,
    y: 0,
    ...overrides,
  };
}

function show(
  kind: Parameters<typeof describeWidget>[0],
  input: Parameters<typeof describeWidget>[1],
  overrides: Partial<Parameters<typeof WidgetPreview>[0]['spec']> = {},
  expanded = false,
) {
  const description = describeWidget(kind, input, 'en');
  return render(
    <WidgetPreview spec={spec(overrides)} description={description} expanded={expanded} />,
  );
}

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  delete document.documentElement.dataset['reduceMotion'];
});

describe('the morph a user who asked for less motion gets', () => {
  it('runs for no time at all when motion is reduced, and moves otherwise', () => {
    // Zero rather than a short spring: the state change is the information, and
    // a preference for less motion is a request for it to arrive without the
    // travel. A non-zero duration here animates through the preference.
    expect(morphTransition(true).duration).toBe(0);
    expect(morphTransition(false).duration).toBeGreaterThan(0);
  });
});

describe('what a widget shows', () => {
  it('draws the reading inside the ring before there is a percentage to fill it', () => {
    // A widget placed seconds after boot has no percentage yet. The ring must
    // still exist and show "—": an empty panel reads as a broken widget rather
    // than as an unmeasured one, and the track with no arc is the honest
    // picture of "nothing measured yet".
    const { container } = show('cpu', EMPTY_WIDGET_INPUT, { layout: 'ring' });

    expect(container.querySelector('svg')).not.toBeNull();
    expect(container.querySelectorAll('circle')).toHaveLength(2);
    expect(container.querySelector('svg')?.parentElement?.textContent).toContain('—');
  });
  it('names itself from its content kind so a reader knows what it is watching', () => {
    show('cpu', { ...EMPTY_WIDGET_INPUT });

    expect(screen.getByRole('group', { name: 'CPU' })).toBeTruthy();
  });

  it('shows only the lead reading until it is expanded', () => {
    const input = { ...EMPTY_WIDGET_INPUT, system: makeSystem() };

    const { unmount } = show('cpu', input);
    // A collapsed widget is a glance, not a dashboard: the second reading must
    // not be in the tree at all, so nothing can measure or read it early.
    expect(screen.queryByText('Temperature')).toBeNull();
    unmount();

    show('cpu', input, {}, true);
    expect(screen.getByText('Temperature')).toBeTruthy();
  });

  it('draws the memory lead as a share of the machine it is running on', () => {
    show('memory', {
      ...EMPTY_WIDGET_INPUT,
      system: makeSystem({ memory: { total: 32 * 1024 ** 3, used: 16 * 1024 ** 3 } }),
    });

    // `AnimatedValue` renders the number twice on purpose: one span the eye
    // sees, one for a screen reader, so a live value is announced once.
    expect(screen.getAllByText('50%').length).toBeGreaterThan(0);
  });

  it('carries the stored opacity so the window needs no opacity of its own', () => {
    const { container } = show('cpu', { ...EMPTY_WIDGET_INPUT }, { opacity: 0.4 });

    const root = container.firstElementChild as HTMLElement;
    expect(root.style.opacity).toBe('0.4');
  });

  it('draws an em dash rather than a zero for a reading nobody measured', () => {
    // The rule the whole product rests on: an unmeasured number and a genuine
    // zero are different facts. A CPU percentage of 0 on a machine with no
    // reading yet would be a confident lie about a quiet processor.
    show('cpu', { ...EMPTY_WIDGET_INPUT });

    expect(screen.getAllByText('—').length).toBeGreaterThan(0);
    expect(screen.queryByText('0 %')).toBeNull();
    expect(screen.queryByText('0%')).toBeNull();
  });

  const layouts: readonly WidgetLayout[] = ['stack', 'split', 'ring'];
  for (const layout of layouts) {
    it(`draws the lead reading in the ${layout} layout`, () => {
      const { container } = show(
        'memory',
        { ...EMPTY_WIDGET_INPUT, system: makeSystem({ memory: { total: 100, used: 25 } }) },
        { layout },
      );

      // Every layout has to show the number; a layout that draws nothing is a
      // blank panel the user cannot tell from a broken one.
      expect(container.textContent).toContain('25%');
    });
  }

  it('renders the expanded readings immediately when the system asks for less motion', () => {
    document.documentElement.dataset['reduceMotion'] = 'true';

    show('cpu', { ...EMPTY_WIDGET_INPUT, system: makeSystem() }, {}, true);

    // Content first, motion second: the readings are present regardless of what
    // the transition does.
    expect(screen.getByText('Temperature')).toBeTruthy();
  });
});
