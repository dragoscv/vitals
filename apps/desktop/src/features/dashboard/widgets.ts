/**
 * The widget catalogue and the user's layout.
 *
 * # Layout is a list of ids, not a grid of coordinates
 *
 * Dashboards usually persist `{ x, y, w, h }` per widget. That format has to
 * answer questions it cannot: what happens to a widget at x=8 when the window
 * is narrowed to three columns, or when a saved layout is restored on a
 * different monitor. The usual answers are overlap, clipping, or a reflow that
 * silently discards the arrangement anyway.
 *
 * Here a layout is an ordered list of enabled widget ids plus a per-widget
 * size hint ('half' | 'full'). CSS Grid does the packing, so the same saved
 * layout is correct at 1280px and at 5120px, and there is no state that can
 * describe an impossible arrangement. The cost is that widgets cannot be
 * placed at an arbitrary coordinate — which, for a fixed set of a dozen
 * panels, nobody has ever needed.
 *
 * # Every widget declares what it needs
 *
 * `requires` lets the dashboard hide a GPU widget on a machine with no
 * discrete GPU, and a battery widget on a desktop, without each widget
 * rendering an apologetic empty card. An unavailable widget is absent from the
 * picker rather than present and permanently blank.
 */

import type { SystemMetrics } from '@vitals/protocol';

export const widgetIds = [
  'cpu',
  'memory',
  'disk',
  'network',
  'gpu',
  'thermals',
  'battery',
  'topCpu',
  'topMemory',
  'uptime',
  'storage',
  'alerts',
] as const;

export type WidgetId = (typeof widgetIds)[number];

export function isWidgetId(value: unknown): value is WidgetId {
  return typeof value === 'string' && (widgetIds as readonly string[]).includes(value);
}

export const widgetSizes = ['half', 'full'] as const;
export type WidgetSize = (typeof widgetSizes)[number];

export function isWidgetSize(value: unknown): value is WidgetSize {
  return typeof value === 'string' && (widgetSizes as readonly string[]).includes(value);
}

/** What a widget needs from the machine in order to say anything at all. */
export type WidgetRequirement = 'always' | 'gpu' | 'battery' | 'thermal';

export interface WidgetDefinition {
  readonly id: WidgetId;
  /** Key into the `dashboard.widget.*` namespace. Resolved at render time. */
  readonly titleKey: string;
  readonly descriptionKey: string;
  readonly requires: WidgetRequirement;
  readonly defaultSize: WidgetSize;
  /** Widgets a full-width layout looks wrong without. Cannot be removed. */
  readonly essential: boolean;
}

export const widgetCatalogue: readonly WidgetDefinition[] = [
  {
    id: 'cpu',
    titleKey: 'widget.cpu.title',
    descriptionKey: 'widget.cpu.description',
    requires: 'always',
    defaultSize: 'half',
    essential: true,
  },
  {
    id: 'memory',
    titleKey: 'widget.memory.title',
    descriptionKey: 'widget.memory.description',
    requires: 'always',
    defaultSize: 'half',
    essential: true,
  },
  {
    id: 'disk',
    titleKey: 'widget.disk.title',
    descriptionKey: 'widget.disk.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'network',
    titleKey: 'widget.network.title',
    descriptionKey: 'widget.network.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'gpu',
    titleKey: 'widget.gpu.title',
    descriptionKey: 'widget.gpu.description',
    requires: 'gpu',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'thermals',
    titleKey: 'widget.thermals.title',
    descriptionKey: 'widget.thermals.description',
    requires: 'thermal',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'battery',
    titleKey: 'widget.battery.title',
    descriptionKey: 'widget.battery.description',
    requires: 'battery',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'topCpu',
    titleKey: 'widget.topCpu.title',
    descriptionKey: 'widget.topCpu.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'topMemory',
    titleKey: 'widget.topMemory.title',
    descriptionKey: 'widget.topMemory.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'uptime',
    titleKey: 'widget.uptime.title',
    descriptionKey: 'widget.uptime.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'storage',
    titleKey: 'widget.storage.title',
    descriptionKey: 'widget.storage.description',
    requires: 'always',
    defaultSize: 'half',
    essential: false,
  },
  {
    id: 'alerts',
    titleKey: 'widget.alerts.title',
    descriptionKey: 'widget.alerts.description',
    requires: 'always',
    defaultSize: 'full',
    essential: false,
  },
];

export const widgetById: ReadonlyMap<WidgetId, WidgetDefinition> = new Map(
  widgetCatalogue.map((widget) => [widget.id, widget]),
);

export interface WidgetPlacement {
  readonly id: WidgetId;
  readonly size: WidgetSize;
}

export type DashboardLayout = readonly WidgetPlacement[];

/**
 * What a first run looks like.
 *
 * The four resources that answer "why is my computer slow", in the order a
 * person actually checks them, then the two process lists that say which
 * program is responsible. Everything else is opt-in: a dashboard that shows
 * twelve panels by default is a wall of numbers nobody reads.
 */
export const defaultLayout: DashboardLayout = [
  { id: 'cpu', size: 'half' },
  { id: 'memory', size: 'half' },
  { id: 'disk', size: 'half' },
  { id: 'network', size: 'half' },
  { id: 'topCpu', size: 'half' },
  { id: 'topMemory', size: 'half' },
];

