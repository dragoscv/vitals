import type { HealthApi, HealthReadings, MeHealth } from './api';

const EMPTY_READINGS: HealthReadings = {
  device: null,
  steps: null,
  stepsAt: null,
  distanceKm: null,
  floors: null,
  activeKcal: null,
  totalKcal: null,
  heartRate: null,
  heartRateAt: null,
  restingHr: null,
  hrv: null,
  spo2: null,
  respiratoryRate: null,
  sleepMin: null,
  sleepAt: null,
  weightKg: null,
  weightAt: null,
  bodyFat: null,
  body: null,
  bmr: null,
  vo2max: null,
  bloodPressure: null,
  hrHistory: null,
};

/** What the Rust side serialises when vmui answered with every reading null. */
export const SPARSE: MeHealth = { at: 1_793_200_000_000, health: EMPTY_READINGS, nutrition: null };

export const FULL: MeHealth = {
  at: 1_793_200_000_000,
  health: {
    ...EMPTY_READINGS,
    device: 'phone',
    steps: 8123,
    distanceKm: 6.1,
    floors: 4,
    activeKcal: 410,
    totalKcal: 2310,
    heartRate: 72,
    restingHr: 58,
    hrv: 41,
    spo2: 97,
    respiratoryRate: 14,
    sleepMin: 412,
    weightKg: 81.4,
    bodyFat: 18.2,
    body: {
      bmi: 27.2,
      fatPct: 21,
      fatKg: null,
      waterKg: null,
      waterPct: null,
      leanKg: null,
      muscleKg: null,
      bmrKcal: 1780,
      boneKg: null,
      ageYears: null,
      bmiBand: 'over',
    },
    bmr: 1790,
    vo2max: 44,
    bloodPressure: { sys: 121, dia: 79 },
    hrHistory: [
      { t: 1, v: 70 },
      { t: 2, v: 74 },
      { t: 3, v: 72 },
    ],
  },
  nutrition: {
    day: '2026-10-28',
    targets: { calories: 2100, protein: 160, carbs: 220, fats: 70 },
    today: { calories: 1460, protein: 98, carbs: 150, fats: 50 },
    remaining: { calories: 640, protein: 62, carbs: 70, fats: 20 },
    balance: -350,
    activeKcal: 410,
    water: { ml: 1500, targetMl: 2500, remainingMl: 1000 },
  },
};

export function fakeHealthApi(overrides: Partial<HealthApi> = {}): HealthApi {
  return {
    status: () => Promise.resolve({ configured: false, base: null, tokenPrefix: null }),
    pairStart: () => Promise.resolve({ id: 'abcdef0123456789', code: '4821' }),
    pairPoll: () => Promise.resolve('pending'),
    signIn: () => Promise.resolve('approved'),
    disconnect: () => Promise.resolve(),
    fetch: () => Promise.resolve(FULL),
    ...overrides,
  };
}
