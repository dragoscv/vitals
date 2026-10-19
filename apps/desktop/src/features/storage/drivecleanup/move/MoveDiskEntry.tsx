/**
 * The clean-up wizard's way into the guided disk move: a provider that knows
 * the drive, and a button on each virtual-disk row of a drive low on space.
 *
 * Kept apart from `MoveDiskDialog` so the wizard only carries this small
 * file; the dialog and its strings load on the first click.
 */

import { HardDriveUpload } from 'lucide-react';
import { Suspense, createContext, lazy, useContext, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button } from '@vitals/ui';

import { STORAGE_NS } from '../../strings';
import type { PlanItem } from '../model';
import { isLowOnSpace } from './model';

const MoveDiskDialog = lazy(async () => ({
  default: (await import('./MoveDiskDialog')).MoveDiskDialog,
}));

interface MoveContextValue {
  readonly mount: string;
  readonly low: boolean;
  readonly open: (focus: string) => void;
}

const MoveContext = createContext<MoveContextValue | null>(null);

export function MoveDiskProvider({
  mount,
  freeBytes,
  totalBytes,
  locale,
  children,
}: {
  readonly mount: string;
  readonly freeBytes: number;
  readonly totalBytes: number;
  readonly locale: string;
  readonly children: React.ReactNode;
}): React.JSX.Element {
  const [focus, setFocus] = useState<string | null>(null);
  const value: MoveContextValue = {
    mount,
    low: isLowOnSpace({ freeBytes, totalBytes }),
    open: setFocus,
  };
  return (
    <MoveContext.Provider value={value}>
      {children}
      {focus !== null && (
        <Suspense fallback={null}>
          <MoveDiskDialog
            mount={mount}
            focus={focus}
            locale={locale}
            onClose={() => {
              setFocus(null);
            }}
          />
        </Suspense>
      )}
    </MoveContext.Provider>
  );
}

/** Shown on a virtual-disk row, only when its drive is low on space. */
export function MoveDiskButton({ item }: { readonly item: PlanItem }): React.JSX.Element | null {
  const context = useContext(MoveContext);
  const { t } = useTranslation(STORAGE_NS);
  if (context === null || !context.low || item.kind !== 'compactVhd') return null;
  const name = item.label ?? 'docker-desktop';
  return (
    <Button
      variant="secondary"
      size="sm"
      aria-label={t('drive.moveDiskLabel', { name })}
      onClick={() => {
        context.open(name);
      }}
    >
      <HardDriveUpload aria-hidden className="size-4" />
      {t('drive.moveDisk')}
    </Button>
  );
}
