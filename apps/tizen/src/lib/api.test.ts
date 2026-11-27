import { VitalsError } from '@vitals/client';
import { describe, expect, it } from 'vitest';

import { controlFailure } from './api';
import { DASH } from './format';

describe('controlFailure', () => {
  it('learns the pairing is read-only only from a token refusal, not from Windows refusing', () => {
    const readOnly = new VitalsError('forbidden', 'forbidden', {
      status: 403,
      detail: { kind: 'forbidden' },
    });
    const denied = new VitalsError('forbidden', 'access-denied', {
      status: 403,
      detail: { kind: 'access-denied' },
    });
    expect(controlFailure(readOnly)).toEqual({ key: 'control.forbidden', readOnly: true });
    expect(controlFailure(denied)).toEqual({ key: 'control.accessDenied', readOnly: false });
  });

  it('passes the PC’s own reason through for unsupported and internal refusals', () => {
    const unsupported = new VitalsError('http', 'x', {
      status: 422,
      detail: { kind: 'unsupported', message: 'no EcoQoS' },
    });
    expect(controlFailure(unsupported)).toEqual({
      key: 'control.unsupported',
      values: { reason: 'no EcoQoS' },
      readOnly: false,
    });
    expect(controlFailure(new VitalsError('http', 'x', { status: 418 }))).toMatchObject({
      key: 'control.failed',
      values: { status: 418 },
    });
  });

  it('shows the dash, not a zero, when the failure carried no HTTP status', () => {
    // No status means the transport gave up — "That did not work (0)" would
    // report a code the PC never sent.
    const outcome = controlFailure(new VitalsError('http', 'x'));
    expect(outcome.values).toEqual({ status: DASH });
    expect(outcome.values?.status).not.toBe(0);
  });
});
