/**
 * The widget overlay's configuration model.
 *
 * A widget is a small always-on-top window showing one reading. The user may
 * place several, each with its own content, size, look and position, so the
 * persisted shape is a list of these records — one per window.
 *
 * This module is pure: no React, no Tauri, no DOM. That is deliberate. The
 * rules that decide what a widget *is* — which kinds exist, how big a "large
 * CPU" widget is, what happens when the stored JSON is nonsense — are the
 * rules most likely to be wrong and the cheapest to test, so they live
 * somewhere a unit test can reach them without a window.
 *
 * The matching Rust struct is `WidgetSpec` in
 * `apps/desktop/src-tauri/src/widgets.rs`. It carries exactly the fields the
 * window host needs (geometry and the two window flags) plus the content
 * fields it hands back to the overlay; `widget_spec_matches_the_frontend`
 * there pins the field names so the two cannot drift silently.
 */

/** What a widget shows. */
export const widgetKinds = ['cpu', 'memory', 'disk', 'network', 'thermal', 'overview'] as const;
export type WidgetKind = (typeof widgetKinds)[number];

/** How much of it fits on screen. */
export const widgetSizes = ['small', 'medium', 'large'] as const;
export type WidgetSize = (typeof widgetSizes)[number];

/**
 * The panel's look.
 *
 * `plain` is a nearly opaque panel that reads over anything; `glass` is
 * lighter with a hairline edge, meant for someone who wants the desktop to
 * show through. Both avoid `backdrop-filter`: on a transparent Windows
 * webview it is expensive and renders differently over a full-screen game,
 * which is exactly where an overlay earns its keep (see `hud.rs`).
 */
export const widgetStyles = ['plain', 'glass'] as const;
export type WidgetStyle = (typeof widgetStyles)[number];

/** How the readings are arranged inside the panel. */
export const widgetLayouts = ['stack', 'split', 'ring'] as const;
export type WidgetLayout = (typeof widgetLayouts)[number];

/**
 * One placed widget, as persisted.
 *
 * Deliberately does NOT carry pixel dimensions: those follow from `size`, and
 * storing them would let a record disagree with the size the user picked after
 * a future change to the table. `geometryFor` derives them at the moment the
 * window is created.
 */
export interface WidgetInstance {
  readonly id: string;
  readonly kind: WidgetKind;
  readonly size: WidgetSize;
  readonly style: WidgetStyle;
  readonly layout: WidgetLayout;
  /** Panel opacity, `MIN_OPACITY`..1. Below that the numbers stop reading. */
  readonly opacity: number;
  readonly alwaysOnTop: boolean;
  readonly clickThrough: boolean;
  /** Logical screen coordinates of the window's top-left corner. */
  readonly x: number;
  readonly y: number;
  /**
   * The other machine whose readings this widget shows, or `null` for this
   * one.
   *
   * A machine's identity, not its address: the address and its token live in
   * the remote registry (`remote.ts`), so moving a pairing to a new port does
   * not rewrite every widget that watches it. A widget whose machine is gone
   * from that registry keeps its record and shows an honest "unreachable"
   * rather than silently falling back to this machine, which would be a widget
   * confidently showing the wrong computer's numbers.
   */
  readonly remoteConnectionId: string | null;
}

/** The list of placed widgets, as persisted. */
export type WidgetRegistry = readonly WidgetInstance[];

/** Opacity floor. A panel lighter than this is unreadable over a bright desktop. */
export const MIN_OPACITY = 0.25;

/**
 * How many widgets one machine may place.
 *
 * Each one is a separate webview process, so this is a real resource budget
 * rather than an arbitrary round number: a dozen widgets is already a dozen
 * renderers sitting on top of whatever is running underneath. The cap is
 * enforced on load as well as on add, so a hand-edited settings file cannot
 * smuggle past it.
 */
export const MAX_WIDGETS = 12;

/** Logical size of the window, in CSS pixels, for a given size step. */
const GEOMETRY: Record<WidgetSize, { readonly width: number; readonly height: number }> = {
  small: { width: 190, height: 120 },
  medium: { width: 280, height: 170 },
  large: { width: 400, height: 260 },
};

/** The window size for a size step. */
export function geometryFor(size: WidgetSize): { readonly width: number; readonly height: number } {
  return GEOMETRY[size];
}

/**
 * Where a new widget goes when the user has not moved it yet.
 *
 * A cascade rather than dead centre: several widgets stacked on the same spot
 * would look like one, and the first thing the user would have to do is drag
 * them apart. Wraps after eight steps so the tenth widget is still on screen.
 */
const CASCADE_STEP = 28;
const CASCADE_WRAP = 8;
const CASCADE_ORIGIN = { x: 80, y: 80 };

export function defaultPosition(index: number): { readonly x: number; readonly y: number } {
  const step = index % CASCADE_WRAP;
  return { x: CASCADE_ORIGIN.x + step * CASCADE_STEP, y: CASCADE_ORIGIN.y + step * CASCADE_STEP };
}

