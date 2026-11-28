/**
 * The widget configuration surface.
 *
 * A list rather than a canvas: the position and the size of a widget are owned
 * by its window (the user drags the thing itself), so the only honest controls
 * here are the ones the window cannot offer — which readings, how big, which
 * style, how transparent, and the two flags that decide how the window behaves
 * when the user is not looking at this screen. There is deliberately no x/y
 * field: it would fight the drag, and `sync_widgets` writes the position once
 * at creation for exactly that reason.
 *
 * Live data comes from the shared session source, so the previews cost one
 * sampler subscription for the whole window rather than one per widget — the
 * lesson `features/dashboard` learned the expensive way.
 *
 * Three things on this screen are not settings for a placed widget. The gallery
 * answers "what would this look like?" before the user commits to a panel, so
 * the first widget someone places is one they chose rather than one they then
 * delete. The bring-back button is the escape hatch for a widget whose monitor
 * is gone. The machines panel is where a widget's source comes from, because
 * pointing one at another computer needs an address and a key the user owns.
 */

import { Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  EmptyState,
  SegmentedControl,
  Switch,
  cn,
} from '@vitals/ui';

import {
  createTauriSystemSource,
  useSystemSnapshot,
  type SystemSource,
} from '../dashboard/useSystemSnapshot';
import type { KeyedBackend } from '@/settings/persistence';
import { describe, type WidgetDescription } from './describe';
import { useWidgetInput } from './live';
import { MachinesPanel, useMachines, type MachineEntry } from './MachinesPanel';
import {
  MAX_WIDGETS,
  MIN_OPACITY,
  layoutsFor,
  widgetSizes,
  widgetStyles,
  type WidgetInstance,
} from './model';
import { useRemoteWidgetInput } from './remote';
import { useWidgets, type WidgetController } from './store';
import { tauriWidgetBridge, type ScreenSource, type WidgetBridge } from './windows';
import { WidgetGallery } from './WidgetGallery';
import { WidgetPreview } from './WidgetPreview';

/**
 * The segment value meaning "this computer".
 *
 * A segmented control has no null, and Radix refuses an empty value outright,
 * so the local machine needs a name to sit under. It is deliberately not a
 * shape `newMachineId` can produce, so a stored machine can never be mistaken
 * for the local one.
 */
const THIS_MACHINE = 'this-machine';

export interface WidgetsScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: SystemSource;
  /** Injectable for the same reason; see `windows.ts` for the real one. */
  readonly bridge?: WidgetBridge;
  /**
   * Injectable so a test can hold the read open and observe the screen before
   * the stored list has arrived. Without it every render in a test is already
   * hydrated, and "the controls wait for the stored layout" cannot be checked.
   */
  readonly backend?: KeyedBackend;
  /**
   * Where the monitors are, when a widget has to be brought back onto one.
   * Injectable so the action can be observed without a window host, and read
   * from the real display list when it is not given.
   */
  readonly screens?: ScreenSource;
  /** The machine list's own key of the settings file; injectable for the same reason as `backend`. */
  readonly machinesBackend?: KeyedBackend;
}

