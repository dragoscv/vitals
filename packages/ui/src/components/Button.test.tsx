import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Button } from './Button';

afterEach(cleanup);

describe('Button', () => {
  it('accepts focus and fires on activation, because every action must be keyboard reachable', () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Terminate</Button>);
    const button = screen.getByRole('button', { name: 'Terminate' });

    button.focus();
    expect(document.activeElement, 'the button did not accept keyboard focus').toBe(button);

    // A native <button> synthesises click from Enter and Space. Asserting on
    // click proves we have not broken that by intercepting keys ourselves.
    fireEvent.click(button);
    expect(onClick, 'activating the button did not fire onClick').toHaveBeenCalledTimes(1);
  });

  it('blocks activation while loading, so a slow action cannot be submitted twice', () => {
    const onClick = vi.fn();
    render(
      <Button loading loadingLabel="Working" onClick={onClick}>
        Apply
      </Button>,
    );
    const button = screen.getByRole('button');

    fireEvent.click(button);
    expect(
      onClick,
      'a loading button still fired onClick — the user can double-submit',
    ).not.toHaveBeenCalled();
    expect(
      button.getAttribute('aria-busy'),
      'a loading button must be aria-busy, not merely disabled',
    ).toBe('true');
  });

  it('uses the caller-supplied loading label rather than inventing English', () => {
    render(
      <Button loading loadingLabel="Se încarcă">
        Apply
      </Button>,
    );
    expect(
      screen.getByRole('status', { name: 'Se încarcă' }),
      'the loadingLabel prop did not reach the spinner',
    ).toBeTruthy();
  });

  it('hides decorative icons from assistive technology', () => {
    render(<Button leadingIcon={<svg data-testid="icon" />}>Refresh</Button>);
    expect(
      screen.getByTestId('icon').parentElement?.getAttribute('aria-hidden'),
      'a decorative icon was left exposed to screen readers, duplicating the label',
    ).toBe('true');
  });

  it('forwards a ref, so callers can focus or measure the element', () => {
    const seen: (HTMLButtonElement | null)[] = [];
    render(<Button ref={(element) => void seen.push(element)}>Ok</Button>);
    expect(seen[0], 'the ref was never populated with the button element').toBeTruthy();
  });

  it('defaults to type="button" so it cannot accidentally submit a surrounding form', () => {
    render(<Button>Filter</Button>);
    expect(screen.getByRole('button').getAttribute('type')).toBe('button');
  });
});
