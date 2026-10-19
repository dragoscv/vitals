import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerStorageStrings } from '../../strings';
import type { PlanItem } from '../model';
import { MoveDiskButton, MoveDiskProvider } from './MoveDiskEntry';

const GB = 1024 ** 3;

beforeAll(async () => {
  await initI18n();
  registerStorageStrings();
});

afterEach(() => {
  cleanup();
});

function item(kind: string, label: string | null): PlanItem {
  return {
    id: 'x',
    kind,
    label,
    size: GB,
    safety: 'review',
    defaultSelected: false,
    irreversible: false,
    needsElevation: true,
    stopsWsl: true,
    stopsDocker: true,
    detail: null,
    available: true,
    unavailableReason: null,
  };
}

function rows(free: number, total: number, items: readonly PlanItem[]) {
  render(
    <MoveDiskProvider mount="C:" freeBytes={free} totalBytes={total} locale="en">
      {items.map((i) => (
        <MoveDiskButton key={`${i.kind}-${i.label ?? ''}`} item={i} />
      ))}
    </MoveDiskProvider>,
  );
}

describe('the move button in the clean-up wizard', () => {
  it('appears on each virtual disk of a drive low on space, named for that disk', () => {
    rows(10 * GB, 500 * GB, [item('compactVhd', 'Ubuntu'), item('compactVhd', null)]);
    expect(screen.getByRole('button', { name: 'Move Ubuntu to another drive' })).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Move docker-desktop to another drive' }),
    ).toBeTruthy();
  });

  it('stays away from a drive with room to spare and from rows that are not disks', () => {
    rows(400 * GB, 500 * GB, [item('compactVhd', 'Ubuntu')]);
    expect(screen.queryByRole('button')).toBeNull();
    cleanup();
    rows(10 * GB, 500 * GB, [item('tempOld', null), item('sparseVhd', 'Ubuntu')]);
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('renders nothing outside the wizard', () => {
    render(<MoveDiskButton item={item('compactVhd', 'Ubuntu')} />);
    expect(screen.queryByRole('button')).toBeNull();
  });
});
