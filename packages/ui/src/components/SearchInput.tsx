import { forwardRef, useRef, type KeyboardEvent } from 'react';
import { cn } from '../lib/cn';
import { focusRing } from '../lib/styles';
import { CloseGlyph, SearchGlyph } from './menuGlyphs';
import { Input, type InputProps } from './Input';

export interface SearchInputProps extends Omit<
  InputProps,
  'leadingIcon' | 'trailingSlot' | 'type' | 'value' | 'onChange'
> {
  readonly value: string;
  readonly onValueChange: (value: string) => void;
  /** Accessible name for the clear button — caller supplies the translation. */
  readonly clearLabel: string;
  /**
   * Announced result summary, e.g. "12 of 540 processes".
   *
   * Rendered into a polite live region. Filtering a table is the one case where
   * a live region genuinely earns its cost: the user changes the input, the
   * content changes elsewhere on screen, and without this a screen reader user
   * gets no feedback at all that their query did anything.
   */
  readonly resultsAnnouncement?: string;
}

/**
 * A text field for filtering a list.
 *
 * Escape clears the query rather than being left to bubble. In a table this is
 * the expected desktop behaviour, and stopping propagation only when there is
 * something to clear matters: if Escape were swallowed unconditionally, an
 * empty search box inside a dialog would trap the user, because the key that
 * closes the dialog would silently do nothing.
 */
export const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(function SearchInput(
  { value, onValueChange, clearLabel, resultsAnnouncement, onKeyDown, className, ...rest },
  ref,
) {
  const innerRef = useRef<HTMLInputElement>(null);

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    onKeyDown?.(event);
    if (event.defaultPrevented) return;

    if (event.key === 'Escape' && value !== '') {
      event.preventDefault();
      event.stopPropagation();
      onValueChange('');
    }
  }

  function handleClear() {
    onValueChange('');
    // Focus returns to the field, not to the document. Clicking clear and
    // being dumped at the top of the page would mean tabbing all the way back
    // just to type a new query.
    innerRef.current?.focus();
  }

  return (
    <div className={cn('flex flex-col', className)}>
      <Input
        ref={(node) => {
          innerRef.current = node;
          if (typeof ref === 'function') ref(node);
          else if (ref) ref.current = node;
        }}
        // `type="search"` would add the browser's own clear affordance on top
        // of ours, giving two unlabelled X buttons in the tab order.
        type="text"
        role="searchbox"
        value={value}
        onChange={(event) => onValueChange(event.target.value)}
        onKeyDown={handleKeyDown}
        leadingIcon={<SearchGlyph />}
        trailingSlot={
          value === '' ? undefined : (
            <button
              type="button"
              aria-label={clearLabel}
              onClick={handleClear}
              className={cn(
                'inline-flex size-5 items-center justify-center rounded-[var(--radius-control)] text-[var(--color-fg-subtle)] hover:text-[var(--color-fg-default)]',
                focusRing,
              )}
            >
              <CloseGlyph className="size-3" />
            </button>
          )
        }
        {...rest}
      />
      {resultsAnnouncement !== undefined && (
        <span
          aria-live="polite"
          aria-atomic="true"
          className="absolute size-px overflow-hidden whitespace-nowrap"
          style={{ clip: 'rect(0 0 0 0)', clipPath: 'inset(50%)' }}
        >
          {resultsAnnouncement}
        </span>
      )}
    </div>
  );
});