/**
 * The layout a kind gets before the user picks one.
 *
 * CPU is a percentage with a trace, so the ring reads best at a glance;
 * everything else opens as rows. This is a default, never a restriction — the
 * user may choose any layout the kind offers (`layoutsFor`).
 */
export function defaultLayoutFor(kind: WidgetKind): WidgetLayout {
  return kind === 'cpu' ? 'ring' : 'stack';
}

/**
 * The layouts offered for a kind.
 *
 * Not every arrangement suits every reading: a ring gauge needs one number
 * that reads 0–100, so a widget whose content is three byte counters offers
 * the two row layouts and not the ring. Offering a layout that renders an
 * empty gauge is how a settings screen teaches people it cannot be trusted.
 */
export function layoutsFor(kind: WidgetKind): readonly WidgetLayout[] {
  return kind === 'cpu' || kind === 'memory' ? ['ring', 'stack', 'split'] : ['stack', 'split'];
}

/** Whether a layout is meaningful for a kind. */
export function layoutFits(kind: WidgetKind, layout: WidgetLayout): boolean {
  return layoutsFor(kind).includes(layout);
}

/** Clamps an opacity into the readable range. */
export function clampOpacity(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.min(1, Math.max(MIN_OPACITY, value));
}

/**
 * Whether the animation should run.
 *
 * One place decides, so no component has to remember: reduced motion means
 * instant state changes, everywhere, including the morph between a widget's
 * compact and expanded forms.
 */
export function shouldAnimate(reducedMotion: boolean): boolean {
  return !reducedMotion;
}

/** A counter that makes generated ids unique within a session. */
let counter = 0;

/**
 * A fresh identifier.
 *
 * Not a UUID: the id becomes part of the window label (`widget-<id>`), which
 * Tauri requires to be a stable, URL-safe string, and a short one keeps the
 * label readable in the logs when a window misbehaves. Uniqueness within one
 * machine's registry is all that is needed, and the timestamp plus a counter
 * gives that even if two are created in the same millisecond.
 */
export function newWidgetId(now: number = Date.now()): string {
  counter += 1;
  return `w${now.toString(36)}${counter.toString(36)}`;
}

/**
 * Adds a widget of `kind`, placed after whatever is already there.
 *
 * Returns the SAME array when the cap is reached, rather than one the caller
 * has to compare — the store uses identity to decide whether to persist.
 */
export function addWidget(
  registry: WidgetRegistry,
  kind: WidgetKind,
  id: string = newWidgetId(),
): WidgetRegistry {
  if (registry.length >= MAX_WIDGETS) return registry;
  const position = defaultPosition(registry.length);
  return [
    ...registry,
    {
      id,
      kind,
      size: 'small',
      style: 'plain',
      layout: defaultLayoutFor(kind),
      opacity: 1,
      alwaysOnTop: true,
      clickThrough: false,
      x: position.x,
      y: position.y,
      remoteConnectionId: null,
    },
  ];
}

/** Removes a widget by id. Returns the same array when nothing matched. */
export function removeWidget(registry: WidgetRegistry, id: string): WidgetRegistry {
  const next = registry.filter((widget) => widget.id !== id);
  return next.length === registry.length ? registry : next;
}

/**
 * Whether two records describe the same widget.
 *
 * Used to make an update idempotent. The window host applies geometry on every
 * sync, so a store that persisted and pushed an unchanged list would ask the
 * host to move a window the user is dragging — fighting them, one frame behind.
 */
export function sameWidget(a: WidgetInstance, b: WidgetInstance): boolean {
  return (
    a.id === b.id &&
    a.kind === b.kind &&
    a.size === b.size &&
    a.style === b.style &&
    a.layout === b.layout &&
    a.opacity === b.opacity &&
    a.alwaysOnTop === b.alwaysOnTop &&
    a.clickThrough === b.clickThrough &&
    a.x === b.x &&
    a.y === b.y &&
    a.remoteConnectionId === b.remoteConnectionId
  );
}

/**
 * Whether a widget shows another machine.
 *
 * Named once so no renderer has to remember that "no machine" is `null` rather
 * than an empty string: a record parsed from a hand-edited file could hold
 * either, and `parseWidgets` normalises the empty one away.
 */
export function isRemoteWidget(widget: WidgetInstance): boolean {
  return widget.remoteConnectionId !== null;
}

/** Whether two registries are identical, in order and content. */
export function sameRegistry(a: WidgetRegistry, b: WidgetRegistry): boolean {
  return (
    a.length === b.length &&
    a.every((widget, index) => {
      const other = b[index];
      return other !== undefined && sameWidget(widget, other);
    })
  );
}

/**
 * Applies a change to one widget.
 *
 * A patch cannot change `id` or `kind`: the first is the window's identity and
 * the second would silently repoint a placed widget at different content,
 * which reads as the widget having lost its configuration. Changing content is
 * a remove-and-add, and the settings screen presents it that way.
 *
 * Returns the same array when the change is a no-op, so an idempotent caller
 * does not trigger a persist.
 */
