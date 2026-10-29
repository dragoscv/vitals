//! The Health page: the owner's personal health, read from vmui.
//!
//! vmui is a separate self-hosted app; `docs/api/vmui-health.md` is the
//! contract. Three properties shape this file:
//!
//! - The device token lives only in the OS secret store (Credential Manager
//!   on Windows, the Keychain on macOS). Between "pair" and "approved" it is
//!   held in memory here and never crosses into the webview, which only ever
//!   sees its first eight characters.
//! - Health data is never written to disk and never logged — not the body,
//!   not the token, not the password. Nothing in here calls `tracing` with
//!   any of them, and a change that does is a privacy defect.
//! - A reading vmui could not supply is `None` all the way to the screen,
//!   which renders an em dash. Every field decodes leniently: a null, a
//!   missing key or a value of the wrong type becomes `None` rather than
//!   failing the whole body, because one odd sensor must not blank the page.

use std::collections::HashMap;
use std::time::Duration;

use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

/// Credential Manager target for the token (also deleted by the uninstaller,
/// `windows/hooks.nsh`).
#[cfg(windows)]
const TOKEN_TARGET: &str = "vitals/vmui-health-token";
/// The base URL sits beside the token. It is not secret; keeping it in the
/// same store means one place holds the whole connection and one uninstall
/// step removes it.
#[cfg(windows)]
const BASE_TARGET: &str = "vitals/vmui-health-base";
#[cfg(windows)]
const CREDENTIAL_USER: &str = "vmui";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "app.vitals.health";

/// What the device calls itself in vmui's list of paired devices.
const PLATFORM: &str = "vitals-desktop";

// ── storage ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Slot {
    Token,
    Base,
}

#[cfg(windows)]
impl Slot {
    fn target(self) -> &'static str {
        match self {
            Self::Token => TOKEN_TARGET,
            Self::Base => BASE_TARGET,
        }
    }
}

#[cfg(target_os = "macos")]
impl Slot {
    fn account(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::Base => "base",
        }
    }
}

#[cfg(windows)]
fn store_put(slot: Slot, value: &str) -> Result<(), String> {
    vitals_win::credentials::write(slot.target(), CREDENTIAL_USER, value).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn store_get(slot: Slot) -> Result<Option<String>, String> {
    vitals_win::credentials::read(slot.target()).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn store_delete(slot: Slot) -> Result<(), String> {
    vitals_win::credentials::delete(slot.target()).map_err(|e| e.to_string())
}

/// `errSecItemNotFound`: "nothing stored" is an answer, not a failure.
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
fn store_put(slot: Slot, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        slot.account(),
        value.as_bytes(),
    )
    .map_err(|e| format!("keychain: {}", e.code()))
}

#[cfg(target_os = "macos")]
fn store_get(slot: Slot) -> Result<Option<String>, String> {
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, slot.account()) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
        Err(e) => Err(format!("keychain: {}", e.code())),
    }
}

#[cfg(target_os = "macos")]
fn store_delete(slot: Slot) -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, slot.account()) {
        Err(e) if e.code() != ERR_SEC_ITEM_NOT_FOUND => Err(format!("keychain: {}", e.code())),
        _ => Ok(()),
    }
}

/// No secret store this app trusts on other systems yet, so nothing is kept
/// and the page stays hidden. A token in a plain file would be readable by
/// anything that can read the home directory.
#[cfg(not(any(windows, target_os = "macos")))]
const UNSUPPORTED: &str = "Health is not available on this system yet";

#[cfg(not(any(windows, target_os = "macos")))]
fn store_put(_slot: Slot, _value: &str) -> Result<(), String> {
    Err(UNSUPPORTED.into())
}

// `Result` for the same signature as the real stores; there is nothing to read.
#[cfg(not(any(windows, target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn store_get(_slot: Slot) -> Result<Option<String>, String> {
    Ok(None)
}

