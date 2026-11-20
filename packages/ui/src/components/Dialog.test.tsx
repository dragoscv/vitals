import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { DIALOG_MAX_WIDTH_REM, DialogContent, DialogRoot, DialogTrigger } from './Dialog';
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
  it('caps every dialog at a fixed length, never at a container token the app can redefine', () => {
    const sizes = ['sm', 'md', 'lg', 'xl', '2xl'] as const;
    const widths: number[] = [];

    for (const size of sizes) {
      render(
        <DialogRoot open>
          <DialogContent size={size} title="Width" closeLabel="Close">
            <p>body</p>
          </DialogContent>
        </DialogRoot>,
      );
      const dialog = screen.getByRole('dialog', { name: 'Width' });

      // `max-w-3xl` is 1920 px in this app: the desktop stylesheet redefines
      // `--container-3xl` to 120rem for its `@3xl/main:` container variants,
      // and Tailwind resolves that same token for the width utility. A size
      // that reaches for it stretches the dialog across a fullscreen window.
      expect(
        dialog.className,
        `size "${size}" rides Tailwind's container scale, which this app redefines to 1920 px`,
      ).not.toMatch(/\bmax-w-(3xl|4xl|5xl)\b/);

      const cap = /max-w-\[(\d+(?:\.\d+)?)rem\]/.exec(dialog.className);
      expect(
        cap,
        `size "${size}" has no fixed-length width cap, so a fullscreen window stretches it`,
      ).not.toBeNull();
      widths.push(Number(cap?.[1]));
      cleanup();
    }

    for (const width of widths) {
      expect(
        width,
        'a dialog size is wider than the widest dialog the design system documents',
      ).toBeLessThanOrEqual(DIALOG_MAX_WIDTH_REM);
    }
    // Strictly increasing: two sizes that differ only in name are a trap for
    // whoever picks one expecting the dialog to change width.
    expect(
      widths,
      `sizes are not ordered, so "wider" does not mean wider: ${widths.join()}`,
    ).toEqual([...widths].sort((a, b) => a - b));
    expect(
      Math.max(...widths),
      'the widest dialog is narrower than the documented maximum, so nothing can reach it',
    ).toBe(DIALOG_MAX_WIDTH_REM);
  });

  it('caps its height to the window and scrolls only the body, so the close button and footer never leave the screen', () => {
    render(
      <DialogRoot open>
        <DialogContent title="Long" closeLabel="Close" footer={<Button>Next</Button>}>
          <p>body</p>
        </DialogContent>
      </DialogRoot>,
    );
    const dialog = screen.getByRole('dialog', { name: 'Long' });
    // jsdom has no layout, so the guarantee is asserted on the classes that produce it.
    expect(
      dialog.className,
      'no window-height cap: a long dialog grows past the window edge',
    ).toMatch(/max-h-\[calc\(100dvh-2rem\)\]/);
    expect(dialog.className, 'the dialog itself must not scroll or overflow').toMatch(
      /\boverflow-hidden\b/,
    );
    const body = dialog.querySelector('[data-dialog-body]');
    expect(body, 'body wrapper missing').not.toBeNull();
    expect(body?.className, 'the body is not the scroll area').toMatch(/\boverflow-y-auto\b/);
    expect(
      body?.className,
      'the body cannot shrink below its content, so flex will not cap it',
    ).toMatch(/\bmin-h-0\b/);
    const footer = screen.getByRole('button', { name: 'Next' }).parentElement;
    expect(footer?.className, 'the footer shrinks and scrolls away').toMatch(/\bshrink-0\b/);
  });

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
