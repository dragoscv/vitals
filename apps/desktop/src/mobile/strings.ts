/**
 * Alert strings for the phone, in both languages, registered at boot.
 *
 * The server sends alerts as i18n keys — `alert.<kind>.title` and
 * `alert.<kind>.<cause>` — so every consumer renders the same alert in the
 * user's language and no language is baked into the wire. The desktop keeps
 * these strings in `features/dashboard/strings.ts`, which the phone must not
 * import (see `boundary.test.ts`). Putting them in `packages/i18n` instead
 * was measured: it pushed the desktop's initial load 1.0 KB over budget for
 * strings the desktop already ships in a lazy chunk. So they live here, in a
 * namespace only the phone loads, and `strings.test.ts` holds en and ro to
 * the same key set the way `scripts/check-drift.ps1` does for the shared
 * locales.
 *
 * Texts are copied verbatim from the desktop so the two surfaces never
 * phrase one alert two ways.
 */

import { i18n } from '@vitals/i18n';

export const MOBILE_ALERTS_NS = 'mobileAlerts';

const en = {
  alert: {
    none: 'Nothing needs your attention.',
    severity: { info: 'Info', warning: 'Warning', critical: 'Critical' },
    cpuSustained: {
      title: 'The processor has been busy for a while',
      cause:
        'CPU has averaged {{percent}}% for over {{seconds}} seconds. Something is running that you may not have started.',
    },
    cpuThrottled: {
      title: 'The processor is running slower than it can',
      thermal: 'It is too hot, so Windows is slowing it down to protect the hardware.',
      powerLimit: 'It has hit its power limit. On a laptop this is usually the charger.',
      currentLimit: 'It has hit a current limit set by the motherboard.',
      voltageDrop: 'The supply voltage dropped below what the current clock needs.',
      powerPolicy: 'A Windows power plan is capping it. You can change this in Performance.',
      unknown: 'The hardware reports throttling but not the reason.',
    },
    memoryPressure: {
      title: 'The machine is running out of memory',
      cause:
        '{{faults}} page faults a second with almost no memory free — Windows is moving pages to disk to keep going, and that is what slows everything down.',
    },
    memoryCommit: {
      title: 'Committed memory is close to the limit',
      cause:
        '{{percent}}% of the commit limit is in use. New allocations may start failing, which applications usually report as an out-of-memory crash.',
    },
    diskSaturated: {
      title: '{{disk}} is busy constantly',
      cause: 'The drive has had work queued the whole time for over a minute.',
    },
    diskLatency: {
      title: '{{disk}} is responding slowly',
      cause:
        'Requests are taking {{ms}} ms on average while the drive is fully busy — long enough that anything reading from it will feel unresponsive.',
    },
    diskSpace: {
      title: '{{disk}} is nearly full',
      cause:
        'Only {{percent}}% free. Below this Windows cannot reliably page, update, or write temporary files.',
    },
    diskHealth: {
      title: '{{disk}} reports that it is failing',
      cause:
        'This is the drive telling you, not a guess by Vitals. Back it up now and plan to replace it.',
    },
    gpuThrottled: {
      title: '{{gpu}} is running slower than it can',
      thermal: 'It is too hot and is clocking itself down.',
      powerLimit: 'It has hit its power limit.',
      currentLimit: 'It has hit a current limit.',
      voltageDrop: 'The supply voltage dropped below what the current clock needs.',
      powerPolicy: 'A power policy is capping it.',
      unknown: 'The hardware reports throttling but not the reason.',
    },
    thermalCpu: {
      title: 'The processor is overheating',
      cause:
        '{{celsius}}°C. At this temperature the chip protects itself by slowing down, and sustained heat shortens its life. Check that the fans and vents are clear.',
    },
    networkErrors: {
      title: '{{adapter}} is dropping packets',
      cause:
        '{{errors}} a second. On a cable this usually means a bad cable or port; on Wi-Fi, a weak signal or a congested channel.',
    },
    batteryLow: { title: 'Battery is low', cause: '{{percent}}% remaining and not charging.' },
    batteryHealth: {
      title: 'The battery has worn down',
      cause:
        'It holds {{percent}}% of its original capacity. This is normal with age, but runtime will keep shrinking.',
    },
  },
};