export function WidgetsScreen({
  source,
  bridge = tauriWidgetBridge,
  backend,
  screens,
  machinesBackend,
}: WidgetsScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation();

  const [fallbackSource] = useState<SystemSource | null>(() =>
    source === undefined ? createTauriSystemSource() : null,
  );
  const active = source ?? fallbackSource;
  if (active === null) throw new Error('widgets screen has no metrics source');

  const snapshot = useSystemSnapshot(active);
  const input = useWidgetInput(snapshot.system);
  // Spread rather than `backend` directly: `exactOptionalPropertyTypes` will not
  // accept an explicit `undefined` for an optional member.
  const controller = useWidgets({
    bridge,
    ...(backend !== undefined && { backend }),
    ...(screens !== undefined && { screens }),
  });
  const machines = useMachines({
    ...(machinesBackend !== undefined && { backend: machinesBackend }),
  });

  // Which widget the window asked us to open. Held rather than scrolled to on
  // arrival: a widget can be opened from its own gear before this screen has
  // ever rendered, and the list must still end up on the right row.
  const [editing, setEditing] = useState<string | null>(null);
  const handleEdit = useCallback((id: string) => {
    setEditing(id);
  }, []);

  useEffect(() => bridge.onEdit(({ id }) => handleEdit(id)), [bridge, handleEdit]);

  const headers = useRef(new Map<string, HTMLElement>());
  useEffect(() => {
    if (editing === null) return;
    headers.current.get(editing)?.scrollIntoView({ block: 'nearest' });
  }, [editing, controller.widgets]);

  // Two reasons a place button refuses, and they are different facts: the
  // stored layout has not arrived yet (adding now would write over it), or
  // there is no room. The reason is handed to the gallery rather than kept to
  // itself, so a disabled tile says which of the two it is instead of going
  // quiet.
  const atCap = controller.widgets.length >= MAX_WIDGETS;
  const placeDisabled = !controller.hydrated || atCap;
  const placeReason = !controller.hydrated
    ? t('widgets.loading')
    : atCap
      ? t('widgets.limit', { count: MAX_WIDGETS })
      : undefined;

  return (
    <section className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
      <header className="flex flex-col gap-1">
        <h1 className="text-lg font-semibold">{t('widgets.title')}</h1>
        <p className="text-sm text-[var(--color-fg-muted)]">{t('widgets.subtitle')}</p>
      </header>

      <WidgetGallery
        locale={i18n.language}
        input={input}
        onAdd={controller.add}
        disabled={placeDisabled}
        {...(placeReason !== undefined && { disabledReason: placeReason })}
      />

      {/*
        The recovery, offered as well as run. The store brings every widget back
        on its own once the stored layout has been read, which covers a monitor
        that was unplugged between sessions; this covers the case the user can
        see — a widget they know should be there is not, because the display it
        lived on was turned off a moment ago. It calls the same action rather
        than re-implementing the clamp, so "back on screen" cannot mean two
        different places.
      */}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={!controller.hydrated}
          onClick={() => controller.bringAllOnScreen()}
        >
          {t('widgets.bringBack')}
        </Button>
      </div>

      {controller.hydrated && controller.widgets.length === 0 ? (
        <EmptyState
          icon={<Plus aria-hidden className="size-5" />}
          title={t('widgets.empty.title')}
          description={t('widgets.empty.body')}
        />
      ) : (
        <ul className="flex flex-col gap-3">
          {controller.widgets.map((widget) => (
            <li key={widget.id}>
              <div
                ref={(element) => {
                  if (element === null) headers.current.delete(widget.id);
                  else headers.current.set(widget.id, element);
                }}
              >
                <WidgetCard
                  widget={widget}
                  input={input}
                  machines={machines.machines}
                  controller={controller}
                  locale={i18n.language}
                  open={controller.open.includes(widget.id)}
                  editing={editing === widget.id}
                />
              </div>
            </li>
          ))}
        </ul>
      )}

      <MachinesPanel
        machines={machines.machines}
        hydrated={machines.hydrated}
        onAdd={machines.add}
        onRemove={machines.remove}
      />
    </section>
  );
}

