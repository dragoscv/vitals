/**
 * Turning a table into a file the user can open somewhere else.
 *
 * # Raw values, not what the cell says
 *
 * Every screen formats for reading: `7.2 %`, `1.4 GB`, `2 min 03 s`. Those are
 * good on screen and useless in a spreadsheet — a locale-formatted number is
 * text, and Romanian decimal commas turn a column of figures into a column of
 * strings that will not sum. So an export column carries an accessor that
 * returns the underlying value, and the formatting stays in the component.
 *
 * # Absent is empty, never zero
 *
 * The repository's one principle applies here more sharply than anywhere in
 * the UI, because a spreadsheet will happily average a column of fabricated
 * zeroes and produce a confident wrong answer. `null` becomes an empty cell in
 * CSV and stays `null` in JSON. It never becomes `0`, and it never becomes the
 * em dash the UI draws — an em dash in a numeric column is worse than blank,
 * since it silently makes the whole column textual.
 *
 * # The CSV is RFC 4180, not `join(',')`
 *
 * A naive join breaks on the first program named `Adobe Acrobat, Reader` and
 * on every Windows path, and the breakage is invisible until someone opens the
 * file and finds their columns shifted. Fields containing a comma, a quote or
 * a newline are quoted and embedded quotes doubled; rows end CRLF.
 *
 * The leading UTF-8 BOM is there for Excel specifically. Without it Excel
 * reads a UTF-8 CSV as the system ANSI code page, and every Romanian
 * diacritic — the `ș` in `Aplicații instalate`, every `ă` in a translated
 * header — arrives mojibaked. It costs three bytes.
 */

/**
 * One exported column.
 *
 * `id` and `header` are deliberately separate. The CSV header row is for a
 * human reading a spreadsheet, so it is the translated column label; the JSON
 * keys are for a program, so they are stable identifiers that do not change
 * when the user switches the app to Romanian. A single field would force one
 * of those two consumers to be wrong.
 */
export interface ExportColumn<Row> {
  /** Stable machine key. Used as the JSON property name. Never translated. */
  readonly id: string;
  /** Translated label. Used as the CSV header cell. */
  readonly header: string;
  /** The underlying value — never a formatted display string. */
  value: (row: Row) => ExportValue;
}

export type ExportValue = string | number | boolean | null;

export type ExportKind = 'csv' | 'json';

/** Excel reads a UTF-8 CSV as the ANSI code page without this. */
const BOM = '\uFEFF';

/** RFC 4180 says CRLF, and Excel on Windows agrees. */
const ROW_END = '\r\n';

/**
 * Quotes a single field if it needs it.
 *
 * A number is emitted with `String()` rather than any locale formatter: the
 * consumer wants `0.0723`, and `0,0723` is a different value in most of the
 * spreadsheet world.
 */
export function csvField(value: ExportValue): string {
  // Absent stays absent. An empty cell is the only honest rendering of a
  // reading that was never taken.
  if (value === null) return '';

  const text = typeof value === 'string' ? value : String(value);
  if (!/[",\r\n]/.test(text)) return text;
  return `"${text.replaceAll('"', '""')}"`;
}

/** Serialises rows to RFC 4180 CSV, with a BOM and a translated header row. */
export function toCsv<Row>(rows: readonly Row[], columns: readonly ExportColumn<Row>[]): string {
  const lines = [columns.map((column) => csvField(column.header)).join(',')];

  for (const row of rows) {
    lines.push(columns.map((column) => csvField(column.value(row))).join(','));
  }

  // Trailing CRLF: RFC 4180 permits it and several parsers, including Excel's,
  // are happier with a terminated final record than with a truncated one.
  return BOM + lines.join(ROW_END) + ROW_END;
}

/**
 * Serialises rows to a JSON array of objects keyed by column `id`.
 *
 * Indented two spaces. This is written to be read by a person pasting it into
 * a bug report at least as often as by a parser, and the size difference on a
 * few thousand rows does not matter for a file that is saved once.
 */
export function toJson<Row>(rows: readonly Row[], columns: readonly ExportColumn<Row>[]): string {
  const objects = rows.map((row) => {
    const record: Record<string, ExportValue> = {};
    for (const column of columns) record[column.id] = column.value(row);
    return record;
  });

  return `${JSON.stringify(objects, null, 2)}\n`;
}

/** `vitals-processes-2026-09-10T14-32-05.csv` — sortable, and legal on NTFS. */
export function exportFilename(name: string, kind: ExportKind, now = new Date()): string {
  // Colons are illegal in a Windows filename and dots before the extension
  // confuse the shell's type association, so the ISO stamp is flattened.
  const stamp = now
    .toISOString()
    .replace(/[:.]/g, '-')
    .replace(/-\d{3}Z$/, 'Z');
  return `vitals-${name}-${stamp}.${kind}`;
}

const MIME: Record<ExportKind, string> = {
  // `text/csv` alone makes some hosts sniff the encoding; the charset is
  // explicit because the BOM is only half the story for a consumer that reads
  // the header rather than the bytes.
  csv: 'text/csv;charset=utf-8',
  json: 'application/json;charset=utf-8',
};

/**
 * Writes the export to wherever the user's browser engine puts downloads.
 *
 * # Why not a native save dialog
 *
 * Rust owns the filesystem everywhere else in this app — `write_flight_recording`
 * takes a path from `@tauri-apps/plugin-dialog`'s `save()` and does the write
 * in the backend, precisely so the webview never gets arbitrary write access.
 * That is the right shape and this should eventually use it, but it needs a
 * backend change and this module may not make one.
 *
 * `dialog:allow-save` is already granted, so the *picker* would work today —
 * but a path with nothing able to write to it is worse than no picker at all:
 * the user chooses a location, and no file appears. So this does not call it.
 *
 * To move this to a native save dialog, one of the following is needed:
 *
 *  - a Rust command, e.g. `write_export(path: String, contents: String)`, added
 *    to `apps/desktop/src-tauri/src/store.rs` and registered in
 *    `generate_handler!`. No new capability string is required — the existing
 *    `dialog:allow-save` covers the picker and `core:default` covers `invoke`.
 *    This is the option that matches how the rest of the app works.
 *  - or the filesystem plugin, which would need `@tauri-apps/plugin-fs` as a
 *    dependency, `tauri-plugin-fs` registered in the builder, and these exact
 *    strings added to the `permissions` array of
 *    `apps/desktop/src-tauri/capabilities/default.json`:
 *      "fs:allow-write-text-file"
 *      { "identifier": "fs:scope", "allow": [{ "path": "$DOWNLOAD/**" }, { "path": "$DOCUMENT/**" }] }
 *    Broader than the first option and it hands the webview a write primitive,
 *    which is why it is listed second.
 *
 * Until then: a Blob and an anchor click. WebView2 handles this natively and
 * routes it through the user's Downloads folder with its own prompt, so
 * nothing is written silently and nothing is lost.
 */
export function saveExport(filename: string, contents: string, kind: ExportKind): void {
  const blob = new Blob([contents], { type: MIME[kind] });
  const url = URL.createObjectURL(blob);

  try {
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    // Never attached to the document. An orphan anchor is clickable and leaves
    // no chance of a stray element surviving in the tree if the click throws.
    anchor.click();
  } finally {
    // Deferred one turn: revoking synchronously races the engine's read of the
    // blob, and the failure mode is a zero-byte download that looks like the
    // export produced nothing.
    setTimeout(() => {
      URL.revokeObjectURL(url);
    }, 0);
  }
}