#[cfg(not(any(windows, target_os = "macos")))]
#[allow(clippy::unnecessary_wraps)]
fn store_delete(_slot: Slot) -> Result<(), String> {
    Ok(())
}

/// The stored connection, read off the IPC thread.
async fn stored() -> Result<(Option<String>, Option<String>), String> {
    tauri::async_runtime::spawn_blocking(|| Ok((store_get(Slot::Base)?, store_get(Slot::Token)?)))
        .await
        .map_err(|e| e.to_string())?
}

async fn save(base: String, token: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        store_put(Slot::Base, &base)?;
        store_put(Slot::Token, &token)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── validation ─────────────────────────────────────────────────────────────

/// Checks a vmui address and returns it without a trailing slash.
///
/// `https` only: the token is a bearer credential and the body is medical
/// data, and plain HTTP would hand both to anyone on the path. Plain HTTP is
/// allowed for `localhost` and `127.0.0.1` alone, where there is no path.
pub fn validate_base(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "invalidUrl".to_string())?;
    let host = url.host_str().unwrap_or_default();
    if host.is_empty() || !url.username().is_empty() || url.password().is_some() {
        return Err("invalidUrl".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("invalidUrl".into());
    }
    match url.scheme() {
        "https" => {}
        "http" if host == "localhost" || host == "127.0.0.1" => {}
        _ => return Err("insecureUrl".into()),
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// A token goes into a header; anything that could break the header line is
/// refused rather than sent.
fn valid_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 512 && token.chars().all(|c| c.is_ascii_graphic())
}

fn prefix(token: &str) -> String {
    token.chars().take(8).collect()
}

// ── HTTP ───────────────────────────────────────────────────────────────────

/// One pooled client for every vmui call. 3 s to connect and 10 s overall,
/// like the AI page's (agents.rs): a vmui that is off must not hold the page.
fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        // fails only when another thread installed one first, which is fine
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let built = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    Ok(CLIENT.get_or_init(|| built))
}

#[derive(Debug, Serialize)]
struct PairRequest<'a> {
    name: &'a str,
    platform: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct PairResponse {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

async fn post_pair(base: &str, body: &PairRequest<'_>) -> Result<(u16, String), String> {
    let res = client()?
        .post(format!("{base}/api/devices/pair"))
        .json(body)
        .send()
        .await
        .map_err(|_| "unreachable".to_string())?;
    let status = res.status().as_u16();
    let text = res.text().await.map_err(|_| "unreachable".to_string())?;
    Ok((status, text))
}

// ── pairing ────────────────────────────────────────────────────────────────

/// Pairings waiting for the owner's approval: id → (base, token). The token
/// stays here until vmui says "approved"; the webview gets only the id and
/// the code to show.
static PENDING: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None);

fn pending_insert(id: String, base: String, token: String) {
    PENDING
        .lock()
        .get_or_insert_with(HashMap::new)
        .insert(id, (base, token));
}

fn pending_get(id: &str) -> Option<(String, String)> {
    PENDING.lock().as_ref().and_then(|m| m.get(id).cloned())
}

fn pending_remove(id: &str) {
    if let Some(m) = PENDING.lock().as_mut() {
        m.remove(id);
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub configured: bool,
    pub base: Option<String>,
    pub token_prefix: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStart {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PairStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairPoll {
    pub status: PairStatus,
}

/// Maps vmui's answer to a poll. 404 means the attempt is gone; anything
/// else that is not a success — 429 "ask more slowly", a 5xx blip — is still
/// "waiting" for the user, and the next poll asks again.
fn poll_status(http: u16, body: &str) -> PairStatus {
    match http {
        404 => PairStatus::Expired,
        200..=299 => {
            let parsed: Option<PairResponse> = serde_json::from_str(body).ok();
            match parsed.and_then(|p| p.status).as_deref() {
                Some("approved") => PairStatus::Approved,
                Some("rejected") => PairStatus::Rejected,
                Some("expired") => PairStatus::Expired,
                _ => PairStatus::Pending,
            }
        }
        _ => PairStatus::Pending,
    }
}

/// Whether a vmui connection is stored, where to, and the token's first
/// eight characters.
#[tauri::command]
pub async fn health_status() -> Result<HealthStatus, String> {
    let (base, token) = stored().await?;
    Ok(HealthStatus {
        configured: base.is_some() && token.is_some(),
        base,
        token_prefix: token.as_deref().map(prefix),
    })
}

/// Step one of pairing: asks vmui for a code the owner approves there.
#[tauri::command]
pub async fn health_pair_start(base: String, name: String) -> Result<PairStart, String> {
    let base = validate_base(&base)?;
    let name = if name.trim().is_empty() {
        "Vitals".to_string()
    } else {
        name.trim().chars().take(64).collect()
    };
    let (status, body) = post_pair(
        &base,
        &PairRequest {
            name: &name,
            platform: PLATFORM,
            email: None,
            password: None,
        },
    )
    .await?;
    if status == 429 {
        return Err("slowDown".into());
    }
    if !(200..=299).contains(&status) {
        return Err("unreachable".into());
    }
    let parsed: PairResponse =
        serde_json::from_str(&body).map_err(|_| "unreachable".to_string())?;
    let (Some(id), Some(token), Some(code)) = (parsed.id, parsed.token, parsed.code) else {
        return Err("unreachable".into());
    };
    if id.is_empty() || !valid_token(&token) {
        return Err("unreachable".into());
    }
    pending_insert(id.clone(), base, token);
    Ok(PairStart { id, code })
}

/// Step two: asks whether the owner has approved; on approval the token is
/// saved and forgotten from memory.
#[tauri::command]
pub async fn health_pair_poll(id: String) -> Result<PairPoll, String> {
    let Some((base, token)) = pending_get(&id) else {
        return Ok(PairPoll {
            status: PairStatus::Expired,
        });
    };
    let mut url =
        reqwest::Url::parse(&format!("{base}/api/devices/pair")).map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("id", &id);
    let res = client()?
        .get(url)
        .send()
        .await
        .map_err(|_| "unreachable".to_string())?;
    let http = res.status().as_u16();
    let body = res.text().await.unwrap_or_default();
    let status = poll_status(http, &body);
    match status {
        PairStatus::Approved => {
            save(base, token).await?;
            pending_remove(&id);
        }
        PairStatus::Rejected | PairStatus::Expired => pending_remove(&id),
        PairStatus::Pending => {}
    }
    Ok(PairPoll { status })
}

/// The instant alternative: the owner's vmui email and password, sent once
/// and never stored or logged.
#[tauri::command]
pub async fn health_sign_in(
    base: String,
    email: String,
    password: String,
) -> Result<PairPoll, String> {
    let base = validate_base(&base)?;
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Vitals".into());
    let (status, body) = post_pair(
        &base,
        &PairRequest {
            name: &host,
            platform: PLATFORM,
            email: Some(email.trim()),
            password: Some(&password),
        },
    )
    .await?;
    match status {
        401 => return Err("wrongCredentials".into()),
        429 => return Err("slowDown".into()),
        200..=299 => {}
        _ => return Err("unreachable".into()),
    }
    let parsed: PairResponse =
        serde_json::from_str(&body).map_err(|_| "unreachable".to_string())?;
    match (parsed.token, parsed.status.as_deref()) {
        (Some(token), Some("approved")) if valid_token(&token) => {
            save(base, token).await?;
            Ok(PairPoll {
                status: PairStatus::Approved,
            })
        }
        _ => Err("unreachable".into()),
    }
}

/// Forgets the token. The address stays, so pairing again is one click.
#[tauri::command]
pub async fn health_disconnect() -> Result<(), String> {
    if let Some(m) = PENDING.lock().as_mut() {
        m.clear();
    }
    tauri::async_runtime::spawn_blocking(|| store_delete(Slot::Token))
        .await
        .map_err(|e| e.to_string())?
}

// ── reading ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FetchErrorKind {
    SignedOut,
    NotOwner,
    Unreachable,
    NotConfigured,
}

/// What `health_fetch` rejects with. A kind, not a message: the screen picks
/// the words and the action ("Pair again") from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FetchError {
    pub kind: FetchErrorKind,
}

impl From<FetchErrorKind> for FetchError {
    fn from(kind: FetchErrorKind) -> Self {
        Self { kind }
    }
}

/// `None` for a success, otherwise what the status means for the user.
pub fn kind_for_status(status: u16) -> Option<FetchErrorKind> {
    match status {
        200..=299 => None,
        401 => Some(FetchErrorKind::SignedOut),
        403 => Some(FetchErrorKind::NotOwner),
        _ => Some(FetchErrorKind::Unreachable),
    }
}

/// Any value that does not decode as `T` — null, missing, the wrong type —
/// becomes `None` instead of failing the whole body.
fn lenient<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).ok())
}