function WidgetCard({
  widget,
  input,
  machines,
  controller,
  locale,
  open,
  editing,
}: {
  readonly widget: WidgetInstance;
  readonly input: ReturnType<typeof useWidgetInput>;
  readonly machines: readonly MachineEntry[];
  readonly controller: WidgetController;
  readonly locale: string;
  readonly open: boolean;
  readonly editing: boolean;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);

  // Hook order is the reason this is called even for a local widget: the
  // `null` case is the hook's own "show this machine" branch, which issues
  // nothing and keeps no traces, so the call costs a render and no request.
  const remote = useRemoteWidgetInput(widget.remoteConnectionId);
  // The preview follows the source: numbers from this computer under another
  // computer's name would be the lie this whole feature exists to avoid.
  const previewInput = widget.remoteConnectionId === null ? input : remote.input;
  const source =
    widget.remoteConnectionId === null
      ? null
      : (machines.find((machine) => machine.id === widget.remoteConnectionId) ?? null);

  // Described through the same function the window uses, so the preview cannot
  // show a reading the widget does not have, or miss one it does.
  const description: WidgetDescription = describe(widget.kind, previewInput, locale);

  return (
    <Card
      className={cn(
        'transition-colors',
        // The row the widget asked for is marked, not just scrolled to: focus
        // follows the scroll and the outline is what says where it went.
        editing && 'border-[var(--color-accent)]',
      )}
    >
      <CardHeader>
        <CardTitle>{t(`widgets.kind.${widget.kind}`)}</CardTitle>
        <div className="flex items-center gap-3">
          {/* A widget can refuse to build (a window manager saying no), so the
              row says whether it is actually on screen rather than implying it. */}
          <span className="text-xs text-[var(--color-fg-muted)]">
            {open ? t('widgets.onScreen') : t('widgets.offScreen')}
          </span>
          <span className="font-mono text-xs text-[var(--color-fg-muted)]">
            {t('widgets.position', { x: Math.round(widget.x), y: Math.round(widget.y) })}
          </span>
        </div>
      </CardHeader>

      <CardBody className="flex flex-col gap-4">
        <div className="flex flex-wrap items-start gap-4">
          <div
            // The preview is drawn at a fixed size rather than at the window's:
            // a `large` widget on this screen would be 400 px of a list row.
            className="flex h-[132px] w-[220px] shrink-0 items-stretch"
            aria-label={t('widgets.preview')}
            role="img"
          >
            <WidgetPreview spec={widget} description={description} expanded={expanded} />
          </div>

          <div className="flex min-w-[16rem] flex-1 flex-col gap-3">
            {/*
              Which machine this widget watches. Named for its own widget
              rather than merely "Source", because six controls called "Source"
              on one screen are six controls a screen reader cannot tell apart.
              A machine missing from the list is still offered the local
              machine, so a widget never becomes unpointable by removing a
              machine — the state message below is what says it is gone.
            */}
            <Field label={t('widgets.source.label')}>
              <SegmentedControl
                ariaLabel={t('widgets.sourceFor', { kind: t(`widgets.kind.${widget.kind}`) })}
                size="sm"
                value={widget.remoteConnectionId ?? THIS_MACHINE}
                options={[
                  { value: THIS_MACHINE, label: t('widgets.source.thisMachine') },
                  ...machines.map((machine) => ({ value: machine.id, label: machine.label })),
                ]}
                onValueChange={(next) =>
                  controller.update(widget.id, {
                    remoteConnectionId: next === THIS_MACHINE ? null : next,
                  })
                }
              />
            </Field>

            {remote.status !== 'idle' && remote.status !== 'live' && (
              // Said in words, not with an icon: "could not reach the machine"
              // and "the machine refused this key" ask the user for opposite
              // things, and a glyph cannot carry which one happened.
              <p className="text-2xs text-[var(--color-danger-text)]" role="status">
                {t(`widgets.status.${remote.status}`, {
                  // Falls back to the stored id when the machine has been
                  // removed from the list — an unnamed failure is still a
                  // failure, and naming the id is the only true name left.
                  name: source?.label ?? widget.remoteConnectionId ?? '',
                })}
              </p>
            )}

            <Field label={t('widgets.size.label')}>
              <SegmentedControl
                ariaLabel={t('widgets.size.label')}
                size="sm"
                value={widget.size}
                onValueChange={(size) => controller.update(widget.id, { size })}
                options={widgetSizes.map((size) => ({
                  value: size,
                  label: t(`widgets.size.${size}`),
                }))}
              />
            </Field>

            <Field label={t('widgets.style.label')}>
              <SegmentedControl
                ariaLabel={t('widgets.style.label')}
                size="sm"
                value={widget.style}
                onValueChange={(style) => controller.update(widget.id, { style })}
                options={widgetStyles.map((style) => ({
                  value: style,
                  label: t(`widgets.style.${style}`),
                }))}
              />
            </Field>

            <Field label={t('widgets.layout.label')}>
              <SegmentedControl
                ariaLabel={t('widgets.layout.label')}
                size="sm"
                value={widget.layout}
                // Only the layouts this kind can actually use. Offering one that
                // has no meaning for the readings would produce a control that
                // silently does nothing.
                options={layoutsFor(widget.kind).map((layout) => ({
                  value: layout,
                  label: t(`widgets.layout.${layout}`),
                }))}
                onValueChange={(layout) => controller.update(widget.id, { layout })}
              />
            </Field>

            <label className="flex flex-col gap-1 text-sm">
              <span className="text-[var(--color-fg-muted)]">
                {t('widgets.opacity', { percent: Math.round(widget.opacity * 100) })}
              </span>
              <input
                type="range"
                min={Math.round(MIN_OPACITY * 100)}
                max={100}
                step={5}
                value={Math.round(widget.opacity * 100)}
                onChange={(event) =>
                  controller.update(widget.id, { opacity: Number(event.target.value) / 100 })
                }
                className="w-full accent-[var(--color-accent)]"
              />
            </label>

            <Switch
              label={t('widgets.alwaysOnTop')}
              description={t('widgets.alwaysOnTopHint')}
              checked={widget.alwaysOnTop}
              onCheckedChange={(checked) => controller.update(widget.id, { alwaysOnTop: checked })}
            />
            <Switch
              label={t('widgets.clickThrough')}
              // Said here because a click-through window cannot receive the
              // click that would turn it back off: this screen is the way out.
              description={t('widgets.clickThroughHint')}
              checked={widget.clickThrough}
              onCheckedChange={(checked) => controller.update(widget.id, { clickThrough: checked })}
            />

            <div className="flex flex-wrap items-center gap-2">
              <Button variant="ghost" size="sm" onClick={() => setExpanded((current) => !current)}>
                {expanded ? t('widgets.collapse') : t('widgets.expand')}
              </Button>
              <Button
                variant="danger"
                size="sm"
                aria-label={t('widgets.remove', { kind: t(`widgets.kind.${widget.kind}`) })}
                onClick={() => controller.remove(widget.id)}
              >
                <Trash2 aria-hidden className="size-3" />
                {t('widgets.removeLabel')}
              </Button>
            </div>
          </div>
        </div>
      </CardBody>
    </Card>
  );
}

function Field({
  label,
  children,
}: {
  readonly label: string;
  readonly children: React.ReactNode;
}): React.JSX.Element {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-[var(--color-fg-muted)]">{label}</span>
      {children}
    </div>
  );
}
