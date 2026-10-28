/**
 * Health page and Health settings translations. English and Romanian side by
 * side so a key cannot be added to one and forgotten in the other.
 */

import { i18n } from '@vitals/i18n';

export const HEALTH_NS = 'health';

const en = {
  title: 'Health',
  notAdvice: 'Readings from your phone and scale, via vmui. Not medical advice.',
  updated: 'Last updated {{time}}',
  stale: 'Out of date',
  estimate: 'estimate',
  unavailable: 'Not available',
  loading: 'Reading your health data…',

  card: {
    activity: 'Activity',
    heart: 'Heart',
    sleep: 'Sleep',
    body: 'Body',
    nutrition: 'Food and water',
  },

  metric: {
    steps: 'Steps today',
    distance: 'Distance',
    floors: 'Floors climbed',
    activeKcal: 'Active calories',
    totalKcal: 'Total calories',
    heartRate: 'Heart rate',
    restingHr: 'Resting heart rate',
    hrv: 'Heart rate variability',
    spo2: 'Blood oxygen',
    respiratoryRate: 'Breathing rate',
    bloodPressure: 'Blood pressure',
    hrHistory: 'Heart rate over the last 24 hours',
    sleep: 'Sleep last night',
    weight: 'Weight',
    bodyFat: 'Body fat',
    bmi: 'Body mass index',
    bmr: 'Calories at rest',
    vo2max: 'Fitness (VO₂ max)',
    eaten: 'Calories eaten',
    target: 'Daily target',
    remaining: 'Left today',
    protein: 'Protein',
    carbs: 'Carbohydrates',
    fats: 'Fat',
    water: 'Water',
  },

  unit: {
    km: '{{value}} km',
    bpm: '{{value}} bpm',
    ms: '{{value}} ms',
    pct: '{{value}} %',
    perMin: '{{value}} breaths/min',
    mmHg: '{{sys}}/{{dia}} mmHg',
    kcal: '{{value}} kcal',
    kcalPerDay: '{{value}} kcal/day',
    kg: '{{value}} kg',
    vo2: '{{value}} mL/kg/min',
    hoursMinutes: '{{hours}} h {{minutes}} min',
    ofTarget: '{{value}} / {{target}}',
    grams: '{{value}} g',
    ml: '{{value}} ml',
  },

  error: {
    signedOut: {
      title: 'Vitals is no longer paired with vmui',
      body: 'The pairing was removed in vmui. Pair again to see your health here.',
    },
    notOwner: {
      title: 'This vmui account is not the owner',
      body: 'Only the owner of this vmui can see its health data. Pair with the owner’s account.',
    },
    unreachable: {
      title: 'vmui unreachable',
      body: 'Vitals could not reach vmui. It will try again in a minute.',
    },
    notConfigured: {
      title: 'Not connected to vmui',
      body: 'Connect Vitals to vmui in Settings to see your health here.',
    },
  },
  pairAgain: 'Pair again',
  openSettings: 'Open settings',
  retry: 'Try again',

  settings: {
    tab: 'Health',
    title: 'Health (vmui)',
    intro:
      'Show your steps, heart, sleep, weight and food from your own vmui. The connection key is kept in this computer’s secure store; your health data is never saved here.',
    address: 'vmui address',
    addressHint: 'For example https://vmui.example.com',
    deviceName: 'Name for this computer',
    pair: 'Pair with code',
    pairHint: 'You approve this computer in vmui. No password needed.',
    codeTitle: 'Approve this code in vmui',
    codeBody: 'In vmui, open Notifications, then Paired devices, and approve the code below.',
    codeWaiting: 'Waiting for approval…',
    codeRejected: 'The pairing was declined in vmui.',
    codeExpired: 'The code expired. Start again for a new one.',
    cancel: 'Cancel',
    orSignIn: 'Or sign in with your vmui email and password',
    email: 'Email',
    password: 'Password',
    passwordHint: 'Sent once to vmui and never stored.',
    signIn: 'Sign in',
    connected: 'Connected to {{base}}',
    keyPrefix: 'Key starting {{prefix}}…',
    notConnected: 'Not connected',
    disconnect: 'Disconnect',
    disconnectConfirm: 'Disconnect from vmui? The Health page will be hidden until you pair again.',
    disconnectYes: 'Yes, disconnect',
    failure: {
      invalidUrl: 'That does not look like a web address.',
      insecureUrl: 'The address must start with https:// (http:// works only for this computer).',
      unreachable: 'Vitals could not reach vmui at that address.',
      wrongCredentials: 'The email or password is wrong.',
      slowDown: 'Too many attempts. Wait a minute and try again.',
      generic: 'Something went wrong: {{message}}',
    },
  },
};

