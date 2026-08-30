import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { DialogContent, DialogRoot, DialogTrigger } from './Dialog';
import { Button } from './Button';

afterEach(cleanup);

function Fixture() {
  return (
    <DialogRoot>
      <DialogTrigger asChild>
        <Button>Open</Button>
      </DialogTrigger>
      <DialogContent
        title="Terminate process"
        description="This cannot be undone."
        closeLabel="Close"
      >
        <Button>Inside</Button>
      </DialogContent>
    </DialogRoot>
  );
}

describe('Dialog', () => {
  it('is named and described, so it is not announced as an anonymous "dialog"', () => {
    render(<Fixture />);
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));

    const dialog = screen.getByRole('dialog', { name: 'Terminate process' });
    const describedBy = dialog.getAttribute('aria-describedby');
    expect(describedBy, 'the dialog description was not linked with aria-describedby').toBeTruthy();
    expect(
      document.getElementById(describedBy ?? '')?.textContent,
      'aria-describedby points at an element that does not exist',
    ).toBe('This cannot be undone.');
  });

  it('moves focus into the dialog on open, so the keyboard is not left behind it', async () => {
    render(<Fixture />);
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));

    // Radix focuses on a microtask after mount; flush before asserting.
    await act(async () => {});

    const dialog = screen.getByRole('dialog');
    expect(
      dialog.contains(document.activeElement),
      'focus stayed outside the dialog — a keyboard user would still be tabbing the page behind it',
    ).toBe(true);
  });

  it('returns focus to the trigger on close, so the user resumes where they were', async () => {
    render(<Fixture />);
    const trigger = screen.getByRole('button', { name: 'Open' });

    fireEvent.click(trigger);
    await act(async () => {});

    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await act(async () => {});

    // Radix restores focus when the focus scope unmounts, which happens after
    // the exit transition rather than on the keypress. Give the frame a chance
    // to run before asserting, or this races the restore rather than testing it.
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(
      document.activeElement,
      'focus was not restored to the trigger; the user is dumped at the top of the document',
    ).toBe(trigger);
  });

  it('closes on Escape', async () => {
    render(<Fixture />);
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    await act(async () => {});

    fireEvent.keyDown(document.activeElement ?? document.body, { key: 'Escape' });
    await act(async () => {});

    expect(screen.queryByRole('dialog'), 'Escape did not dismiss the dialog').toBeNull();
  });

  it('marks the rest of the page inert, which is what actually contains a screen reader', async () => {
    render(
      <div>
        <p data-testid="outside">Background content</p>
        <Fixture />
      </div>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    await act(async () => {});

    // Radix hides siblings of the portal at the body level; walk up from the
    // outside node until an aria-hidden ancestor is found.
    let node: HTMLElement | null = screen.getByTestId('outside');
    let hidden = false;
    while (node !== null && node !== document.body) {
      if (node.getAttribute('aria-hidden') === 'true') {
        hidden = true;
        break;
      }
      node = node.parentElement;
    }
    expect(
      hidden,
      'content behind the modal was not aria-hidden — a screen reader cursor can escape the dialog',
    ).toBe(true);
  });

  it('exposes a close control with the caller-supplied label', () => {
    render(<Fixture />);
    fireEvent.click(screen.getByRole('button', { name: 'Open' }));
    expect(
      screen.getByRole('button', { name: 'Close' }),
      'the close button did not use the closeLabel prop',
    ).toBeTruthy();
  });
});
