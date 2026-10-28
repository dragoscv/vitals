# vmui health contract (Health page)

The Vitals apps read the owner's personal health from **vmui** (a separate
self-hosted app). This file is the contract every Vitals surface codes against.
Source of truth: vmui `src/lib/health/me.ts` (`MeHealth`).

## Pairing (getting a token)

1. `POST {base}/api/devices/pair` with JSON `{"name": "<device name>", "platform": "vitals-desktop" | "vitals-ios" | "vitals-android"}`.
   Response: `{"id": "<16 hex>", "token": "vmd_...", "code": "1234", "status": "pending"}`.
   Show the 4-digit `code`; the owner approves it in vmui (Home -> Notifications -> Paired devices, or the toast).
2. Poll `GET {base}/api/devices/pair?id=<id>` every 3 s (max 10 min) until `{"status": "approved"}`.
   `{"status": "rejected"}` or `"expired"`/404 ends the attempt. 429 = slow down.
3. Alternative (instant): `POST` with `{"name", "platform", "email", "password"}` -> `{"id", "token", "status": "approved"}`; 401 = wrong credentials.
   The password is sent once and never stored.

The token (`vmd_` + 32 base64url chars) is stored ONLY in the OS secret store
(Windows Credential Manager target `vitals/vmui-health-token`, macOS/iOS
Keychain service `app.vitals.health`, Android Keystore-encrypted file).
The base URL is stored beside it (not secret). Show only the first 8 chars of a token.
`base` must be `https://` (refuse `http://` except `http://localhost`/`127.0.0.1` for development).

## Reading

`GET {base}/api/v1/me/health` with `Authorization: Bearer vmd_...`. Never put the token in the URL.

- 200 -> body below. Keep it in memory only: never write it to disk, logs, history or caches.
- 401 -> token unknown/revoked: show "Pair again", keep the base URL.
- 403 -> device is not the owner's: show "This vmui account is not the owner".
- other / network error -> show "vmui unreachable", keep the last body in memory if any.
  Poll every 60 s while the page is visible; not at all while hidden.

```jsonc
{
  "at": 1793200000000,               // epoch ms, when vmui built the answer
  "health": null | {                 // null: Home Assistant down or no phone sensors
    "device": "phone_slug",
    "steps": 8123 | null,            // count today
    "stepsAt": 1793... | null,       // epoch ms
    "distanceKm": 6.1 | null,
    "floors": 4 | null,
    "activeKcal": 410 | null,
    "totalKcal": 2310 | null,
    "heartRate": 72 | null,          // bpm, latest
    "heartRateAt": 1793... | null,
    "restingHr": 58 | null,          // bpm
    "hrv": 41 | null,                // ms (as published by the phone)
    "spo2": 97 | null,               // %
    "respiratoryRate": 14 | null,    // breaths/min
    "sleepMin": 412 | null,          // minutes last night
    "sleepAt": 1793... | null,
    "weightKg": 81.4 | null,
    "weightAt": 1793... | null,
    "bodyFat": 18.2 | null,          // %
    "impedance": 500 | null,         // ohm (often decorative; do not show)
    "body": null | { "bmi": 27.2, "fatPct": 21.0, "fatKg": 17.1, "waterKg": 46.0, "waterPct": 56.5,
                     "leanKg": 64.3, "muscleKg": 35.0, "bmrKcal": 1780, "boneKg": 3.2, "ageYears": 32,
                     "bmiBand": "sub" | "normal" | "over" | "obese" },   // estimates from height/weight
    "bmr": 1790 | null,              // kcal/day
    "vo2max": 44 | null,             // mL/kg/min
    "bloodPressure": null | { "sys": 121, "dia": 79 },  // mmHg
    "hrHistory": [ { "t": 1793..., "v": 71 } ]          // last 24 h, <= 96 points
  },
  "nutrition": null | {
    "day": "2026-10-28",
    "targets": { "weightKg": 81, "weightSource": "scale" | "fallback", "ageYears": 32, "bmr": 1780,
                 "tdee": 2400, "calories": 2100, "protein": 160, "carbs": 220, "fats": 70, "fiber": 25 },
    "today": { "day": "2026-10-28", "calories": 1460, "protein": 98, "carbs": 150, "fats": 50, "fiber": 18, "meals": 3 },
    "remaining": { "calories": 640, "protein": 62, "carbs": 70, "fats": 20 },   // may be negative
    "balance": -350 | null,          // kcal eaten - (BMR + active)
    "activeKcal": 410 | null,
    "water": { "day": "...", "ml": 1500, "glasses": 6, "targetMl": 2500, "remainingMl": 1000,
               "lastAt": 1793... | null, "underPace": true, "entries": [], "week": [ { "day": "...", "ml": 2100 } ] },
    "week": [ { "day": "...", "calories": 2050, "protein": 150, "carbs": 210, "fats": 68, "fiber": 22, "meals": 4 } ]
  }
}
```

Rules for every surface: a `null` reading is shown as an em dash, never as 0;
unknown fields are ignored (decoders must be lenient); `body` values are labelled
"estimate"; this is not medical advice (one line under the page title).
