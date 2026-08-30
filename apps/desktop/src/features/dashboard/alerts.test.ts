import { describe, expect, it } from 'vitest';

import { RingBuffer } from '@vitals/charts';

import {
  evaluateAlerts,
  sortBySeverity,
  sustainedAbove,
  SUSTAIN_SAMPLES,
  THRESHOLDS,
  type Alert,
} from './alerts';
import { HistoryCollector, type MetricHistory } from './history';
import { makeSystem } from './test-fixtures';

/** A history whose CPU series has been held at `value` for `count` samples. */
function historyWithCpu(value: number, count = SUSTAIN_SAMPLES): MetricHistory {
  const collector = new HistoryCollector();
  for (let index = 0; index < count; index += 1) {
    collector.push({
      system: makeSystem({ cpu: { total: value } }),
      processes: new Map(),
      seq: index + 1,
      elapsedMs: 1000,
      timestampMs: (index + 1) * 1000,
    });
  }
  return collector.current;
}

function idleHistory(): MetricHistory {
  return historyWithCpu(5);
}

function ids(alerts: readonly Alert[]): readonly string[] {
  return alerts.map((alert) => alert.id);
}

describe('sustainedAbove', () => {
  it('is false before there is enough history', () => {
    // The rule that matters: an alert two seconds after launch, on the
    // strength of two samples, is a guess dressed as a measurement.
    const buffer = new RingBuffer(50);
    for (let index = 0; index < SUSTAIN_SAMPLES - 1; index += 1) buffer.push(99);

    expect(sustainedAbove(buffer, 90)).toBe(false);
    buffer.push(99);
    expect(sustainedAbove(buffer, 90)).toBe(true);
  });

  it('is false when a single recent sample dipped', () => {
    const buffer = new RingBuffer(50);
    for (let index = 0; index < SUSTAIN_SAMPLES; index += 1) buffer.push(99);
    expect(sustainedAbove(buffer, 90)).toBe(true);

    buffer.push(10);
    expect(sustainedAbove(buffer, 90)).toBe(false);
  });

  it('treats a gap as neither above nor below', () => {
    // A stalled sampler must not be able to raise an alert on data that was
    // never collected — nor to clear one.
    const buffer = new RingBuffer(50);
    for (let index = 0; index < SUSTAIN_SAMPLES; index += 1) buffer.push(99);
    buffer.pushGap();

    expect(sustainedAbove(buffer, 90)).toBe(false);
  });

  it('is exclusive at the threshold', () => {
    const buffer = new RingBuffer(50);
    for (let index = 0; index < SUSTAIN_SAMPLES; index += 1) buffer.push(90);
    expect(sustainedAbove(buffer, 90)).toBe(false);
  });
});

