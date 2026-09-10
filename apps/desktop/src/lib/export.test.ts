import { describe, expect, it, vi } from 'vitest';

import { csvField, exportFilename, saveExport, toCsv, toJson, type ExportColumn } from './export';

interface Row {
  readonly name: string;
  readonly cpu: number | null;
  readonly note: string | null;
}

const columns: readonly ExportColumn<Row>[] = [
  { id: 'name', header: 'Name', value: (row) => row.name },
  { id: 'cpuFraction', header: 'CPU', value: (row) => row.cpu },
  { id: 'note', header: 'Note, with comma', value: (row) => row.note },
];

describe('csvField', () => {
  it('quotes a field containing a comma, a quote or a newline, and doubles embedded quotes', () => {
    expect(csvField('Adobe Acrobat, Reader')).toBe('"Adobe Acrobat, Reader"');
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField('two\nlines')).toBe('"two\nlines"');
    expect(csvField('cr\rhere')).toBe('"cr\rhere"');
  });

  it('leaves a plain field, a Windows path and a number unquoted', () => {
    expect(csvField('chrome.exe')).toBe('chrome.exe');
    expect(csvField('C:\\Program Files\\Thing')).toBe('C:\\Program Files\\Thing');
    expect(csvField(0.0723)).toBe('0.0723');
    expect(csvField(true)).toBe('true');
  });

  it('exports null as an empty cell, never as 0 or a dash', () => {
    expect(csvField(null)).toBe('');
  });
});

describe('toCsv', () => {
  const rows: readonly Row[] = [
    { name: 'chrome.exe', cpu: 0.0723, note: null },
    { name: 'Thing, Inc', cpu: null, note: 'has "quotes"' },
  ];

  it('starts with a UTF-8 BOM so Excel does not read diacritics as ANSI', () => {
    expect(toCsv(rows, columns).codePointAt(0)).toBe(0xfeff);
  });

  it('ends every row with CRLF, including the last', () => {
    const csv = toCsv(rows, columns);
    const body = csv.slice(1);
    expect(body.endsWith('\r\n')).toBe(true);
    // Three records: header plus two rows. No bare LF anywhere.
    expect(body.split('\r\n')).toHaveLength(4);
    expect(body.replaceAll('\r\n', '')).not.toContain('\n');
  });

  it('writes the translated header and raw values, not formatted display strings', () => {
    const [header, first, second] = toCsv(rows, columns).slice(1).split('\r\n');
    expect(header).toBe('Name,CPU,"Note, with comma"');
    expect(first).toBe('chrome.exe,0.0723,');
    expect(second).toBe('"Thing, Inc",,"has ""quotes"""');
  });

  it('preserves Romanian diacritics byte-for-byte', () => {
    const csv = toCsv([{ name: 'Aplicații instalate', cpu: 1, note: 'ăâîșț' }], columns);
    expect(csv).toContain('Aplicații instalate,1,ăâîșț');
  });
});

describe('toJson', () => {
  it('keys by stable column id, keeps null as null and numbers as numbers', () => {
    const parsed: unknown = JSON.parse(
      toJson([{ name: 'chrome.exe', cpu: 0.0723, note: null }], columns),
    );
    expect(parsed).toEqual([{ name: 'chrome.exe', cpuFraction: 0.0723, note: null }]);
  });

  it('is valid JSON for an empty table', () => {
    expect(JSON.parse(toJson([], columns))).toEqual([]);
  });
});

describe('exportFilename', () => {
  it('is sortable, legal on NTFS and carries the kind as extension', () => {
    const name = exportFilename('processes', 'csv', new Date('2026-09-10T14:32:05.123Z'));
    expect(name).toBe('vitals-processes-2026-09-10T14-32-05Z.csv');
    expect(name).not.toMatch(/[:*?"<>|]/);
  });
});

describe('saveExport', () => {
  it('downloads via an anchor click with the file name and revokes the URL afterwards', () => {
    vi.useFakeTimers();
    const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:vitals/1');
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      expect(this.download).toBe('out.csv');
      expect(this.href).toBe('blob:vitals/1');
    });

    saveExport('out.csv', '\uFEFFa,b\r\n', 'csv');

    expect(click).toHaveBeenCalledOnce();
    const blob = create.mock.calls[0]?.[0];
    expect(blob).toBeInstanceOf(Blob);
    expect((blob as Blob).type).toBe('text/csv;charset=utf-8');
    // Not revoked synchronously: that races the engine's read of the blob.
    expect(revoke).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(revoke).toHaveBeenCalledWith('blob:vitals/1');

    vi.restoreAllMocks();
    vi.useRealTimers();
  });
});
