import type { RemoteConnection } from './remote';

/**
 * The two questions a widget window cannot answer for itself.
 *
 * A widget runs in its own webview with the narrowest capability set of any
 * window in the app: it may listen and emit events and nothing else. It
 * therefore cannot read the settings file that holds the accent the user
 * chose, and it cannot see the machine registry the widgets screen keeps in
 * memory. Both live in the main window, so both are asked for over events
 * rather than granted a permission — a widget that could read the settings
 * file could read every other setting in it too.
 *
 * Every function here is safe to call with no host at all: in a browser
 * preview it does nothing rather than throwing at its caller, because the one
 * thing a widget can always do is show this machine's own readings.
 */

const NAMESPACE = 'vitals://widget';

/** The machines the user has configured. */
export const MACHINES_EVENT = `${NAMESPACE}-machines`;
/** A widget asking for the list above, since Tauri replays no event to a late listener. */
export const MACHINES_REQUEST_EVENT = `${NAMESPACE}-machines-request`;
/** The accent the user chose, as an `Accent`. */
export const ACCENT_EVENT = `${NAMESPACE}-accent`;
/** A widget asking for the accent above. */
export const ACCENT_REQUEST_EVENT = `${NAMESPACE}-accent-request`;

type Unlisten = () => void;

/** The event API, or `null` where there is no Tauri host. */
async function eventHost() {
  try {
    return await import('@tauri-apps/api/event');
  } catch {
    return null;
  }
}

/**
 * Subscribes to an event and hands back an unsubscribe that is always safe.
 *
 * The listener arrives asynchronously, so an unsubscribe that happens first
 * has to detach the listener once it exists — otherwise a widget that mounts
 * and unmounts quickly would leave a handler behind for the rest of the
 * window's life.
 */
function subscribe<TPayload>(name: string, handler: (payload: TPayload) => void): Unlisten {
  let live = true;
  let off: Unlisten | null = null;

  void eventHost()
    .then((host) =>
      host === null ? undefined : host.listen<TPayload>(name, (event) => handler(event.payload)),
    )
    .then((unlisten) => {
      if (unlisten === undefined) return;
      if (live) off = unlisten;
      else unlisten();
    })
    .catch(() => undefined);

  return () => {
    live = false;
    off?.();
    off = null;
  };
}

function emit<TPayload>(name: string, payload: TPayload): void {
  void eventHost()
    .then((host) => (host === null ? undefined : host.emit(name, payload)))
    .catch(() => undefined);
}

/** Tells every widget window which machines exist. */
export function publishMachines(connections: readonly RemoteConnection[]): void {
  emit(MACHINES_EVENT, connections);
}

/** Asks the main window for the machine list. */
export function requestMachines(): void {
  emit(MACHINES_REQUEST_EVENT, null);
}

/** Called in a widget window with the machine list, whenever it changes. */
export function onMachines(handler: (connections: readonly RemoteConnection[]) => void): Unlisten {
  return subscribe<readonly RemoteConnection[]>(MACHINES_EVENT, handler);
}

/** Called in the main window when a widget asks for the machine list. */
export function onMachinesRequest(handler: () => void): Unlisten {
  return subscribe<null>(MACHINES_REQUEST_EVENT, handler);
}

/** Tells every widget window which accent the user chose. */
export function publishAccent(accent: string): void {
  emit(ACCENT_EVENT, accent);
}

/** Asks the main window for the accent. */
export function requestAccent(): void {
  emit(ACCENT_REQUEST_EVENT, null);
}

/** Called in a widget window with the accent, whenever the main window answers. */
export function onAccent(handler: (accent: string) => void): Unlisten {
  return subscribe<string>(ACCENT_EVENT, handler);
}

/** Called in the main window when a widget asks for the accent. */
export function onAccentRequest(handler: () => void): Unlisten {
  return subscribe<null>(ACCENT_REQUEST_EVENT, handler);
}
