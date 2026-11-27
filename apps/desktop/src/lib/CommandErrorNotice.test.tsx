import { render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { CommandErrorNotice } from './CommandErrorNotice';

beforeAll(async () => {
  await initI18n();
});
beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('CommandErrorNotice', () => {
  // A realistic `CommandError`: a stable kind plus a Rust-authored message.
  const rejected = { kind: 'access-denied', message: 'open process 6440 requires elevation' };

  it('shows the sentence for the kind rather than the backend raw text', () => {
    render(<CommandErrorNotice error={rejected} />);
    expect(screen.getByText(/Running Vitals as an administrator usually fixes it\./)).toBeTruthy();
    // The raw line is not the sentence, so it must not be the visible text.
    expect(screen.queryByText('open process 6440 requires elevation')).not.toBeNull();
  });

  it('keeps the backend raw text reachable under Details', () => {
    render(<CommandErrorNotice error={rejected} />);
    expect(screen.getByText('Details')).toBeTruthy();
    expect(screen.getByText('open process 6440 requires elevation')).toBeTruthy();
  });

  it('shows an unknown rejection its own message and no disclosure', () => {
    render(<CommandErrorNotice error={new Error('the helper is not installed')} />);
    expect(screen.getByText('the helper is not installed')).toBeTruthy();
    // No kind means there is no sentence, and an empty disclosure would be
    // worse than none.
    expect(screen.queryByText('Details')).toBeNull();
  });

  it('renders no disclosure when the raw text already is the sentence', () => {
    render(
      <CommandErrorNotice
        error={{ kind: 'refused', message: 'The system refused the request.' }}
      />,
    );
    expect(screen.getByText('The system refused the request.')).toBeTruthy();
    expect(screen.queryByText('Details')).toBeNull();
  });
});