describe('evaluateAlerts', () => {
  it('says nothing about a healthy machine', () => {
    expect(evaluateAlerts({ system: makeSystem(), history: idleHistory() })).toEqual([]);
  });

  it('says nothing before the first frame', () => {
    expect(evaluateAlerts({ system: null, history: idleHistory() })).toEqual([]);
  });

  it('does not fire on a momentary CPU spike', () => {
    // Every application launch pins the CPU. A dashboard that shouts about it
    // teaches the user to ignore it, and is then useless when it matters.
    const history = historyWithCpu(100, 3);
    expect(ids(evaluateAlerts({ system: makeSystem(), history }))).not.toContain('cpuSustained');
  });

  it('fires once CPU has been pinned long enough', () => {
    const history = historyWithCpu(96);
    expect(ids(evaluateAlerts({ system: makeSystem(), history }))).toContain('cpuSustained');
  });

  describe('memory', () => {
    it('stays quiet at high usage with no faulting', () => {
      // The central claim of this module: 90% RAM full of file cache is a
      // healthy machine, and reporting it as a problem is the mistake every
      // other monitor makes.
      const system = makeSystem({
        memory: { used: 29 * 1024 ** 3, available: 3 * 1024 ** 3, pageFaultsPerSec: 100 },
      });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).not.toContain(
        'memoryPressure',
      );
    });

    it('fires when the machine is actually thrashing', () => {
      const system = makeSystem({
        memory: {
          used: 30 * 1024 ** 3,
          available: 1024 ** 3,
          pageFaultsPerSec: THRESHOLDS.pageFaultsPerSec + 1,
        },
      });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).toContain('memoryPressure');
    });

    it('stays quiet when faulting is high but memory is free', () => {
      // High fault rates are normal during a large file copy. Without the
      // free-memory condition this would fire on a perfectly healthy machine.
      const system = makeSystem({
        memory: { available: 20 * 1024 ** 3, pageFaultsPerSec: 50_000 },
      });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).not.toContain(
        'memoryPressure',
      );
    });

    it('warns when commit approaches its limit', () => {
      const system = makeSystem({
        memory: { committed: 39 * 1024 ** 3, commitLimit: 40 * 1024 ** 3 },
      });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).toContain('memoryCommit');
    });
  });

  describe('disk', () => {
    it('warns when nearly full', () => {
      const system = makeSystem({ disks: [{ total: 1024 ** 3 * 500, free: 1024 ** 3 * 10 }] });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).toContain('diskSpace');
    });

    it('escalates to critical below one percent', () => {
      const system = makeSystem({ disks: [{ total: 1024 ** 3 * 500, free: 1024 ** 2 * 100 }] });
      const alert = evaluateAlerts({ system, history: idleHistory() }).find(
        (candidate) => candidate.id === 'diskSpace',
      );
      expect(alert?.severity).toBe('critical');
    });

    it('reports the drive failing itself, not our guess', () => {
      const system = makeSystem({
        disks: [
          {
            health: {
              failing: true,
              lifeRemaining: null,
              powerOnHours: null,
              totalWritten: null,
              reallocatedSectors: null,
            },
          },
        ],
      });
      const alert = evaluateAlerts({ system, history: idleHistory() }).find(
        (candidate) => candidate.id === 'diskHealth',
      );
      expect(alert?.severity).toBe('critical');
    });

    it('separates a slow disk from a merely busy one', () => {
      // Different causes, different fixes: saturation is a workload problem,
      // latency without saturation is hardware or driver.
      const collector = new HistoryCollector();
      for (let index = 0; index < SUSTAIN_SAMPLES; index += 1) {
        collector.push({
          system: makeSystem({ disks: [{ activeTime: 99, responseMs: 90 }] }),
          processes: new Map(),
          seq: index + 1,
          elapsedMs: 1000,
          timestampMs: (index + 1) * 1000,
        });
      }

      const system = makeSystem({ disks: [{ activeTime: 99, responseMs: 90 }] });
      expect(ids(evaluateAlerts({ system, history: collector.current }))).toContain('diskLatency');
    });
  });

  describe('hardware', () => {
    it('reports the throttle reason, not just that it happened', () => {
      const system = makeSystem({ cpu: { throttled: 'thermal' } });
      const alert = evaluateAlerts({ system, history: idleHistory() }).find(
        (candidate) => candidate.id === 'cpuThrottled',
      );
      // The reason is the actionable part: a thermal throttle means clean the
      // fans, a power-policy throttle means change a setting.
      expect(alert?.causeKey).toBe('alert.cpuThrottled.thermal');
    });

    it('warns about overheating', () => {
      const system = makeSystem({ cpu: { temperature: 99 } });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).toContain('thermalCpu');
    });

    it('says nothing about temperature it cannot read', () => {
      // Most desktops report null here. Silence is correct; zero would be a lie.
      const system = makeSystem({ cpu: { temperature: null } });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).not.toContain('thermalCpu');
    });

    it('ignores dropped packets on a disconnected adapter', () => {
      const system = makeSystem({ networks: [{ connected: false, errorsPerSec: 500 }] });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).not.toContain(
        'networkErrors',
      );
    });
  });

  describe('battery', () => {
    it('warns when low and unplugged', () => {
      const system = makeSystem({ battery: { charge: 5, charging: false } });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).toContain('batteryLow');
    });

    it('stays quiet while charging', () => {
      const system = makeSystem({ battery: { charge: 5, charging: true } });
      expect(ids(evaluateAlerts({ system, history: idleHistory() }))).not.toContain('batteryLow');
    });

    it('mentions wear as information, not a warning', () => {
      const system = makeSystem({ battery: { health: 45 } });
      const alert = evaluateAlerts({ system, history: idleHistory() }).find(
        (candidate) => candidate.id === 'batteryHealth',
      );
      expect(alert?.severity).toBe('info');
    });
  });
});

describe('sortBySeverity', () => {
  it('puts the most serious first', () => {
    const alerts: Alert[] = [
      { id: 'batteryHealth', severity: 'info', titleKey: 'a', causeKey: 'a', values: {} },
      { id: 'diskHealth', severity: 'critical', titleKey: 'b', causeKey: 'b', values: {} },
      { id: 'cpuSustained', severity: 'warning', titleKey: 'c', causeKey: 'c', values: {} },
    ];
    expect(sortBySeverity(alerts).map((alert) => alert.severity)).toEqual([
      'critical',
      'warning',
      'info',
    ]);
  });

  it('is stable within a severity', () => {
    // Alerts are re-evaluated every second. An unstable sort would swap two
    // warnings on every tick and produce a list that flickers while nothing
    // has actually changed.
    const alerts: Alert[] = [
      { id: 'diskSpace', severity: 'warning', titleKey: 'a', causeKey: 'a', values: {} },
      { id: 'memoryCommit', severity: 'warning', titleKey: 'b', causeKey: 'b', values: {} },
      { id: 'networkErrors', severity: 'warning', titleKey: 'c', causeKey: 'c', values: {} },
    ];

    for (let run = 0; run < 5; run += 1) {
      expect(ids(sortBySeverity(alerts))).toEqual(['diskSpace', 'memoryCommit', 'networkErrors']);
    }
  });
});
