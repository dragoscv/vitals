/**
 * App-wide state: where the user is, which PCs are paired, and the live
 * connection to each. No router library — a TV app has one window and one
 * Back key, so a stack of routes is the whole navigation model, as in the
 * Google TV app's `MainActivity`.
 */

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  useSyncExternalStore,
} from 'react';

import { INITIAL, acquire, connectionFor, type LiveConnection, type LiveState } from '../lib/live';
import type { Pairing, PairingStore } from '../lib/pairing';

export type Route =
  | { readonly name: 'overview' }
  | { readonly name: 'tv' }
  | { readonly name: 'pcs' }
  | { readonly name: 'add' }
  | { readonly name: 'settings' }
  | { readonly name: 'pc'; readonly id: string };

export type TopRoute = Exclude<Route, { name: 'pc' }>['name'];

export interface Navigator {
  readonly current: Route;
  readonly depth: number;
  push(route: Route): void;
  /** A drawer destination replaces the stack: Back from any top level leaves the app. */
  top(name: TopRoute): void;
  /** `false` when already at the top level. */
  pop(): boolean;
}

export interface AppServices {
  readonly nav: Navigator;
  readonly pairings: PairingStore;
}

export const AppContext = createContext<AppServices | null>(null);

export function useApp(): AppServices {
  const value = useContext(AppContext);
  if (value === null) throw new Error('useApp outside AppContext');
  return value;
}

export function useNavigatorState(): Navigator {
  const [stack, setStack] = useState<readonly Route[]>([{ name: 'overview' }]);
  const current = stack[stack.length - 1] ?? { name: 'overview' };
  return {
    current,
    depth: stack.length,
    push: (route) => setStack((s) => [...s, route]),
    top: (name) => setStack([{ name }]),
    pop: () => {
      if (stack.length <= 1) return false;
      setStack((s) => s.slice(0, -1));
      return true;
    },
  };
}

export function usePairings(): readonly Pairing[] {
  const { pairings } = useApp();
  return useSyncExternalStore(pairings.subscribe, pairings.get);
}

/** Subscribes to a PC's shared live connection for as long as the caller is mounted. */
export function useLive(pairing: Pairing | undefined): {
  state: LiveState;
  connection: LiveConnection | null;
} {
  const id = pairing?.id;
  const base = pairing?.baseUrl;
  const token = pairing?.token;
  // Keyed on the PC's identity, not the object: a scope update produces a
  // new object for the same PC and must not reconnect.
  const connection = useMemo(
    () => (pairing === undefined ? null : connectionFor(pairing)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [id, base, token],
  );
  // Subscribing is what holds the stream open; React unsubscribes on unmount.
  const subscribe = useCallback(
    (listener: () => void) => {
      if (connection === null) return () => {};
      const held = acquire(connection);
      const off = held.connection.subscribe(listener);
      return () => {
        off();
        held.release();
      };
    },
    [connection],
  );
  const get = useCallback(() => connection?.get() ?? INITIAL, [connection]);
  const state = useSyncExternalStore(subscribe, get);
  return { state, connection };
}