const ro: typeof en = {
  alert: {
    none: 'Nimic nu necesită atenția ta.',
    severity: { info: 'Informație', warning: 'Avertisment', critical: 'Critic' },
    cpuSustained: {
      title: 'Procesorul este solicitat de ceva timp',
      cause:
        'Procesorul a fost în medie la {{percent}}% timp de peste {{seconds}} secunde. Rulează ceva ce poate nu ai pornit tu.',
    },
    cpuThrottled: {
      title: 'Procesorul rulează mai încet decât poate',
      thermal: 'Este prea cald, așa că Windows îl încetinește ca să protejeze hardware-ul.',
      powerLimit: 'A atins limita de putere. Pe un laptop, de obicei este de la încărcător.',
      currentLimit: 'A atins o limită de curent impusă de placa de bază.',
      voltageDrop: 'Tensiunea de alimentare a scăzut sub necesarul frecvenței curente.',
      powerPolicy: 'Un plan de alimentare Windows îl limitează. Poți schimba asta în Performanță.',
      unknown: 'Hardware-ul raportează limitare, dar nu și motivul.',
    },
    memoryPressure: {
      title: 'Calculatorul rămâne fără memorie',
      cause:
        '{{faults}} erori de pagină pe secundă și aproape deloc memorie liberă — Windows mută pagini pe disc ca să facă față, iar asta încetinește totul.',
    },
    memoryCommit: {
      title: 'Memoria angajată este aproape de limită',
      cause:
        '{{percent}}% din limita de angajare este folosită. Alocările noi pot începe să eșueze, ceea ce aplicațiile raportează de obicei ca lipsă de memorie.',
    },
    diskSaturated: {
      title: '{{disk}} este ocupat continuu',
      cause: 'Unitatea a avut lucru în așteptare tot timpul, de peste un minut.',
    },
    diskLatency: {
      title: '{{disk}} răspunde încet',
      cause:
        'Cererile durează în medie {{ms}} ms cât timp unitatea este complet ocupată — suficient cât orice citește de acolo să pară blocat.',
    },
    diskSpace: {
      title: '{{disk}} este aproape plin',
      cause:
        'Doar {{percent}}% liber. Sub acest prag Windows nu mai poate pagina, actualiza sau scrie fișiere temporare în mod fiabil.',
    },
    diskHealth: {
      title: '{{disk}} raportează că se defectează',
      cause:
        'Îți spune unitatea însăși, nu este o presupunere a Vitals. Fă o copie de siguranță acum și pregătește-te să o înlocuiești.',
    },
    gpuThrottled: {
      title: '{{gpu}} rulează mai încet decât poate',
      thermal: 'Este prea caldă și își reduce singură frecvența.',
      powerLimit: 'A atins limita de putere.',
      currentLimit: 'A atins o limită de curent.',
      voltageDrop: 'Tensiunea de alimentare a scăzut sub necesarul frecvenței curente.',
      powerPolicy: 'O politică de alimentare o limitează.',
      unknown: 'Hardware-ul raportează limitare, dar nu și motivul.',
    },
    thermalCpu: {
      title: 'Procesorul se supraîncălzește',
      cause:
        '{{celsius}}°C. La această temperatură cipul se protejează încetinind, iar căldura susținută îi scurtează viața. Verifică dacă ventilatoarele și fantele sunt libere.',
    },
    networkErrors: {
      title: '{{adapter}} pierde pachete',
      cause:
        '{{errors}} pe secundă. Pe cablu asta înseamnă de obicei un cablu sau un port defect; pe Wi-Fi, semnal slab sau un canal aglomerat.',
    },
    batteryLow: { title: 'Bateria este descărcată', cause: '{{percent}}% rămas și nu se încarcă.' },
    batteryHealth: {
      title: 'Bateria s-a uzat',
      cause:
        'Mai păstrează {{percent}}% din capacitatea inițială. Este normal odată cu vârsta, dar autonomia va scădea în continuare.',
    },
  },
};

/** Both locales, for the parity test. */
export const bundles = { en, ro } as const;

/**
 * Adds the phone's alert strings to i18next.
 *
 * Must run after `initI18n()`: i18next only defines `addResourceBundle`
 * once `init()` has run, and calling it earlier throws before React mounts —
 * the splash-hang the desktop shipped once. `deep=true, overwrite=false`,
 * because a shallow add replaces the whole namespace.
 */
export function registerMobileStrings(): void {
  if (typeof i18n.addResourceBundle !== 'function') {
    throw new Error(
      'registerMobileStrings() called before initI18n(): i18next only defines addResourceBundle after init',
    );
  }
  i18n.addResourceBundle('en', MOBILE_ALERTS_NS, en, true, false);
  i18n.addResourceBundle('ro', MOBILE_ALERTS_NS, ro, true, false);
}
