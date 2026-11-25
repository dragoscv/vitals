import { beforeEach, describe, expect, it } from 'vitest';

import { useNavVisibility } from '../../shell/navigation';
import { refreshCodwenVisibility } from './visibility';

beforeEach(() => {
  useNavVisibility.setState({ codwen: null });
});

describe('refreshCodwenVisibility', () => {
  it('publishes a stored account as something to see', async () => {
    await refreshCodwenVisibility(async () => ({ signedIn: true }));

    expect(useNavVisibility.getState().codwen).toBe(true);
  });

  it('publishes an account that is not signed in as nothing to see', async () => {
    await refreshCodwenVisibility(async () => ({ signedIn: false }));

    expect(useNavVisibility.getState().codwen).toBe(false);
  });

  it('treats a probe that answered nothing as not signed in', async () => {
    await refreshCodwenVisibility(async () => null);

    expect(useNavVisibility.getState().codwen).toBe(false);
  });

  it('clears a previous yes when the probe fails, so a dead host cannot leave an account behind', async () => {
    useNavVisibility.setState({ codwen: true });

    await refreshCodwenVisibility(async () => {
      throw new Error('the host refused the read');
    });

    expect(useNavVisibility.getState().codwen).toBe(false);
  });
});
