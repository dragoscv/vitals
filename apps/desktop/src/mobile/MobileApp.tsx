import { Cpu, ListTree, Settings as SettingsIcon } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { VitalsClient, type ControlRequest } from '@vitals/client';
import { i18n as i18next, type Locale } from '@vitals/i18n';
import { cn } from '@vitals/ui';

import { useLive, useLiveMachines, type LiveMachine, type MobileClient } from './lib/live';
import { claimPairing, readPairings, writePairings, type Pairing } from './lib/pairing';
import { MachinesScreen } from './screens/MachinesScreen';
import { ProcessesScreen } from './screens/ProcessesScreen';
import { SettingsScreen } from './screens/SettingsScreen';

type Tab = 'machines' | 'processes' | 'settings';

/** Where the chosen language is remembered between visits. */
const LOCALE_KEY = 'vitals.mobile.locale.v1';

export interface MobileAppProps {
  /** Replaces the real client; tests inject a fake shaped like `VitalsClient`. */
  readonly makeClient?: (pairing: Pairing) => MobileClient;
  readonly storage?: Storage;
}

function defaultClient(pairing: Pairing): MobileClient {
  return new VitalsClient({ baseUrl: pairing.baseUrl, token: pairing.token });
}

/** The phone app: three tabs, one connection per paired PC. */
export function MobileApp({
  makeClient = defaultClient,
  storage = window.localStorage,
}: MobileAppProps) {
  const { t } = useTranslation();
  const [pairings, setPairings] = useState<readonly Pairing[]>(() => readPairings(storage));
  const [tab, setTab] = useState<Tab>('machines');
  const [selectedId, setSelectedId] = useState<string | undefined>(undefined);
  const [pairFailed, setPairFailed] = useState(false);

  // Claim a pairing from the URL once, on first mount. Deliberately no
  // "cancelled" guard: StrictMode runs the effect twice, and only the first
  // run sees the fragment (it strips it), so ignoring its result would
  // silently swallow a failed pairing. React 19 tolerates a late setState.
  useEffect(() => {
    claimPairing({
      location: window.location,
      history: window.history,
      storage,
      makeProbe: (baseUrl, token) =>
        makeClient({ id: baseUrl, baseUrl, token, name: '', addedAt: 0, readOnly: false }),
      now: () => Date.now(),
    })
      .then((pairing) => {
        if (pairing === undefined) return;
        setPairings(readPairings(storage));
        setSelectedId(pairing.id);
      })
      .catch(() => {
        setPairFailed(true);
      });
  }, [storage, makeClient]);

  const machines = useLiveMachines(pairings, makeClient);

  const update = useCallback(
    (next: readonly Pairing[]) => {
      writePairings(storage, next);
      setPairings(next);
    },
    [storage],
  );

  const selected = pairings.find((p) => p.id === selectedId);
  const selectedMachine = selected === undefined ? undefined : machines.get(selected.id);

  function open(id: string): void {
    setSelectedId(id);
    setTab('processes');
  }

  function changeLocale(locale: Locale): void {
    storage.setItem(LOCALE_KEY, locale);
    void i18next.changeLanguage(locale);
  }

  return (
    <div className="flex h-full min-h-0 flex-col bg-[var(--color-bg-base)] text-[var(--color-fg-default)]">
      <main className="safe-top flex min-h-0 flex-1 flex-col">
        {tab === 'machines' && (
          <MachinesScreen
            pairings={pairings}
            machines={machines}
            onOpen={open}
            pairFailed={pairFailed}
          />
        )}
        {tab === 'processes' && (
          <ProcessesTab
            pairing={selected}
            machine={selectedMachine}
            onReadOnlyLearned={(id) =>
              update(pairings.map((p) => (p.id === id ? { ...p, readOnly: true } : p)))
            }
          />
        )}
        {tab === 'settings' && (
          <SettingsScreen
            pairings={pairings}
            onRename={(id, name) => update(pairings.map((p) => (p.id === id ? { ...p, name } : p)))}
            onRemove={(id) => {
              update(pairings.filter((p) => p.id !== id));
              if (selectedId === id) setSelectedId(undefined);
            }}
            onLocale={changeLocale}
          />
        )}
      </main>

      <nav
        aria-label={t('a11y.mainNavigation')}
        className="safe-bottom shrink-0 border-t border-[var(--color-border-subtle)] bg-[var(--color-bg-subtle)]"
      >
        <ul className="grid grid-cols-3">
          <TabButton
            icon={<Cpu />}
            label={t('mobile.tab.machines')}
            active={tab === 'machines'}
            onClick={() => setTab('machines')}
          />
          <TabButton
            icon={<ListTree />}
            label={t('mobile.tab.processes')}
            active={tab === 'processes'}
            onClick={() => setTab('processes')}
          />
          <TabButton
            icon={<SettingsIcon />}
            label={t('mobile.tab.settings')}
            active={tab === 'settings'}
            onClick={() => setTab('settings')}
          />
        </ul>
      </nav>
    </div>
  );
}

/** Restores the language chosen on a previous visit, or `undefined` for the default. */
export function storedLocale(storage: Storage): Locale | undefined {
  const value = storage.getItem(LOCALE_KEY);
  return value === 'en' || value === 'ro' ? value : undefined;
}

function ProcessesTab({
  pairing,
  machine,
  onReadOnlyLearned,
}: {
  readonly pairing: Pairing | undefined;
  readonly machine: LiveMachine | undefined;
  readonly onReadOnlyLearned: (id: string) => void;
}) {
  const live = useLive(machine);
  const control = useCallback(
    (request: ControlRequest) =>
      machine === undefined
        ? Promise.reject(new Error('no machine selected'))
        : machine.control(request),
    [machine],
  );
  return (
    <ProcessesScreen
      pairing={pairing}
      live={live}
      control={control}
      onReadOnlyLearned={onReadOnlyLearned}
    />
  );
}

function TabButton({
  icon,
  label,
  active,
  onClick,
}: {
  readonly icon: React.ReactNode;
  readonly label: string;
  readonly active: boolean;
  readonly onClick: () => void;
}) {
  return (
    <li>
      <button
        type="button"
        onClick={onClick}
        aria-current={active ? 'page' : undefined}
        className={cn(
          'flex min-h-[56px] w-full flex-col items-center justify-center gap-0.5 text-2xs font-medium [&_svg]:size-5',
          active ? 'text-[var(--color-accent)]' : 'text-[var(--color-fg-muted)]',
        )}
      >
        {icon}
        {label}
      </button>
    </li>
  );
}
