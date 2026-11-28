/**
 * The widget gallery.
 *
 * One tile per kind, each rendering the real panel from the live reading rather
 * than a mock. The settings screen offers content kinds as bare buttons, so the
 * only way to answer "what does a Thermals widget look like?" is to place one
 * and delete it again. The gallery is the answer to that question, and it holds
 * no state: the caller passes the reading and receives the chosen kind.
 *
 * The preview reuses `WidgetPreview` — the same renderer the placed window uses
 * — so a tile cannot show a reading its widget would not have. A second preview
 * component would be a second thing to keep in step, and the second one is
 * always the one that drifts.
 */

import { Plus } from 'lucide-react';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, Card, CardBody, CardHeader, CardTitle } from '@vitals/ui';

import { describe, type WidgetInput } from './describe';
import { defaultLayoutFor, widgetKinds, type WidgetInstance, type WidgetKind } from './model';
import { WidgetPreview } from './WidgetPreview';

export interface WidgetGalleryProps {
  readonly locale: string;
  /** Live input, so a preview shows real numbers rather than a mock. */
  readonly input: WidgetInput;
  /** Called when the user picks a kind; the caller places it. */
  readonly onAdd: (kind: WidgetKind) => void;
  /** True when the placement cap is reached: every tile is disabled, with a reason. */
  readonly disabled?: boolean;
  readonly disabledReason?: string;
}

/**
 * The record a tile previews.
 *
 * The smallest size and the kind's default layout, because that is what the
 * caller actually places — a gallery that previewed a layout the user does not
 * get would be the same lie as a mock, one layer down. The id is synthetic and
 * never leaves this component.
 */
function previewSpec(kind: WidgetKind): WidgetInstance {
  return {
    id: `gallery-${kind}`,
    kind,
    size: 'small',
    style: 'plain',
    layout: defaultLayoutFor(kind),
    opacity: 1,
    alwaysOnTop: true,
    clickThrough: false,
    x: 0,
    y: 0,
    remoteConnectionId: null,
  };
}

export function WidgetGallery({
  locale,
  input,
  onAdd,
  disabled = false,
  disabledReason,
}: WidgetGalleryProps): React.JSX.Element {
  const { t } = useTranslation();
  // One base per gallery, indexed by kind below: a tile's heading and its
  // button have to point at each other, so the ids must be unique per tile and
  // per gallery instance, and `useId` cannot be called inside the map.
  const base = useId();
  // One reason node serves all six buttons, so the text is written once rather
  // than repeated under every tile.
  const reasonId = `${base}-reason`;
  const showReason = disabled && disabledReason !== undefined;

  return (
    <div className="flex flex-col gap-2">
      {showReason && (
        <p id={reasonId} className="text-xs text-[var(--color-fg-muted)]">
          {disabledReason}
        </p>
      )}

      <ul className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {widgetKinds.map((kind) => {
          const name = t(`widgets.kind.${kind}`);
          const titleId = `${base}-${kind}-title`;
          const labelId = `${base}-${kind}-label`;
          return (
            <li key={kind}>
              <Card className="flex h-full flex-col">
                <CardHeader>
                  <CardTitle id={titleId}>{name}</CardTitle>
                </CardHeader>
                <CardBody className="flex flex-1 flex-col gap-3">
                  {/*
                    A fixed window, and deliberately no `role` or label of its
                    own. The preview *is* the widget, which already names itself
                    as a group from its content kind, so a second name here
                    would be the same word twice and a second thing to keep in
                    step.
                  */}
                  <div className="flex h-[132px] w-full items-stretch">
                    <WidgetPreview
                      spec={previewSpec(kind)}
                      description={describe(kind, input, locale)}
                      // Collapsed: the placed window opens compact and grows on
                      // hover, so the tile shows the reading the user meets at
                      // rest rather than one they have to hover to reach.
                      expanded={false}
                    />
                  </div>

                  <Button
                    variant="secondary"
                    size="sm"
                    className="mt-auto"
                    disabled={disabled}
                    onClick={() => onAdd(kind)}
                    // Named by the tile's own heading plus the verb on screen:
                    // six buttons reading only "Add a widget" are six buttons a
                    // screen reader cannot tell apart, and the heading is where
                    // the kind already lives. Both parts are locale text, so
                    // nothing is glued together by hand and no key is invented;
                    // the name still contains the visible words, which is what
                    // "click Add a widget" for speech control needs.
                    aria-labelledby={`${titleId} ${labelId}`}
                    {...(disabled && disabledReason !== undefined
                      ? { title: disabledReason, 'aria-describedby': reasonId }
                      : {})}
                  >
                    <Plus aria-hidden className="size-3" />
                    <span id={labelId}>{t('widgets.add')}</span>
                  </Button>
                </CardBody>
              </Card>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
