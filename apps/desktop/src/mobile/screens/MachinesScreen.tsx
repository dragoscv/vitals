import { useTranslation } from 'react-i18next';

import { EmptyState } from '@vitals/ui';

import { MachineCard } from '../components/MachineCard';
import { useLive, type LiveMachine } from '../lib/live';
import type { Pairing } from '../lib/pairing';

export interface MachinesScreenProps {
  readonly pairings: readonly Pairing[];
  readonly machines: ReadonlyMap<string, LiveMachine>;
  readonly onOpen: (id: string) => void;
  /** Set when the URL carried a token the PC would not accept. */
  readonly pairFailed: boolean;
}

/** Every paired PC, one card each. */
export function MachinesScreen({ pairings, machines, onOpen, pairFailed }: MachinesScreenProps) {
  const { t } = useTranslation();

  if (pairings.length === 0) {
    return (
      <EmptyState
        className="flex-1"
        title={pairFailed ? t('mobile.empty.pairFailed') : t('mobile.empty.title')}
        description={pairFailed ? t('mobile.empty.pairFailedBody') : t('mobile.empty.body')}
      />
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-3 pb-4">
      <h1 className="mb-3 text-lg font-semibold">{t('mobile.machines.title')}</h1>
      {pairFailed && (
        <p role="alert" className="mb-3 text-2xs text-[var(--color-status-danger)]">
          {t('mobile.empty.pairFailedBody')}
        </p>
      )}
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 landscape:grid-cols-2">
        {pairings.map((pairing) => (
          <LiveCard
            key={pairing.id}
            pairing={pairing}
            machine={machines.get(pairing.id)}
            onOpen={onOpen}
          />
        ))}
      </div>
    </div>
  );
}

function LiveCard({
  pairing,
  machine,
  onOpen,
}: {
  readonly pairing: Pairing;
  readonly machine: LiveMachine | undefined;
  readonly onOpen: (id: string) => void;
}) {
  const live = useLive(machine);
  return <MachineCard pairing={pairing} live={live} onOpen={onOpen} />;
}
