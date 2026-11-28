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
import {
  MAX_WIDGETS,
  MIN_OPACITY,
  layoutsFor,
  widgetKinds,
  widgetSizes,
  widgetStyles,
  type WidgetInstance,
} from './model';
import { useWidgets, type WidgetController } from './store';
import { tauriWidgetBridge, type WidgetBridge } from './windows';
import { WidgetPreview } from './WidgetPreview';

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
}

export function WidgetsScreen({
  source,
  bridge = tauriWidgetBridge,
  backend,
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
  const controller = useWidgets({ bridge, ...(backend !== undefined && { backend }) });

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

  return (
    <section className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
      <header className="flex flex-col gap-1">
        <h1 className="text-lg font-semibold">{t('widgets.title')}</h1>
        <p className="text-sm text-[var(--color-fg-muted)]">{t('widgets.subtitle')}</p>
      </header>

      <div className="flex flex-wrap items-center gap-2" role="group" aria-label={t('widgets.add')}>
        {widgetKinds.map((kind) => (
          <Button
            key={kind}
            variant="secondary"
            size="sm"
            disabled={!controller.hydrated || controller.widgets.length >= MAX_WIDGETS}
            onClick={() => controller.add(kind)}
          >
            <Plus aria-hidden className="size-3" />
            {t(`widgets.kind.${kind}`)}
          </Button>
        ))}
        {controller.widgets.length >= MAX_WIDGETS && (
          <span className="text-xs text-[var(--color-fg-muted)]">
            {t('widgets.limit', { count: MAX_WIDGETS })}
          </span>
        )}
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
    </section>
  );
}

function WidgetCard({
  widget,
  input,
  controller,
  locale,
  open,
  editing,
}: {
  readonly widget: WidgetInstance;
  readonly input: ReturnType<typeof useWidgetInput>;
  readonly controller: WidgetController;
  readonly locale: string;
  readonly open: boolean;
  readonly editing: boolean;
}): React.JSX.Element {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);

  // Described through the same function the window uses, so the preview cannot
  // show a reading the widget does not have, or miss one it does.
  const description: WidgetDescription = describe(widget.kind, input, locale);

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
