import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { VitalsError } from '@vitals/client';
import { initI18n } from '@vitals/i18n';
import type { Process } from '@vitals/protocol';

import { ActionSheet } from './ActionSheet';

const process = {
  key: { pid: 4242, startTime: 1_700_000_000_000 },
  name: 'notepad.exe',
  cpu: 1,
  memoryPrivate: 1024,
} as unknown as Process;

beforeAll(async () => {
  await initI18n('en');
});

describe('ActionSheet', () => {
  it('does not call control() on the first tap of End task, and sends pid plus startTime on the second', async () => {
    const control = vi.fn(() => Promise.resolve());
    render(
      <ActionSheet
        process={process}
        control={control}
        onClose={() => {}}
        onSettled={() => {}}
        readOnly={false}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'End task' }));
    expect(control).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'End notepad.exe' }));
    await waitFor(() => expect(control).toHaveBeenCalledTimes(1));
    expect(control).toHaveBeenCalledWith({
      action: 'terminate',
      key: { pid: 4242, startTime: 1_700_000_000_000 },
    });
  });

  it('shows the read-only explanation and reports it when the host answers 403', async () => {
    const control = vi.fn(() =>
      Promise.reject(
        new VitalsError('forbidden', 'forbidden', { status: 403, detail: { kind: 'forbidden' } }),
      ),
    );
    const onSettled = vi.fn();
    render(
      <ActionSheet
        process={process}
        control={control}
        onClose={() => {}}
        onSettled={onSettled}
        readOnly={false}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Suspend' }));

    await screen.findByRole('alert');
    expect(screen.getByRole('alert').textContent).toMatch(/paired read-only|Re-pair it/);
    expect(onSettled).toHaveBeenCalledWith({ readOnly: true });
  });

  it('hides every control once the machine is known to be read-only', () => {
    render(
      <ActionSheet
        process={process}
        control={vi.fn()}
        onClose={() => {}}
        onSettled={() => {}}
        readOnly
      />,
    );
    expect(screen.queryByRole('button', { name: 'End task' })).toBeNull();
    expect(screen.getByText('This phone was paired read-only')).toBeTruthy();
  });
});
