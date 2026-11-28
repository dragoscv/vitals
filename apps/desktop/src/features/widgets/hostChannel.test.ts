/**
 * The channel a widget window asks the main window over.
 *
 * Two webviews agree on four event names and nothing checks that agreement at
 * compile time, so a rename on one side would silently leave every remote
 * widget empty and every widget on the app's default accent. The names are
 * pinned here, and so is the property that makes the channel safe to call
 * from a browser preview: with no host it does nothing rather than throwing
 * at its caller.
 */

import { describe, expect, it } from 'vitest';

import {
  ACCENT_EVENT,
  ACCENT_REQUEST_EVENT,
  MACHINES_EVENT,
  MACHINES_REQUEST_EVENT,
  onAccent,
  onAccentRequest,
  onMachines,
  publishAccent,
  publishMachines,
  requestAccent,
  requestMachines,
} from './hostChannel';

describe('the widget host channel', () => {
  it('names the events both windows have to agree on', () => {
    expect(MACHINES_EVENT).toBe('vitals://widget-machines');
    expect(MACHINES_REQUEST_EVENT).toBe('vitals://widget-machines-request');
    expect(ACCENT_EVENT).toBe('vitals://widget-accent');
    expect(ACCENT_REQUEST_EVENT).toBe('vitals://widget-accent-request');
  });

  it('stays silent where there is no host instead of failing its caller', () => {
    // There is no Tauri event channel in a browser preview. A widget that threw
    // here would land in its error boundary and show an error instead of this
    // machine's own readings, which it can still show.
    expect(() => publishMachines([])).not.toThrow();
    expect(() => requestMachines()).not.toThrow();
    expect(() => publishAccent('green')).not.toThrow();
    expect(() => requestAccent()).not.toThrow();

    const disposers = [
      onMachines(() => undefined),
      onAccent(() => undefined),
      onAccentRequest(() => undefined),
    ];
    for (const dispose of disposers) expect(() => dispose()).not.toThrow();
  });
});