/// Declares a struct whose every field is `Option<_>`, defaulted, decoded
/// leniently and serialised camelCase. One macro rather than an attribute
/// pair on each of sixty fields, which is where one would be forgotten.
macro_rules! lenient_struct {
    ($(#[$meta:meta])* pub struct $name:ident { $($field:ident: $ty:ty,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $(
                #[serde(default, deserialize_with = "lenient")]
                pub $field: Option<$ty>,
            )*
        }
    };
}

lenient_struct! {
    /// `GET /api/v1/me/health`.
    pub struct MeHealth {
        at: f64,
        health: HealthReadings,
        nutrition: Nutrition,
    }
}

lenient_struct! {
    pub struct HealthReadings {
        device: String,
        steps: f64,
        steps_at: f64,
        distance_km: f64,
        floors: f64,
        active_kcal: f64,
        total_kcal: f64,
        heart_rate: f64,
        heart_rate_at: f64,
        resting_hr: f64,
        hrv: f64,
        spo2: f64,
        respiratory_rate: f64,
        sleep_min: f64,
        sleep_at: f64,
        weight_kg: f64,
        weight_at: f64,
        body_fat: f64,
        body: BodyEstimate,
        bmr: f64,
        vo2max: f64,
        blood_pressure: BloodPressure,
        hr_history: Vec<HrPoint>,
    }
}

