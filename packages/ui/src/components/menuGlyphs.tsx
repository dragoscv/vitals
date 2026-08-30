/**
 * Inline glyphs for menu and control indicators.
 *
 * Not `lucide-react` imports: these appear inside state indicators that must
 * render even if an icon set fails to load, and drawing three paths locally
 * costs less than the icons themselves. All are `aria-hidden` — the state they
 * illustrate is already exposed through ARIA on the parent, and announcing
 * "checkmark" alongside "checked" is a duplicate.
 */

export function CheckGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <path
        d="M3.5 8.5l3 3 6-7"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function DotGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className={className ?? 'size-3.5'}>
      <circle cx="8" cy="8" r="3" fill="currentColor" />
    </svg>
  );
}

export function ChevronRightGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <path
        d="M6 3.5L10.5 8 6 12.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function ChevronDownGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <path
        d="M3.5 6L8 10.5 12.5 6"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function MinusGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <path d="M4 8h8" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
    </svg>
  );
}

export function SearchGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <circle cx="7" cy="7" r="4.25" stroke="currentColor" strokeWidth="1.5" />
      <path
        d="M10.2 10.2L13.5 13.5"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function CloseGlyph({ className }: { readonly className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="none" aria-hidden="true" className={className ?? 'size-3.5'}>
      <path d="M4 4l8 8m0-8l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}
