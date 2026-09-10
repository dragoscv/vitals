import { useId, type ReactNode } from 'react';

/**
 * A labelled setting.
 *
 * Exists so that the label/description/control relationship is wired once.
 * The description is joined to the control with `aria-describedby` through a
 * render prop rather than being a sibling paragraph — a hint that is only
 * visually adjacent is invisible to a screen reader user, who then hears
 * "Record history, switch, off" with none of the reason it is off by default.
 */
export function SettingsRow({
  label,
  description,
  children,
}: {
  readonly label: string;
  readonly description?: string;
  readonly children: (ids: {
    readonly labelId: string;
    readonly describedBy?: string;
  }) => ReactNode;
}) {
  const base = useId();
  const labelId = `${base}-label`;
  const descriptionId = `${base}-description`;

  return (
    <div className="flex items-start justify-between gap-4 py-2.5">
      <div className="min-w-0 flex-1">
        <span id={labelId} className="block text-sm text-[var(--color-fg-default)]">
          {label}
        </span>
        {description !== undefined && (
          <p id={descriptionId} className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
            {description}
          </p>
        )}
      </div>
      <div className="shrink-0">
        {children({
          labelId,
          ...(description !== undefined && { describedBy: descriptionId }),
        })}
      </div>
    </div>
  );
}

/** Groups related rows under a heading. */
export function SettingsSection({
  title,
  children,
}: {
  readonly title: string;
  readonly children: ReactNode;
}) {
  return (
    <section className="border-b border-[var(--color-border-subtle)] py-3 first:pt-0 last:border-b-0">
      <h3 className="mb-1 text-2xs font-semibold tracking-wide text-[var(--color-fg-subtle)] uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}
