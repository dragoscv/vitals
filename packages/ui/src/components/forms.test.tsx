import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Checkbox } from './Checkbox';
import { Input } from './Input';
import { SearchInput } from './SearchInput';
import { SegmentedControl } from './SegmentedControl';
import { Switch } from './Switch';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './Tabs';

afterEach(cleanup);

describe('Switch', () => {
  it('toggles with Space and reports state as a switch, not a checkbox', async () => {
    const onCheckedChange = vi.fn();
    render(<Switch ariaLabel="Start with Windows" onCheckedChange={onCheckedChange} />);

    const control = screen.getByRole('switch', { name: 'Start with Windows' });
    expect(
      control.getAttribute('aria-checked'),
      'an off switch must report aria-checked="false"',
    ).toBe('false');

    control.focus();
    fireEvent.keyDown(control, { key: ' ' });
    fireEvent.keyUp(control, { key: ' ' });
    fireEvent.click(control);
    await act(async () => {});

    expect(onCheckedChange, 'Space did not toggle the switch').toHaveBeenCalled();
  });

  it('links a visible label to the control, enlarging the hit target past 24px', () => {
    render(<Switch label="Launch minimised" />);
    // getByRole with a name only resolves if htmlFor/id are wired correctly.
    expect(
      screen.getByRole('switch', { name: 'Launch minimised' }),
      'the visible label is not associated with the switch, so clicking it does nothing',
    ).toBeTruthy();
  });
});

describe('Checkbox', () => {
  it('reports the indeterminate state as "mixed" for a partial selection', () => {
    render(<Checkbox checked="indeterminate" ariaLabel="Select all" />);
    expect(
      screen.getByRole('checkbox', { name: 'Select all' }).getAttribute('aria-checked'),
      'a partially selected "select all" reported a plain unchecked state, which is a lie',
    ).toBe('mixed');
  });

  it('toggles from the keyboard', async () => {
    const onCheckedChange = vi.fn();
    render(<Checkbox ariaLabel="Show system processes" onCheckedChange={onCheckedChange} />);
    const box = screen.getByRole('checkbox');
    box.focus();
    expect(document.activeElement, 'the checkbox did not accept focus').toBe(box);

    fireEvent.click(box);
    await act(async () => {});
    expect(onCheckedChange, 'activating the checkbox did not report a change').toHaveBeenCalled();
  });
});

describe('Input', () => {
  it('associates its label so the field is not announced as unlabelled', () => {
    render(<Input label="Filter" />);
    expect(screen.getByLabelText('Filter'), 'the label was not wired to the input').toBeTruthy();
  });

  it('marks an invalid field and links the message, because a red border says nothing aloud', () => {
    render(<Input label="Refresh interval" error="Must be at least 100 ms" />);
    const field = screen.getByLabelText('Refresh interval');

    expect(field.getAttribute('aria-invalid'), 'an errored field was not marked aria-invalid').toBe(
      'true',
    );
    const describedBy = field.getAttribute('aria-describedby');
    expect(
      document.getElementById(describedBy ?? '')?.textContent,
      'the error message was not linked with aria-describedby',
    ).toBe('Must be at least 100 ms');
  });

  it('does not put static hint text in a live region', () => {
    render(<Input label="Port" hint="Between 1 and 65535" />);
    expect(
      screen.queryByRole('alert'),
      'help text was announced as an alert, which talks over the user as they type',
    ).toBeNull();
  });
});