const ro: typeof en = {
  title: 'Sănătate',
  notAdvice: 'Date de la telefon și cântar, prin vmui. Nu sunt sfaturi medicale.',
  updated: 'Actualizat ultima dată {{time}}',
  stale: 'Neactualizat',
  estimate: 'estimare',
  unavailable: 'Indisponibil',
  loading: 'Se citesc datele de sănătate…',

  card: {
    activity: 'Activitate',
    heart: 'Inimă',
    sleep: 'Somn',
    body: 'Corp',
    nutrition: 'Mâncare și apă',
  },

  metric: {
    steps: 'Pași azi',
    distance: 'Distanță',
    floors: 'Etaje urcate',
    activeKcal: 'Calorii active',
    totalKcal: 'Calorii totale',
    heartRate: 'Puls',
    restingHr: 'Puls în repaus',
    hrv: 'Variabilitatea pulsului',
    spo2: 'Oxigen în sânge',
    respiratoryRate: 'Ritm respirator',
    bloodPressure: 'Tensiune arterială',
    hrHistory: 'Pulsul în ultimele 24 de ore',
    sleep: 'Somn azi-noapte',
    weight: 'Greutate',
    bodyFat: 'Grăsime corporală',
    bmi: 'Indice de masă corporală',
    bmr: 'Calorii în repaus',
    vo2max: 'Condiție fizică (VO₂ max)',
    eaten: 'Calorii mâncate',
    target: 'Țintă zilnică',
    remaining: 'Rămase azi',
    protein: 'Proteine',
    carbs: 'Carbohidrați',
    fats: 'Grăsimi',
    water: 'Apă',
  },

  unit: {
    km: '{{value}} km',
    bpm: '{{value}} bpm',
    ms: '{{value}} ms',
    pct: '{{value}} %',
    perMin: '{{value}} respirații/min',
    mmHg: '{{sys}}/{{dia}} mmHg',
    kcal: '{{value}} kcal',
    kcalPerDay: '{{value}} kcal/zi',
    kg: '{{value}} kg',
    vo2: '{{value}} mL/kg/min',
    hoursMinutes: '{{hours}} h {{minutes}} min',
    ofTarget: '{{value}} / {{target}}',
    grams: '{{value}} g',
    ml: '{{value}} ml',
  },

  error: {
    signedOut: {
      title: 'Vitals nu mai este asociat cu vmui',
      body: 'Asocierea a fost ștearsă în vmui. Asociază din nou ca să-ți vezi sănătatea aici.',
    },
    notOwner: {
      title: 'Acest cont vmui nu este al proprietarului',
      body: 'Doar proprietarul acestui vmui îi poate vedea datele de sănătate. Asociază cu contul proprietarului.',
    },
    unreachable: {
      title: 'vmui nu răspunde',
      body: 'Vitals nu a putut ajunge la vmui. Va încerca din nou peste un minut.',
    },
    notConfigured: {
      title: 'Nu ești conectat la vmui',
      body: 'Conectează Vitals la vmui din Setări ca să-ți vezi sănătatea aici.',
    },
  },
  pairAgain: 'Asociază din nou',
  openSettings: 'Deschide setările',
  retry: 'Încearcă din nou',

  settings: {
    tab: 'Sănătate',
    title: 'Sănătate (vmui)',
    intro:
      'Vezi pașii, inima, somnul, greutatea și mâncarea din propriul vmui. Cheia de conectare stă în spațiul securizat al acestui calculator; datele de sănătate nu sunt salvate aici niciodată.',
    address: 'Adresa vmui',
    addressHint: 'De exemplu https://vmui.example.com',
    deviceName: 'Numele acestui calculator',
    pair: 'Asociază cu un cod',
    pairHint: 'Aprobi acest calculator în vmui. Nu e nevoie de parolă.',
    codeTitle: 'Aprobă acest cod în vmui',
    codeBody:
      'În vmui, deschide Notificări, apoi Dispozitive asociate, și aprobă codul de mai jos.',
    codeWaiting: 'Se așteaptă aprobarea…',
    codeRejected: 'Asocierea a fost refuzată în vmui.',
    codeExpired: 'Codul a expirat. Începe din nou pentru unul nou.',
    cancel: 'Anulează',
    orSignIn: 'Sau conectează-te cu emailul și parola vmui',
    email: 'Email',
    password: 'Parolă',
    passwordHint: 'Trimisă o singură dată la vmui și nesalvată niciodată.',
    signIn: 'Conectează-te',
    connected: 'Conectat la {{base}}',
    keyPrefix: 'Cheie care începe cu {{prefix}}…',
    notConnected: 'Neconectat',
    disconnect: 'Deconectează',
    disconnectConfirm:
      'Te deconectezi de la vmui? Pagina Sănătate va fi ascunsă până asociezi din nou.',
    disconnectYes: 'Da, deconectează',
    failure: {
      invalidUrl: 'Nu pare a fi o adresă web.',
      insecureUrl:
        'Adresa trebuie să înceapă cu https:// (http:// merge doar pentru acest calculator).',
      unreachable: 'Vitals nu a putut ajunge la vmui la acea adresă.',
      wrongCredentials: 'Emailul sau parola sunt greșite.',
      slowDown: 'Prea multe încercări. Așteaptă un minut și încearcă din nou.',
      generic: 'Ceva nu a mers: {{message}}',
    },
  },
};

export const bundles = { en, ro } as const;

export function registerHealthStrings(): void {
  for (const [locale, bundle] of Object.entries(bundles)) {
    i18n.addResourceBundle(locale, HEALTH_NS, bundle, true, false);
  }
}
