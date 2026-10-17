import { act, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { Content } from './Content';

function Screen({ title, loaded = true }: { readonly title: string; readonly loaded?: boolean }) {
  return <div data-route-visible="">{loaded ? <h2>{title}</h2> : <p>Loading</p>}</div>;
}

describe('Content', () => {
  it('leaves focus alone on first load, where it belongs to the start of the document', () => {
    render(
      <Content routeKey="dashboard">
        <Screen title="Dashboard" />
      </Content>,
    );
    expect(document.activeElement).toBe(document.body);
  });

  it('moves focus to the new screen’s heading after navigation, without making it a tab stop', () => {
    const { rerender } = render(
      <Content routeKey="dashboard">
        <Screen title="Dashboard" />
      </Content>,
    );
    rerender(
      <Content routeKey="processes">
        <Screen title="Processes" />
      </Content>,
    );
    const heading = screen.getByRole('heading', { name: 'Processes' });
    expect(document.activeElement).toBe(heading);
    expect(heading.getAttribute('tabindex')).toBe('-1');
  });

  it('waits for a screen that is still loading to render its heading', async () => {
    const { rerender } = render(
      <Content routeKey="dashboard">
        <Screen title="Dashboard" />
      </Content>,
    );
    rerender(
      <Content routeKey="storage">
        <Screen title="Storage" loaded={false} />
      </Content>,
    );
    expect(document.activeElement).toBe(document.body);
    rerender(
      <Content routeKey="storage">
        <Screen title="Storage" />
      </Content>,
    );
    // MutationObserver callbacks are delivered as microtasks.
    await act(async () => {
      await Promise.resolve();
    });
    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Storage' }));
  });
});