lenient_struct! {
    pub struct BodyEstimate {
        bmi: f64,
        fat_pct: f64,
        fat_kg: f64,
        water_kg: f64,
        water_pct: f64,
        lean_kg: f64,
        muscle_kg: f64,
        bmr_kcal: f64,
        bone_kg: f64,
        age_years: f64,
        bmi_band: String,
    }
}

lenient_struct! {
    pub struct BloodPressure {
        sys: f64,
        dia: f64,
    }
}

lenient_struct! {
    pub struct HrPoint {
        t: f64,
        v: f64,
    }
}

lenient_struct! {
    pub struct Nutrition {
        day: String,
        targets: NutritionTargets,
        today: NutritionDay,
        remaining: NutritionRemaining,
        balance: f64,
        active_kcal: f64,
        water: Water,
    }
}

lenient_struct! {
    pub struct NutritionTargets {
        weight_kg: f64,
        weight_source: String,
        age_years: f64,
        bmr: f64,
        tdee: f64,
        calories: f64,
        protein: f64,
        carbs: f64,
        fats: f64,
        fiber: f64,
    }
}

lenient_struct! {
    pub struct NutritionDay {
        day: String,
        calories: f64,
        protein: f64,
        carbs: f64,
        fats: f64,
        fiber: f64,
        meals: f64,
    }
}

lenient_struct! {
    pub struct NutritionRemaining {
        calories: f64,
        protein: f64,
        carbs: f64,
        fats: f64,
    }
}

