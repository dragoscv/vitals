import { useCallback, useLayoutEffect, useState } from 'react';

/** The measured box of the selected item, in the container's coordinate space. */
export interface SlidingIndicatorGeometry {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
  /** Taken from the container's `aria-orientation`, so the indicator can be a
   *  bottom bar in a row and a left bar in a column. */
  readonly orientation: 'horizontal' | 'vertical';
}

export interface SlidingIndicatorOptions {
  /** Matches every item the indicator may sit under, so their boxes can be watched. */
  readonly items: string;
  /** Matches the selected item. */
  readonly selected: string;
}

export interface SlidingIndicator {
  /** Attach to the container that holds the items. */
  readonly ref: (node: HTMLElement | null) => void;
  /** `null` until a selected item has been measured — render nothing then. */
  readonly geometry: SlidingIndicatorGeometry | null;
}

/**
 * Measures the selected item inside a container so that one indicator element
 * can be *moved* to it, instead of every item drawing a marker that appears and
 * disappears.
 *
 * The awkward part is knowing when to re-measure. The selection is applied as a
 * `data-state` attribute by whichever primitive owns it, and nothing tells this
 * hook that it changed: a `useLayoutEffect` runs when the component renders, and
 * that render is not guaranteed to be the one that followed the attribute change
 * (Radix re-renders the list from its own context, which this hook cannot see).
 * So the DOM is observed directly — a `MutationObserver` for the attribute, a
 * `ResizeObserver` for anything that changes a box, including a web font landing
 * after first paint and resizing every label.
 *
 * Every measurement is guarded. Where there is no layout engine all the boxes
 * are zero, and in a collapsed container they are zero too: the caller gets
 * `null` and draws nothing, rather than an indicator pinned to the origin.
 */
export function useSlidingIndicator({
  items,
  selected,
}: SlidingIndicatorOptions): SlidingIndicator {
  const [node, setNode] = useState<HTMLElement | null>(null);
  const [geometry, setGeometry] = useState<SlidingIndicatorGeometry | null>(null);

  // A stable callback ref. An inline arrow would be a new function on every
  // render, so React would detach and reattach the whole observer chain each
  // time instead of once on mount.
  const ref = useCallback((next: HTMLElement | null) => {
    setNode(next);
  }, []);

  useLayoutEffect(() => {
    if (node === null) return;

    const measure = () => {
      const active = node.querySelector<HTMLElement>(selected);
      if (active === null || active.offsetWidth === 0) {
        setGeometry((previous) => (previous === null ? previous : null));
        return;
      }
      const next: SlidingIndicatorGeometry = {
        left: active.offsetLeft,
        top: active.offsetTop,
        width: active.offsetWidth,
        height: active.offsetHeight,
        orientation: isVertical(node) ? 'vertical' : 'horizontal',
      };
      // Returning the previous object when nothing moved is what stops this
      // from being an infinite loop: the observer fires on every measurement,
      // and a fresh object each time would re-render forever.
      setGeometry((previous) =>
        previous !== null &&
        previous.left === next.left &&
        previous.top === next.top &&
        previous.width === next.width &&
        previous.height === next.height &&
        previous.orientation === next.orientation
          ? previous
          : next,
      );
    };

    measure();

    const mutations = new MutationObserver(measure);
    mutations.observe(node, { subtree: true, attributes: true, attributeFilter: ['data-state'] });

    const resizes = typeof ResizeObserver === 'function' ? new ResizeObserver(measure) : null;
    resizes?.observe(node);
    for (const item of node.querySelectorAll(items)) {
      resizes?.observe(item);
    }

    window.addEventListener('resize', measure);

    return () => {
      mutations.disconnect();
      resizes?.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [node, items, selected]);

  return { ref, geometry };
}

/**
 * Which way the container runs.
 *
 * Radix marks this differently per primitive — `aria-orientation` on a tab
 * list, `data-orientation` on a toggle group — so both are read here rather
 * than making every caller restate what its own DOM already says.
 */
function isVertical(node: HTMLElement): boolean {
  return (
    node.getAttribute('aria-orientation') === 'vertical' ||
    node.getAttribute('data-orientation') === 'vertical'
  );
}
