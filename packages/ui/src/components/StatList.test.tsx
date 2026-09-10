import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { StatList, type Stat } from './StatList';

afterEach(cleanup);

const stats: readonly Stat[] = [
  { key: 'used', label: 'In use', value: '7.2 GB' },
  { key: 'smart', label: 'Drive health', value: null },
];

describe('StatList', () => {
  it('renders an unmeasured value as an em dash named "unavailable", never as a zero or a blank', () => {
    render(<StatList stats={stats} unavailableLabel="Not available" />);

    const dash = screen.getByLabelText('Not available');
    expect(dash.textContent).toBe('—');
    // The label still appears so the reader knows WHICH field is missing.
    expect(screen.getByText('Drive health')).toBeTruthy();
    expect(screen.queryByText('0')).toBeNull();
  });

  it('drops the row entirely under omitNull, so a machine with no optional sensors shows no column of dashes', () => {
    render(<StatList stats={stats} omitNull />);

    expect(screen.queryByText('Drive health')).toBeNull();
    expect(screen.queryByText('—')).toBeNull();
    expect(screen.getByText('In use')).toBeTruthy();
  });

  it('renders nothing at all when every row is omitted, rather than an empty list element', () => {
    const { container } = render(
      <StatList stats={[{ key: 'a', label: 'A', value: null }]} omitNull />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('pairs each label with its value as a definition term and description', () => {
    render(<StatList stats={stats} />);

    const term = screen.getByText('In use');
    expect(term.tagName).toBe('DT');
    const value = screen.getByText('7.2 GB');
    expect(value.closest('dd')).not.toBeNull();
  });

  it('shows a hint beneath the value it explains, and only there', () => {
    render(
      <StatList
        stats={[
          { key: 'cached', label: 'Cached', value: '3 GB', hint: 'Freed instantly when needed' },
          { key: 'free', label: 'Free', value: '1 GB' },
        ]}
      />,
    );

    const hint = screen.getByText('Freed instantly when needed');
    expect(hint.tagName).toBe('DD');
    // Same group as the value it belongs to — a hint floating under a
    // different number is worse than no hint.
    expect(hint.parentElement).toBe(screen.getByText('3 GB').closest('div'));
    expect(screen.getByText('Free').parentElement?.querySelectorAll('dd')).toHaveLength(1);
  });
});