lenient_struct! {
    pub struct Water {
        day: String,
        ml: f64,
        glasses: f64,
        target_ml: f64,
        remaining_ml: f64,
        last_at: f64,
        under_pace: bool,
    }
}

/// Decodes a 200 body. `None` only when it is not a JSON object at all.
pub fn decode(body: &str) -> Option<MeHealth> {
    serde_json::from_str(body).ok()
}

/// Reads the owner's health from vmui. Kept in memory by the caller only.
#[tauri::command]
pub async fn health_fetch() -> Result<MeHealth, FetchError> {
    let (base, token) = stored()
        .await
        .map_err(|_| FetchError::from(FetchErrorKind::NotConfigured))?;
    let (Some(base), Some(token)) = (base, token) else {
        return Err(FetchErrorKind::NotConfigured.into());
    };
    let unreachable = || FetchError::from(FetchErrorKind::Unreachable);
    let res = client()
        .map_err(|_| unreachable())?
        .get(format!("{base}/api/v1/me/health"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|_| unreachable())?;
    if let Some(kind) = kind_for_status(res.status().as_u16()) {
        return Err(kind.into());
    }
    let body = res.text().await.map_err(|_| unreachable())?;
    decode(&body).ok_or_else(unreachable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_public_host_over_plain_http_is_refused_so_the_token_never_travels_unencrypted() {
        assert_eq!(
            validate_base("http://vmui.example.com").unwrap_err(),
            "insecureUrl"
        );
        assert_eq!(
            validate_base("ftp://vmui.example.com").unwrap_err(),
            "insecureUrl"
        );
        assert_eq!(validate_base("not a url").unwrap_err(), "invalidUrl");
        assert_eq!(
            validate_base("https://user:pw@vmui.example.com").unwrap_err(),
            "invalidUrl"
        );
    }

    #[test]
    fn https_and_local_development_addresses_are_accepted_without_a_trailing_slash() {
        assert_eq!(
            validate_base("https://vmui.example.com/").unwrap(),
            "https://vmui.example.com"
        );
        assert_eq!(
            validate_base(" https://h.example/vmui/ ").unwrap(),
            "https://h.example/vmui"
        );
        assert_eq!(
            validate_base("http://localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert_eq!(
            validate_base("http://127.0.0.1:3000/").unwrap(),
            "http://127.0.0.1:3000"
        );
    }

    #[test]
    fn a_full_body_decodes_every_reading_the_page_shows() {
        let body = r#"{
          "at": 1793200000000,
          "health": {
            "device": "phone", "steps": 8123, "stepsAt": 1793199000000, "distanceKm": 6.1,
            "floors": 4, "activeKcal": 410, "totalKcal": 2310, "heartRate": 72,
            "heartRateAt": 1, "restingHr": 58, "hrv": 41, "spo2": 97, "respiratoryRate": 14,
            "sleepMin": 412, "sleepAt": 1, "weightKg": 81.4, "weightAt": 1, "bodyFat": 18.2,
            "impedance": 500,
            "body": { "bmi": 27.2, "fatPct": 21.0, "bmrKcal": 1780, "bmiBand": "over" },
            "bmr": 1790, "vo2max": 44,
            "bloodPressure": { "sys": 121, "dia": 79 },
            "hrHistory": [ { "t": 1, "v": 71 }, { "t": 2, "v": 73 } ]
          },
          "nutrition": {
            "day": "2026-10-28",
            "targets": { "calories": 2100, "protein": 160, "carbs": 220, "fats": 70 },
            "today": { "calories": 1460, "protein": 98, "carbs": 150, "fats": 50, "meals": 3 },
            "remaining": { "calories": 640, "protein": 62, "carbs": 70, "fats": 20 },
            "balance": -350,
            "water": { "ml": 1500, "targetMl": 2500, "remainingMl": 1000, "underPace": true,
                       "entries": [], "week": [] },
            "week": []
          }
        }"#;
        let me = decode(body).unwrap();
        assert_eq!(me.at, Some(1_793_200_000_000.0));
        let h = me.health.unwrap();
        assert_eq!(h.steps, Some(8123.0));
        assert_eq!(h.distance_km, Some(6.1));
        assert_eq!(h.heart_rate, Some(72.0));
        assert_eq!(h.blood_pressure.unwrap().sys, Some(121.0));
        assert_eq!(h.body.unwrap().bmi, Some(27.2));
        assert_eq!(h.hr_history.unwrap().len(), 2);
        let n = me.nutrition.unwrap();
        assert_eq!(n.targets.unwrap().calories, Some(2100.0));
        assert_eq!(n.remaining.unwrap().calories, Some(640.0));
        assert_eq!(n.water.unwrap().target_ml, Some(2500.0));
    }

    #[test]
    fn a_sparse_body_turns_every_null_into_none_and_ignores_unknown_fields() {
        let body = r#"{
          "at": 1, "somethingNew": { "x": 1 },
          "health": { "steps": null, "heartRate": null, "body": null, "bloodPressure": null,
                      "hrHistory": null, "futureSensor": 3 },
          "nutrition": null
        }"#;
        let me = decode(body).unwrap();
        let h = me.health.unwrap();
        assert_eq!(h, HealthReadings::default());
        assert_eq!(me.nutrition, None);
    }

    #[test]
    fn a_reading_of_the_wrong_type_is_none_rather_than_blanking_the_page() {
        let me = decode(r#"{ "at": 1, "health": { "steps": "lots", "heartRate": 60 } }"#).unwrap();
        let h = me.health.unwrap();
        assert_eq!(h.steps, None);
        assert_eq!(h.heart_rate, Some(60.0));
    }

    #[test]
    fn a_missing_reading_serialises_as_null_never_as_zero() {
        let json = serde_json::to_value(HealthReadings::default()).unwrap();
        assert!(json["steps"].is_null());
        assert!(json["heartRate"].is_null());
    }

    #[test]
    fn a_revoked_token_means_signed_out_and_a_foreign_account_means_not_owner() {
        assert_eq!(kind_for_status(200), None);
        assert_eq!(kind_for_status(401), Some(FetchErrorKind::SignedOut));
        assert_eq!(kind_for_status(403), Some(FetchErrorKind::NotOwner));
        assert_eq!(kind_for_status(500), Some(FetchErrorKind::Unreachable));
        assert_eq!(kind_for_status(404), Some(FetchErrorKind::Unreachable));
    }

    #[test]
    fn the_error_kind_reaches_the_webview_in_camel_case() {
        let json = serde_json::to_string(&FetchError::from(FetchErrorKind::SignedOut)).unwrap();
        assert_eq!(json, r#"{"kind":"signedOut"}"#);
    }

    #[test]
    fn a_poll_maps_approval_rejection_and_a_gone_attempt() {
        assert_eq!(
            poll_status(200, r#"{"status":"approved"}"#),
            PairStatus::Approved
        );
        assert_eq!(
            poll_status(200, r#"{"status":"rejected"}"#),
            PairStatus::Rejected
        );
        assert_eq!(
            poll_status(200, r#"{"status":"pending"}"#),
            PairStatus::Pending
        );
        assert_eq!(poll_status(404, ""), PairStatus::Expired);
        assert_eq!(poll_status(429, ""), PairStatus::Pending);
    }

    #[test]
    fn only_the_first_eight_characters_of_a_token_are_ever_shown() {
        assert_eq!(prefix("vmd_abcdefghijklmnop"), "vmd_abcd");
        assert!(valid_token("vmd_abc-DEF_123"));
        assert!(!valid_token("vmd_abc\r\nX-Evil: 1"));
        assert!(!valid_token(""));
    }
}