/** What the machine can actually report, derived from a live frame. */
export interface WidgetCapabilities {
  readonly hasGpu: boolean;
  readonly hasBattery: boolean;
  readonly hasThermal: boolean;
}

export const NO_CAPABILITIES: WidgetCapabilities = {
  hasGpu: false,
  hasBattery: false,
  hasThermal: false,
};

/**
 * Reads capabilities from a frame.
 *
 * Thermal availability is decided by an actual reading rather than by the
 * presence of a sensor field, because the Rust side reports `null` for every
 * temperature it cannot read without a ring-0 driver. Treating "the field
 * exists" as "the machine has thermals" would offer a Thermals widget that is
 * permanently empty on most desktops.
 */
export function capabilitiesFrom(system: SystemMetrics | null): WidgetCapabilities {
  if (system === null) return NO_CAPABILITIES;

  const hasThermal =
    system.cpu.temperature !== null || system.gpus.some((gpu) => gpu.temperature !== null);

  return {
    hasGpu: system.gpus.length > 0,
    hasBattery: system.battery !== null,
    hasThermal,
  };
}

export function isAvailable(widget: WidgetDefinition, capabilities: WidgetCapabilities): boolean {
  switch (widget.requires) {
    case 'always':
      return true;
    case 'gpu':
      return capabilities.hasGpu;
    case 'battery':
      return capabilities.hasBattery;
    case 'thermal':
      return capabilities.hasThermal;
    default: {
      const exhaustive: never = widget.requires;
      return exhaustive;
    }
  }
}

/**
 * Repairs a stored layout against the current build and machine.
 *
 * Total by design, for the same reason `parseSettings` is: the layout lives in
 * a hand-editable JSON file that outlives the version that wrote it. A widget
 * removed in an upgrade, a duplicate introduced by a bad merge, or a GPU
 * widget restored onto a machine whose card was pulled must all degrade to a
 * usable dashboard rather than an exception during the first paint.
 *
 * Unavailable widgets are *dropped from the render*, not from storage — see
 * {@link visibleLayout}. Deleting them here would mean undocking a laptop
 * permanently forgets its GPU widget.
 */
export function parseLayout(raw: unknown): DashboardLayout {
  if (!Array.isArray(raw)) return defaultLayout;

  const seen = new Set<WidgetId>();
  const layout: WidgetPlacement[] = [];

  for (const entry of raw as readonly unknown[]) {
    if (typeof entry !== 'object' || entry === null) continue;
    const record = entry as Record<string, unknown>;
    const id = record['id'];
    if (!isWidgetId(id) || seen.has(id)) continue;

    seen.add(id);
    const definition = widgetById.get(id);
    layout.push({
      id,
      size: isWidgetSize(record['size']) ? record['size'] : (definition?.defaultSize ?? 'half'),
    });
  }

  // An empty result means the file was present but unusable. Defaults beat a
  // blank dashboard, which reads as a broken app rather than an empty one.
  return layout.length > 0 ? layout : defaultLayout;
}

/** The layout minus anything this machine cannot report. */
export function visibleLayout(
  layout: DashboardLayout,
  capabilities: WidgetCapabilities,
): DashboardLayout {
  return layout.filter((placement) => {
    const definition = widgetById.get(placement.id);
    return definition !== undefined && isAvailable(definition, capabilities);
  });
}

/** Widgets that could be added: supported by the machine, not already placed. */
export function availableToAdd(
  layout: DashboardLayout,
  capabilities: WidgetCapabilities,
): readonly WidgetDefinition[] {
  const placed = new Set(layout.map((placement) => placement.id));
  return widgetCatalogue.filter(
    (widget) => !placed.has(widget.id) && isAvailable(widget, capabilities),
  );
}

export function addWidget(layout: DashboardLayout, id: WidgetId): DashboardLayout {
  if (layout.some((placement) => placement.id === id)) return layout;
  const definition = widgetById.get(id);
  if (definition === undefined) return layout;
  return [...layout, { id, size: definition.defaultSize }];
}

export function removeWidget(layout: DashboardLayout, id: WidgetId): DashboardLayout {
  const definition = widgetById.get(id);
  if (definition?.essential === true) return layout;
  return layout.filter((placement) => placement.id !== id);
}

export function resizeWidget(
  layout: DashboardLayout,
  id: WidgetId,
  size: WidgetSize,
): DashboardLayout {
  return layout.map((placement) => (placement.id === id ? { ...placement, size } : placement));
}

/**
 * Moves a widget by one position.
 *
 * A swap with the neighbour rather than a splice-and-insert: this is driven by
 * keyboard and by two buttons on the widget's menu, and "move up" that jumps
 * two places when the widget above is full-width would be indistinguishable
 * from a bug. Drag-and-drop, if it is ever added, can reuse the same list.
 */
export function moveWidget(
  layout: DashboardLayout,
  id: WidgetId,
  direction: 'up' | 'down',
): DashboardLayout {
  const index = layout.findIndex((placement) => placement.id === id);
  if (index === -1) return layout;

  const target = direction === 'up' ? index - 1 : index + 1;
  if (target < 0 || target >= layout.length) return layout;

  const next = [...layout];
  const moved = next[index];
  const displaced = next[target];
  if (moved === undefined || displaced === undefined) return layout;
  next[index] = displaced;
  next[target] = moved;
  return next;
}
