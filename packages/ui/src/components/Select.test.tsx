import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ScrollArea } from './ScrollArea';
import { Select } from './Select';

afterEach(cleanup);

const OPTIONS = [
  { value: 'auto', label: 'Automatic' },
  { value: '1s', label: 'Every second' },
  { value: '5s', label: 'Every 5 seconds' },
] as const;

describe('Select', () => {
  it('is a labelled combobox, so its purpose is announced before it is opened', () => {
    render(<Select label="Refresh rate" value="auto" options={OPTIONS} onValueChange={vi.fn()} />);
    expect(
      screen.getByRole('combobox', { name: 'Refresh rate' }),
      'the visible label was not associated with the trigger',
    ).toBeTruthy();
  });

  it('opens from the keyboard and exposes options as a listbox', async () => {
    render(
      <Select ariaLabel="Refresh rate" value="auto" options={OPTIONS} onValueChange={vi.fn()} />,
    );
    const trigger = screen.getByRole('combobox');
    trigger.focus();

    fireEvent.keyDown(trigger, { key: 'Enter' });
    await act(async () => {});

    expect(
      screen.queryByRole('listbox'),
      'Enter did not open the list — the control is mouse-only',
    ).not.toBeNull();
    expect(screen.getAllByRole('option').length).toBe(3);
  });

  it('marks the current value as selected rather than relying on a tick glyph', async () => {
    render(
      <Select ariaLabel="Refresh rate" value="1s" options={OPTIONS} onValueChange={vi.fn()} />,
    );
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
    await act(async () => {});

    expect(
      screen.getByRole('option', { name: 'Every second' }).getAttribute('aria-selected'),
      'the chosen option was shown with a checkmark but not exposed as selected',
    ).toBe('true');
  });

  it('closes on Escape and restores focus to the trigger', async () => {
    render(
      <Select ariaLabel="Refresh rate" value="auto" options={OPTIONS} onValueChange={vi.fn()} />,
    );
    const trigger = screen.getByRole('combobox');
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    await act(async () => {});

    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' });
    await act(async () => {});

    expect(screen.queryByRole('listbox'), 'Escape did not close the list').toBeNull();
    expect(document.activeElement, 'focus did not return to the select trigger').toBe(trigger);
  });

  it('renders grouped options under their section labels', async () => {
    render(
      <Select
        ariaLabel="Adapter"
        value="eth0"
        onValueChange={vi.fn()}
        options={[
          { label: 'Wired', options: [{ value: 'eth0', label: 'Ethernet' }] },
          { label: 'Wireless', options: [{ value: 'wlan0', label: 'Wi-Fi' }] },
        ]}
      />,
    );
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
    await act(async () => {});

    expect(screen.getByRole('option', { name: 'Ethernet' })).toBeTruthy();
    expect(screen.getByRole('option', { name: 'Wi-Fi' })).toBeTruthy();
  });
});

describe('ScrollArea', () => {
  it('names its focusable viewport, which is otherwise announced as a bare group', () => {
    render(
      <ScrollArea ariaLabel="Sidebar">
        <p>content</p>
      </ScrollArea>,
    );
    expect(
      screen.getByLabelText('Sidebar'),
      'the scrollable region is keyboard focusable but has no name',
    ).toBeTruthy();
  });
});
