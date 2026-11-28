/**
 * The other machines a widget may watch.
 *
 * A widget can draw a reading from a computer this one did not start, over the
 * LAN API that machine already serves. What such a widget needs is an address
 * and a bearer token, and both belong to the user: nothing here discovers a
 * machine, because a monitor that scanned the network and began polling
 * whatever it found would be the opposite of the product.
 *
 * So this is the pair — the list of machines and the form that adds one — and
 * the hook that owns them. The hook holds the list rather than the component,
 * because three things have to agree on one list: these rows, the source picker
 * on every widget card, and the registry `useRemoteWidgetInput` reads. Two
 * copies of the list is how a widget ends up watching a machine the user has
 * already removed.
 *
 * A token is a credential. It is typed into a masked field and read back as a
 * short prefix only — enough to tell two keys apart, never enough to use one —
 * and it is written to the same settings file as every other durable choice,
 * on this computer.
 */

import { Plus, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, Card, CardBody, CardHeader, CardTitle, Input } from '@vitals/ui';

import { reportToLog } from '@/lib/logToFile';
import { createKeyedBackend, type KeyedBackend } from '@/settings/persistence';

import { setRemoteConnections, type RemoteConnection } from './remote';
import { onAccentRequest, onMachinesRequest, publishAccent, publishMachines } from './hostChannel';

/** One key of the shared settings file, alongside `widgets` and `app`. */
const MACHINES_KEY = 'widget-machines';
/** The same interval the widget registry uses for its own durable write. */
const WRITE_DEBOUNCE_MS = 250;
/** How much of a token is shown back. */
const TOKEN_PREFIX = 8;

/** A machine, plus what the user called it. */
export interface MachineEntry extends RemoteConnection {
  /**
   * The name shown in the picker and in a failure message.
   *
   * Part of this module's own type rather than of `RemoteConnection`: the data
   * layer only needs an address and a key, and a label is a thing the user
   * reads. Keeping it here means the LAN layer cannot start depending on
   * something cosmetic, which is how a rename becomes a breaking change.
   */
  readonly label: string;
}

/** What the add form collects, before an id is minted for it. */
export interface MachineDraft {
  readonly label: string;
  readonly baseUrl: string;
  readonly token: string;
}

export interface MachineController {
  readonly machines: readonly MachineEntry[];
  /** False until the stored list has been read. */
  readonly hydrated: boolean;
  add: (draft: MachineDraft) => void;
  remove: (id: string) => void;
}

let counter = 0;

/** A stable id for a new machine. Shaped like `newWidgetId`, for the same reason. */
export function newMachineId(now: number = Date.now()): string {
  counter += 1;
  return `m${now.toString(36)}${counter.toString(36)}`;
}

/**
 * A prefix of a token, and never the whole of one.
 *
 * The last character is always dropped, so a token short enough to fit inside
 * the prefix is still not printed in full — the ellipsis is the promise that
 * what is shown cannot be used.
 */
export function tokenPrefix(token: string): string {
  const shown = token.slice(0, Math.min(TOKEN_PREFIX, Math.max(0, token.length - 1)));
  return `${shown}…`;
}

/**
 * The address as stored, or `null` when it is not one.
 *
 * Trailing slashes are left alone: the client strips them when it joins a path,
 * and rewriting what the user typed is how a displayed address stops matching
 * the one that is actually called. Returning the trimmed text verbatim keeps
 * the row honest about where the widget points.
 */
export function parseAddress(value: string): string | null {
  const raw = value.trim();
  if (raw === '') return null;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') return null;
  if (url.hostname === '') return null;
  return raw;
}

/**
 * Reads the stored list, dropping anything that is not a machine.
 *
 * Tolerant because the file is shared and hand-editable: one malformed entry
 * must cost that entry, not the screen. An id is required and must be unique,
 * since the id is what a widget stores and what the registry is keyed by.
 */
export function parseMachines(raw: unknown): MachineEntry[] {
  if (!Array.isArray(raw)) return [];
  const machines: MachineEntry[] = [];
  const seen = new Set<string>();
  for (const entry of raw) {
    if (typeof entry !== 'object' || entry === null) continue;
    const { id, label, baseUrl, token } = entry as Record<string, unknown>;
    if (typeof id !== 'string' || id === '' || seen.has(id)) continue;
    if (typeof baseUrl !== 'string' || baseUrl === '') continue;
    if (typeof token !== 'string') continue;
    seen.add(id);
    machines.push({
      id,
      baseUrl,
      token,
      // An address is a poor name but a truthful one; an unnamed row is worse.
      label: typeof label === 'string' && label !== '' ? label : baseUrl,
    });
  }
  return machines;
}

export interface MachinesOptions {
  readonly backend?: KeyedBackend;
}

/**
 * The machine list, persisted and published to the data layer.
 *
 * Publishing happens on every change rather than at the call site, because the
 * registry is what decides whether a widget polls at all: a machine removed
 * here must stop being called even while a widget still names it.
 */