export function updateWidget(
  registry: WidgetRegistry,
  id: string,
  patch: Partial<Omit<WidgetInstance, 'id' | 'kind'>>,
): WidgetRegistry {
  let changed = false;
  const next = registry.map((widget) => {
    if (widget.id !== id) return widget;
    const merged: WidgetInstance = { ...widget, ...patch };
    // Keep a layout the kind can actually draw. A kind given a layout it does
    // not offer would render an empty panel; falling back is better than
    // dropping the widget, which would look like the edit failed.
    const resolved: WidgetInstance =
      merged.layout !== undefined && layoutFits(widget.kind, merged.layout)
        ? merged
        : { ...merged, layout: defaultLayoutFor(widget.kind) };
    const clamped: WidgetInstance = { ...resolved, opacity: clampOpacity(resolved.opacity) };
    if (sameWidget(widget, clamped)) return widget;
    changed = true;
    return clamped;
  });
  return changed ? next : registry;
}

/* ------------------------------------------------------------------ */
/* Parsing                                                             */
/* ------------------------------------------------------------------ */

function asRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asMember<T extends string>(value: unknown, allowed: readonly T[]): T | null {
  return typeof value === 'string' && (allowed as readonly string[]).includes(value)
    ? (value as T)
    : null;
}

function asBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function asCoordinate(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

/** A stored machine id, or `null` for anything that is not a usable one. */
function asConnectionId(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed === '' ? null : trimmed;
}

/**
 * Reads a persisted registry, keeping every widget that can be drawn.
 *
 * Lenient about the things a person would not expect to lose a widget over —
 * an opacity outside the range, a size added by a newer version, a missing
 * position — and strict about the things that make a record meaningless: a
 * kind this build cannot render, or an empty or duplicated id, which would
 * mean two windows claiming the same label.
 *
 * A hand-edited or half-written settings file is the realistic input here, so
 * "recover as much as possible" beats "reject the whole list".
 */
export function parseWidgets(value: unknown): WidgetRegistry {
  if (!Array.isArray(value)) return [];

  const seen = new Set<string>();
  const widgets: WidgetInstance[] = [];

  for (const entry of value) {
    if (widgets.length >= MAX_WIDGETS) break;
    const record = asRecord(entry);
    if (record === null) continue;

    const id = typeof record['id'] === 'string' ? record['id'].trim() : '';
    if (id === '' || seen.has(id)) continue;

    const kind = asMember(record['kind'], widgetKinds);
    if (kind === null) continue;

    seen.add(id);
    const index = widgets.length;
    const position = defaultPosition(index);
    const size = asMember(record['size'], widgetSizes) ?? 'small';
    const style = asMember(record['style'], widgetStyles) ?? 'plain';
    const storedLayout = asMember(record['layout'], widgetLayouts);
    const layout =
      storedLayout !== null && layoutFits(kind, storedLayout)
        ? storedLayout
        : defaultLayoutFor(kind);

    widgets.push({
      id,
      kind,
      size,
      style,
      layout,
      opacity: clampOpacity(typeof record['opacity'] === 'number' ? record['opacity'] : 1),
      alwaysOnTop: asBoolean(record['alwaysOnTop'], true),
      clickThrough: asBoolean(record['clickThrough'], false),
      x: asCoordinate(record['x'], position.x),
      y: asCoordinate(record['y'], position.y),
      // Trimmed and dropped when empty: `""` would be a machine id that
      // matches nothing and reads as "remote but unknown", which is a worse
      // lie than "this machine".
      remoteConnectionId: asConnectionId(record['remoteConnectionId']),
    });
  }

  return widgets;
}

/* ------------------------------------------------------------------ */
/* Window spec                                                         */
/* ------------------------------------------------------------------ */

/**
 * What the window host is told for one widget.
 *
 * The same fields as a `WidgetInstance` plus the geometry the host needs to
 * size the window. The host deliberately does not carry the size table: if it
 * did, changing a widget's dimensions would mean editing Rust and TypeScript
 * in step, and a mismatch would be a subtly wrong window rather than a
 * compile error.
 */
export interface WidgetWindowSpec extends WidgetInstance {
  readonly width: number;
  readonly height: number;
}

/** Adds the resolved geometry to a widget for the window host. */
export function toWindowSpec(widget: WidgetInstance): WidgetWindowSpec {
  const { width, height } = geometryFor(widget.size);
  return { ...widget, width, height };
}

/** The window label for a widget. Must match `label_for` in `widgets.rs`. */
export const WIDGET_LABEL_PREFIX = 'widget-';

export function widgetLabel(id: string): string {
  return `${WIDGET_LABEL_PREFIX}${id}`;
}

/** Recovers the widget id from a window label, or `null` for any other window. */
export function widgetIdFromLabel(label: string): string | null {
  if (!label.startsWith(WIDGET_LABEL_PREFIX)) return null;
  const id = label.slice(WIDGET_LABEL_PREFIX.length);
  return id === '' ? null : id;
}
