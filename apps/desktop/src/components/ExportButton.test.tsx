import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { ExportButton } from './ExportButton';
import { bundles } from './exportStrings';
import type { ExportColumn } from '../lib/export';

beforeAll(async () => {
  await initI18n();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

interface Row {
  readonly name: string;
  readonly cpu: number | null;
}

const columns: readonly ExportColumn<Row>[] = [
  { id: 'name', header: 'Name', value: (row) => row.name },
  { id: 'cpu', header: 'CPU', value: (row) => row.cpu },
];

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('export translations', () => {
  it('defines the same keys in English and Romanian', () => {
    expect(keyPaths(bundles.ro).sort()).toEqual(keyPaths(bundles.en).sort());
  });
});

describe('ExportButton', () => {
  it('writes the visible rows as CSV with the raw number, not the display string', async () => {
    const save = vi.fn();
    render(
      <ExportButton
        name="probe"
        rows={[{ name: 'chrome.exe', cpu: 0.0723 }]}
        columns={columns}
        save={save}
      />,
    );

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Export' }));
    fireEvent.click(await screen.findByRole('menuitem', { name: /CSV/ }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    const [filename, contents, kind] = save.mock.calls[0] as [string, string, string];
    expect(filename).toMatch(/^vitals-probe-.*\.csv$/);
    expect(kind).toBe('csv');
    expect(contents).toBe('\uFEFFName,CPU\r\nchrome.exe,0.0723\r\n');
  });

  it('writes JSON keyed by column id when JSON is chosen', async () => {
    const save = vi.fn();
    render(
      <ExportButton name="probe" rows={[{ name: 'a', cpu: null }]} columns={columns} save={save} />,
    );

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Export' }));
    fireEvent.click(await screen.findByRole('menuitem', { name: /JSON/ }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledOnce();
    });
    const contents = save.mock.calls[0]?.[1] as string;
    expect(JSON.parse(contents)).toEqual([{ name: 'a', cpu: null }]);
  });

  it('is disabled, with the reason, when there is nothing to export', () => {
    render(<ExportButton name="probe" rows={[]} columns={columns} />);
    const button = screen.getByRole<HTMLButtonElement>('button', { name: 'Nothing to export' });
    expect(button.disabled).toBe(true);
    expect(button.title).toBe('The table is empty, so there is nothing to save.');
  });

  it('is translated when the app is in Romanian', async () => {
    await i18n.changeLanguage('ro');
    render(<ExportButton name="probe" rows={[{ name: 'a', cpu: 1 }]} columns={columns} />);
    expect(screen.getByRole('button', { name: 'Exportă' })).toBeTruthy();
  });
});