export function useMachines(options: MachinesOptions = {}): MachineController {
  const { backend = createKeyedBackend(MACHINES_KEY) } = options;

  const [machines, setMachines] = useState<readonly MachineEntry[]>([]);
  const [hydrated, setHydrated] = useState(false);

  // A caller may pass a fresh object literal, so the backend cannot be a
  // dependency. The ref is stable and each effect reads it when it needs it.
  const backendRef = useRef(backend);
  useEffect(() => {
    backendRef.current = backend;
  });

  useEffect(() => {
    let cancelled = false;

    void backendRef.current
      .load()
      .then((raw) => {
        if (!cancelled) setMachines(parseMachines(raw));
      })
      .catch((error: unknown) => {
        // Defaults, never a throw: a corrupt file must not stop the screen from
        // opening, which would turn a cosmetic problem into a broken product.
        reportToLog('warn', error, 'widgets');
      })
      .finally(() => {
        if (!cancelled) setHydrated(true);
      });

    return () => {
      cancelled = true;
    };
  }, [backendRef]);

  useEffect(() => {
    if (!hydrated) return;
    setRemoteConnections(machines);
    // Published as well as registered: the registry above is module state in
    // this webview, and a widget runs in a different one.
    publishMachines(machines);
  }, [hydrated, machines]);

  // A widget window normally starts long after this one published, and Tauri
  // does not replay an event to a listener that was not there yet, so the
  // widget asks and this is the answer.
  useEffect(() => {
    if (!hydrated) return;
    return onMachinesRequest(() => publishMachines(machines));
  }, [hydrated, machines]);

  // The accent is read off the root element rather than from the theme context:
  // this window has already applied the user's choice there, and the DOM is the
  // one place a provider and a browser preview agree on.
  useEffect(
    () =>
      onAccentRequest(() => publishAccent(document.documentElement.dataset['accent'] ?? 'green')),
    [],
  );

  useEffect(() => {
    if (!hydrated) return;

    const timer = setTimeout(() => {
      void backendRef.current.save(machines).catch((error: unknown) => {
        reportToLog('warn', error, 'widgets');
      });
    }, WRITE_DEBOUNCE_MS);

    return () => {
      clearTimeout(timer);
    };
  }, [machines, hydrated, backendRef]);

  const add = useCallback((draft: MachineDraft) => {
    setMachines((current) => [...current, { id: newMachineId(), ...draft }]);
  }, []);

  const remove = useCallback((id: string) => {
    setMachines((current) => current.filter((machine) => machine.id !== id));
  }, []);

  return { machines, hydrated, add, remove };
}

export interface MachinesPanelProps {
  readonly machines: readonly MachineEntry[];
  readonly hydrated: boolean;
  readonly onAdd: (draft: MachineDraft) => void;
  readonly onRemove: (id: string) => void;
}

/** Which field the last refusal was about. */
type Field = 'label' | 'address' | 'token';

export function MachinesPanel({
  machines,
  hydrated,
  onAdd,
  onRemove,
}: MachinesPanelProps): React.JSX.Element {
  const { t } = useTranslation();

  const [label, setLabel] = useState('');
  const [address, setAddress] = useState('');
  const [token, setToken] = useState('');
  const [refused, setRefused] = useState<{ field: Field; message: string } | null>(null);

  const handleSubmit = (event: React.FormEvent): void => {
    event.preventDefault();

    const name = label.trim();
    if (name === '') {
      setRefused({ field: 'label', message: t('widgets.machines.error.name') });
      return;
    }

    const baseUrl = parseAddress(address);
    if (baseUrl === null) {
      setRefused({ field: 'address', message: t('widgets.machines.error.address') });
      return;
    }

    const key = token.trim();
    if (key === '') {
      setRefused({ field: 'token', message: t('widgets.machines.error.token') });
      return;
    }

    setRefused(null);
    onAdd({ label: name, baseUrl, token: key });
    setLabel('');
    setAddress('');
    setToken('');
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('widgets.machines.title')}</CardTitle>
      </CardHeader>

      <CardBody className="flex flex-col gap-4">
        <p className="text-xs text-[var(--color-fg-muted)]">{t('widgets.machines.subtitle')}</p>

        {machines.length === 0 ? (
          <p className="text-xs text-[var(--color-fg-muted)]">{t('widgets.machines.empty')}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {machines.map((machine) => (
              <li
                key={machine.id}
                className="flex flex-wrap items-center justify-between gap-2 rounded-[var(--radius-control)] border border-[var(--color-border-default)] px-2 py-1.5"
              >
                <div className="flex min-w-0 flex-col">
                  <span className="truncate text-sm">{machine.label}</span>
                  <span className="truncate font-mono text-2xs text-[var(--color-fg-muted)]">
                    {machine.baseUrl}
                  </span>
                  {/* The prefix, never the token: enough to say which key this
                      is, and useless to anything that wants to spend it. */}
                  <span className="font-mono text-2xs text-[var(--color-fg-subtle)]">
                    {t('widgets.machines.key', { prefix: tokenPrefix(machine.token) })}
                  </span>
                </div>

                <Button
                  variant="danger"
                  size="sm"
                  aria-label={t('widgets.machines.remove', { name: machine.label })}
                  onClick={() => onRemove(machine.id)}
                >
                  <Trash2 aria-hidden className="size-3" />
                  {t('widgets.removeLabel')}
                </Button>
              </li>
            ))}
          </ul>
        )}

        <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
          <div className="grid gap-3 sm:grid-cols-2">
            <Input
              label={t('widgets.machines.name')}
              value={label}
              onChange={(event) => setLabel(event.target.value)}
              {...(refused?.field === 'label' && { error: refused.message })}
            />
            <Input
              label={t('widgets.machines.address')}
              value={address}
              placeholder="http://192.168.1.20:7331"
              onChange={(event) => setAddress(event.target.value)}
              {...(refused?.field === 'address' && { error: refused.message })}
            />
          </div>

          <Input
            label={t('widgets.machines.token')}
            // Masked: the field is the one place the whole key ever appears, and
            // a shoulder is cheaper than an exploit.
            type="password"
            value={token}
            onChange={(event) => setToken(event.target.value)}
            hint={t('widgets.machines.tokenHint')}
            {...(refused?.field === 'token' && { error: refused.message })}
          />

          <div>
            <Button type="submit" variant="secondary" size="sm" disabled={!hydrated}>
              <Plus aria-hidden className="size-3" />
              {t('widgets.machines.add')}
            </Button>
          </div>
        </form>
      </CardBody>
    </Card>
  );
}
