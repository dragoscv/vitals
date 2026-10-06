import { describe, expect, it } from 'vitest';

import {
  COLUMNS,
  COLUMN_BY_ID,
  DEFAULT_PREFERENCES,
  loadPreferences,
  mergePreferences,
  savePreferences,
  toggleColumn,
} from './columns';

describe('toggleColumn', () => {
  it('translates every value a translated column can produce, in both locales', async () => {
    // The Type column first shipped showing "process.kind.background": its
    // keys existed only as English fallbacks, never in a locale bundle.
    const { bundles } = await import('./strings');
    const values: Readonly<Record<string, readonly string[]>> = {
      'processes:kind.': ['app', 'background', 'service', 'system', 'containerized'],
      'processes:power.': ['veryLow', 'low', 'moderate', 'high', 'veryHigh'],
    };
    for (const column of COLUMNS) {
      const prefix = column.translate;
      if (prefix === undefined || !prefix.startsWith('processes:')) continue;
      const path = prefix.slice('processes:'.length, -1);
      for (const locale of ['en', 'ro'] as const) {
        const group = (bundles[locale] as unknown as Record<string, Record<string, string>>)[path];
        for (const value of values[prefix] ?? []) {
          expect(group?.[value], `${locale} ${prefix}${value}`).toBeTruthy();
        }
      }
    }
  });

  it('can show every column, including the ones that are off by default', () => {
    // Ticking Threads, Handles or Uptime used to do nothing at all.
    for (const column of COLUMNS) {
      const without = DEFAULT_PREFERENCES.visible.filter(
        (id) => id !== column.id || column.required,
      );
      expect(toggleColumn(without, column.id), column.id).toContain(column.id);
    }
  });

  it('hides a visible column and puts it back where it was', () => {
    const hidden = toggleColumn(DEFAULT_PREFERENCES.visible, 'memory');
    expect(hidden).not.toContain('memory');
    expect(toggleColumn(hidden, 'memory')).toEqual(DEFAULT_PREFERENCES.visible);
  });

  it('never removes a required column', () => {
    expect(toggleColumn(DEFAULT_PREFERENCES.visible, 'name')).toContain('name');
  });
});

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (key: string) => map.get(key) ?? null,
    key: (index: number) => [...map.keys()][index] ?? null,
    removeItem: (key: string) => map.delete(key),
    setItem: (key: string, value: string) => void map.set(key, value),
  };
}

describe('preferences', () => {
  it('round-trips through storage', () => {
    const storage = memoryStorage();
    savePreferences({ ...DEFAULT_PREFERENCES, sortColumn: 'memory', grouped: false }, storage);
    const loaded = loadPreferences(storage);
    expect(loaded.sortColumn).toBe('memory');
    expect(loaded.grouped).toBe(false);
  });

  it('falls back to defaults rather than throwing on corrupt data', () => {
    // The process list is what people open when their machine misbehaves. A
    // bad stored value must never be the reason it will not render.
    const storage = memoryStorage();
    storage.setItem('vitals.processes.preferences', '{not json');
    expect(loadPreferences(storage)).toEqual(DEFAULT_PREFERENCES);
  });

  it('re-adds required columns hidden by a stale stored value', () => {
    // A persisted state with no name column would be an unusable table with
    // no in-app route back to a usable one.
    const merged = mergePreferences({ visible: ['cpu'] });
    expect(merged.visible).toContain('name');
    expect(merged.visible).toContain('pid');
  });

  it('discards unknown column ids', () => {
    const merged = mergePreferences({ visible: ['cpu', 'not-a-column'] });
    expect(merged.visible.every((id) => COLUMN_BY_ID.has(id))).toBe(true);
  });

  it('clamps a stored width to the column minimum', () => {
    const merged = mergePreferences({ widths: { name: 1 } });
    expect(merged.widths.name).toBe(COLUMN_BY_ID.get('name')?.minWidth);
  });

  it('does not throw when storage rejects a write', () => {
    const hostile = {
      ...memoryStorage(),
      setItem: () => {
        throw new Error('quota');
      },
    } as Storage;
    expect(() => savePreferences(DEFAULT_PREFERENCES, hostile)).not.toThrow();
  });
});

describe('column rendering', () => {
  const row = {
    id: '1:1',
    depth: 0,
    childIds: [],
    rolledCpu: 12.5,
    rolledMemory: 0,
    rolledDisk: 0,
    rolledNetwork: 0,
    rolledGpu: null,
    rolledPower: 0,
    powerTrend: null,
    descendantCount: 0,
    process: {
      key: { pid: 1, startTime: 1 },
      parent: null,
      name: 'x.exe',
      kind: 'app',
      state: 'running',
      flags: 0,
      integrity: null,
      protection: 'none',
      cpu: 12.5,
      memoryPrivate: 0,
      memoryWorkingSet: 0,
      diskRead: 0,
      diskWrite: 0,
      netRx: 0,
      netTx: 0,
      gpu: null,
      gpuMemory: null,
      threadCount: 1,
      handleCount: null,
      user: null,
      description: null,
      uptimeSecs: 0,
    },
  } as const;

  it('renders an em-dash, never a zero, for absent readings', () => {
    expect(COLUMN_BY_ID.get('gpu')?.render(row, 'en')).toBe('—');
    expect(COLUMN_BY_ID.get('handles')?.render(row, 'en')).toBe('—');
    expect(COLUMN_BY_ID.get('user')?.render(row, 'en')).toBe('—');
  });
});