describe('SearchInput', () => {
  it('clears on Escape, the expected desktop behaviour for a filter box', () => {
    const onValueChange = vi.fn();
    render(
      <SearchInput
        value="chrome"
        onValueChange={onValueChange}
        clearLabel="Clear"
        ariaLabel="Search"
      />,
    );

    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Escape' });
    expect(onValueChange, 'Escape did not clear the query').toHaveBeenCalledWith('');
  });

  it('lets Escape bubble when already empty, so it cannot trap the user in a dialog', () => {
    const onValueChange = vi.fn();
    const onOuterKeyDown = vi.fn();
    render(
      <div onKeyDown={onOuterKeyDown}>
        <SearchInput value="" onValueChange={onValueChange} clearLabel="Clear" ariaLabel="Search" />
      </div>,
    );

    fireEvent.keyDown(screen.getByRole('searchbox'), { key: 'Escape' });
    expect(
      onValueChange,
      'an empty search box should have nothing to clear',
    ).not.toHaveBeenCalled();
    expect(
      onOuterKeyDown,
      'Escape was swallowed by an empty search box — a surrounding dialog could never be closed',
    ).toHaveBeenCalled();
  });

  it('returns focus to the field after clearing, rather than dropping it', () => {
    const onValueChange = vi.fn();
    render(
      <SearchInput
        value="node"
        onValueChange={onValueChange}
        clearLabel="Clear"
        ariaLabel="Search"
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(onValueChange).toHaveBeenCalledWith('');
    expect(
      document.activeElement,
      'focus was lost after clearing; the user must tab back to type again',
    ).toBe(screen.getByRole('searchbox'));
  });

  it('announces the filtered result count in a polite live region', () => {
    render(
      <SearchInput
        value="node"
        onValueChange={vi.fn()}
        clearLabel="Clear"
        ariaLabel="Search"
        resultsAnnouncement="3 of 540 processes"
      />,
    );
    const region = screen.getByText('3 of 540 processes');
    expect(
      region.getAttribute('aria-live'),
      'filtering must be announced politely, or a screen reader user gets no feedback at all',
    ).toBe('polite');
  });
});

describe('Tabs', () => {
  it('keeps one tab stop and moves the selection with arrow keys', async () => {
    render(
      <Tabs defaultValue="cpu" activationMode="manual">
        <TabsList aria-label="Resource">
          <TabsTrigger value="cpu">CPU</TabsTrigger>
          <TabsTrigger value="gpu">GPU</TabsTrigger>
        </TabsList>
        <TabsContent value="cpu">CPU panel</TabsContent>
        <TabsContent value="gpu">GPU panel</TabsContent>
      </Tabs>,
    );

    const cpu = screen.getByRole('tab', { name: 'CPU' });
    const gpu = screen.getByRole('tab', { name: 'GPU' });

    // Radix's roving-focus group elects its tab stop in an effect after the
    // items register, so the assertion has to run after a flush rather than
    // on the first committed render.
    await act(async () => {});

    // Before anything inside is focused the strip itself carries the tab stop
    // and both tabs are -1; once a tab is focused the stop moves onto it.
    // Either way there is exactly ONE stop for the whole strip, which is the
    // property that matters — without it, reaching the panel in a nine-tab
    // monitor costs nine Tab presses.
    expect(
      [cpu, gpu].filter((tab) => tab.getAttribute('tabindex') === '0').length,
      'more than one tab was in the tab order, so Tab walks the entire strip',
    ).toBeLessThanOrEqual(1);

    cpu.focus();
    fireEvent.keyDown(cpu, { key: 'ArrowRight' });
    await act(async () => {});

    expect(document.activeElement, 'ArrowRight did not move focus to the next tab').toBe(gpu);
    expect(
      cpu.getAttribute('tabindex'),
      'the tab left behind stayed in the tab order — the roving stop did not move with focus',
    ).toBe('-1');
    expect(
      screen.queryByText('GPU panel'),
      'manual activation should not switch panels on arrow alone — samplers would churn per keypress',
    ).toBeNull();
  });

  it('links each tab to its panel', () => {
    render(
      <Tabs defaultValue="cpu">
        <TabsList aria-label="Resource">
          <TabsTrigger value="cpu">CPU</TabsTrigger>
        </TabsList>
        <TabsContent value="cpu">CPU panel</TabsContent>
      </Tabs>,
    );
    const controls = screen.getByRole('tab', { name: 'CPU' }).getAttribute('aria-controls');
    expect(
      document.getElementById(controls ?? '')?.textContent,
      'aria-controls does not resolve to the panel',
    ).toBe('CPU panel');
  });
});

describe('SegmentedControl', () => {
  it('exposes the group name and the pressed segment', () => {
    render(
      <SegmentedControl
        ariaLabel="Time range"
        value="60s"
        onValueChange={vi.fn()}
        options={[
          { value: '60s', label: '60s' },
          { value: '5m', label: '5m' },
        ]}
      />,
    );

    // A single-select toggle group is exposed as a radiogroup, not a plain
    // group — which is the correct mapping: the segments are mutually
    // exclusive, and `radio` is what tells a screen reader "one of these".
    expect(
      screen.getByRole('radiogroup', { name: 'Time range' }),
      'the segmented control did not expose a named radiogroup',
    ).toBeTruthy();
    expect(
      screen.getByRole('radio', { name: '60s' }).getAttribute('aria-checked'),
      'the active segment did not report its selected state',
    ).toBe('true');
  });

  it('ignores a deselect, because a segmented control has no empty state', async () => {
    const onValueChange = vi.fn();
    render(
      <SegmentedControl
        ariaLabel="Units"
        value="binary"
        onValueChange={onValueChange}
        options={[
          { value: 'binary', label: 'GiB' },
          { value: 'decimal', label: 'GB' },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole('radio', { name: 'GiB' }));
    await act(async () => {});

    expect(
      onValueChange,
      'clicking the active segment cleared the setting instead of doing nothing',
    ).not.toHaveBeenCalled();
  });
});
