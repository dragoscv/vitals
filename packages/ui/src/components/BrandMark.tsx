import { useId } from 'react';
import { cn } from '../lib/cn';
import { MARK_COLOUR, MARK_GLYPH, MARK_GRID, MARK_TILE, markVariantFor } from './brandGeometry';

/** What the mark is saying right now; see packages/ui/src/styles/brand.css. */
export type BrandMarkState = 'idle' | 'intro' | 'thinking' | 'success' | 'error';

export interface BrandMarkProps {
  /** Rendered edge in CSS pixels. Picks the optical variant, so 16 is not a shrunken 512. */
  readonly size?: number;
  readonly state?: BrandMarkState;
  /** Without the tile: the line alone, in `currentColor`, for monochrome contexts. */
  readonly bare?: boolean;
  /** Hover lift and press. Off for decorative marks. */
  readonly interactive?: boolean;
  /**
   * Marks this as where the splash mark lands. The name is applied only for
   * the duration of the transition (apps/desktop/src/main.tsx): two elements
   * holding one `view-transition-name` at capture time abort the transition.
   */
  readonly morphTarget?: boolean;
  /** When set, the mark is announced; otherwise it is decorative. */
  readonly label?: string;
  readonly className?: string;
}

/**
 * The Vitals mark, drawn live from the generated brand geometry so the app,
 * the icons and the site can never drift apart. Animation states are CSS only
 * (brand.css) and collapse to the static final frame under reduced motion.
 */
export function BrandMark({
  size = 24,
  state = 'idle',
  bare = false,
  interactive = false,
  morphTarget = false,
  label,
  className,
}: BrandMarkProps) {
  const gradient = `vm-${useId().replace(/:/g, '')}`;
  const variant = MARK_GLYPH[markVariantFor(size)];
  const stroke = bare ? 'currentColor' : '#ffffff';

  return (
    <svg
      viewBox={`0 0 ${MARK_GRID} ${MARK_GRID}`}
      width={size}
      height={size}
      className={cn('vitals-mark shrink-0', className)}
      data-state={state}
      {...(interactive && { 'data-interactive': '' })}
      {...(morphTarget && { 'data-mark-target': '' })}
      {...(label ? { role: 'img', 'aria-label': label } : { 'aria-hidden': true })}
    >
      {!bare && (
        <>
          <defs>
            <linearGradient id={gradient} x1="0.2" y1="0" x2="0.8" y2="1">
              <stop offset="0" stopColor={MARK_COLOUR.top} />
              <stop offset="1" stopColor={MARK_COLOUR.bottom} />
            </linearGradient>
          </defs>
          <rect
            className="vm-tile"
            x={MARK_TILE.x}
            y={MARK_TILE.y}
            width={MARK_TILE.size}
            height={MARK_TILE.size}
            rx={MARK_TILE.radius}
            fill={`url(#${gradient})`}
          />
        </>
      )}
      <g className="vm-glyph">
        <path
          className="vm-line"
          d={variant.d}
          pathLength={1}
          fill="none"
          stroke={stroke}
          strokeWidth={variant.stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          className="vm-trace"
          d={variant.d}
          pathLength={1}
          fill="none"
          stroke={bare ? 'currentColor' : MARK_COLOUR.signal}
          strokeWidth={variant.stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </g>
    </svg>
  );
}
