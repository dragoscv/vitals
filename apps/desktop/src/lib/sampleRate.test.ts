import { describe, expect, it } from 'vitest';

import { effectiveRate } from './sampleRate';
import { samplingRates } from '../settings/schema';

const visible = true;
const hidden = false;

describe('effectiveRate', () => {
  it('maps every setting the UI offers', () => {
    // A rate the UI can produce but this map does not cover would resolve to
    // `undefined` and be sent to Rust as a null, which fails deserialisation
    // silently in a background call nobody is watching.
    for (const rate of samplingRates) {
      expect(
        effectiveRate({ samplingRate: rate, throttleWhenHidden: true }, visible),
        `no visible mapping for ${rate}`,
      ).toBeTruthy();

      expect(
        effectiveRate({ samplingRate: rate, throttleWhenHidden: true }, hidden),
        `no hidden mapping for ${rate}`,
      ).toBeTruthy();
    }
  });

  it('uses the chosen rate while the window is visible', () => {
    expect(effectiveRate({ samplingRate: 'fast', throttleWhenHidden: true }, visible)).toBe('high');
    expect(effectiveRate({ samplingRate: 'normal', throttleWhenHidden: true }, visible)).toBe(
      'normal',
    );
    expect(effectiveRate({ samplingRate: 'slow', throttleWhenHidden: true }, visible)).toBe('low');
  });

  it('ignores visibility entirely when throttling is off', () => {
    // The switch exists so someone watching a long-running job in the
    // background can keep the data coming. Throttling anyway would make the
    // setting a lie.
    for (const rate of samplingRates) {
      expect(effectiveRate({ samplingRate: rate, throttleWhenHidden: false }, hidden)).toBe(
        effectiveRate({ samplingRate: rate, throttleWhenHidden: false }, visible),
      );
    }
  });

  it('never samples faster when hidden than when visible', () => {
    // The property that actually matters, and the one a careless edit to the
    // tables would break. Ordered fastest to slowest.
    const order = ['realtime', 'high', 'normal', 'low', 'background', 'paused'];

    for (const rate of samplingRates) {
      const shown = order.indexOf(
        effectiveRate({ samplingRate: rate, throttleWhenHidden: true }, visible),
      );
      const away = order.indexOf(
        effectiveRate({ samplingRate: rate, throttleWhenHidden: true }, hidden),
      );

      expect(away, `${rate} samples faster hidden than visible`).toBeGreaterThanOrEqual(shown);
    }
  });

  it('does not pick up the pace for someone who asked for relaxed', () => {
    // A flat throttle target would push "relaxed" from low to a *faster*
    // background rate on some orderings. Hidden must stay at or below the
    // visible choice, which the previous test asserts generally and this one
    // pins for the case that motivated separate tables.
    expect(effectiveRate({ samplingRate: 'slow', throttleWhenHidden: true }, hidden)).toBe(
      'background',
    );
  });
});
