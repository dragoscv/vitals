/**
 * The sliding tab indicator.
 *
 * The behaviour under test is measured geometry, and happy-dom ships no layout
 * engine — every box reports zero and the indicator is (correctly) not rendered
 * at all. So the boxes are stubbed from `data-*` attributes the fixture sets,
 * the same stand-in the Dialog test uses when it asserts the classes that
 * produce a layout guarantee rather than the rendered pixels.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { settle, stubBoxes } from '../test/layout';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './Tabs';

afterEach(cleanup);

function Fixture() {
  return (
    <Tabs defaultValue="cpu">
      <TabsList aria-label="Resource">
        <TabsTrigger value="cpu" data-left="0" data-top="0" data-w="60" data-h="32">
          CPU
        </TabsTrigger>
        <TabsTrigger value="gpu" data-left="64" data-top="0" data-w="80" data-h="32">
          GPU
        </TabsTrigger>
      </TabsList>
      <TabsContent value="cpu">CPU panel</TabsContent>
      <TabsContent value="gpu">GPU panel</TabsContent>
    </Tabs>
  );
}

function indicatorStyle(): CSSStyleDeclaration {
  const element = document.querySelector<HTMLElement>('[data-tab-indicator]');
  expect(element, 'no sliding indicator was rendered at all').not.toBeNull();
  return (element as HTMLElement).style;
}

describe('Tabs indicator', () => {
  it('is one aria-hidden marker that moves to the selected tab, rather than one per tab', async () => {
    const restore = stubBoxes();
    try {
      render(<Fixture />);
      await settle();

      const indicator = document.querySelector('[data-tab-indicator]');
      expect(
        indicator?.getAttribute('aria-hidden'),
        'the indicator is announced, so the selected tab is read out twice',
      ).toBe('true');
      expect(
        document.querySelectorAll('[data-tab-indicator]').length,
        'more than one marker was rendered — that is the per-tab marker this replaces',
      ).toBe(1);
      expect(indicatorStyle().width).toBe('60px');
      expect(
        indicatorStyle().transform,
        'the indicator did not start on the selected tab',
      ).toContain('0px');

      fireEvent.mouseDown(screen.getByRole('tab', { name: 'GPU' }), { button: 0 });
      await settle();

      expect(
        document.querySelectorAll('[data-tab-indicator]').length,
        'a second marker appeared instead of the first one moving',
      ).toBe(1);
      expect(indicatorStyle().width, 'the indicator did not resize to the new tab').toBe('80px');
      expect(
        indicatorStyle().transform,
        'the indicator did not move to the tab the user selected',
      ).toContain('64px');
    } finally {
      restore();
    }
  });

  it('draws nothing when no box can be measured, rather than pinning a marker to the corner', async () => {
    render(<Fixture />);
    await settle();

    expect(
      document.querySelector('[data-tab-indicator]'),
      'the indicator was rendered from a zero-size measurement, which puts it at the strip origin on every real layout',
    ).toBeNull();
  });

  it('leaves the keyboard model untouched — the marker is decoration, not a control', async () => {
    const restore = stubBoxes();
    try {
      render(<Fixture />);
      await settle();

      const cpu = screen.getByRole('tab', { name: 'CPU' });
      cpu.focus();
      fireEvent.keyDown(cpu, { key: 'Enter' });
      await settle();

      expect(document.activeElement, 'committing a tab moved focus off it').toBe(cpu);
      expect(indicatorStyle().width, 'the indicator ignored a keyboard selection').toBe('60px');
    } finally {
      restore();
    }
  });
});
