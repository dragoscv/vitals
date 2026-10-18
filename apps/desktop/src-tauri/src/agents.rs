//! The AI page's data: agentq state and AI client sessions (ADR-0039).
//!
//! # Only while someone looks
//!
//! The reader thread starts on the first `subscribe_agents` and parks when
//! the last subscriber leaves (`unsubscribe_agents`, or a subscription that
//! stopped renewing — a webview that crashed cannot unsubscribe). A machine
//! where nobody opens the page pays nothing, as AGENTS.md requires.
//!
//! # Why a timer and not file notifications
//!
//! agentq rewrites heartbeat files every 20 s per ticket and the journal on
//! every event; on this machine that is several writes a second across ~60
//! directories. A change notification per write would wake the thread more
//! often than the 2-second poll does, and every read here is incremental
//! (journal and transcripts from a byte offset), so a poll costs a handful
//! of small JSON files. Measured by `prove_agents`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use vitals_core::agents::{AgentqAction, AgentsSnapshot};

/// Event carrying the whole [`AgentsSnapshot`] each refresh.
pub const AGENTS_EVENT: &str = "vitals://agents";
/// Refresh while the page is open.
const REFRESH: Duration = Duration::from_secs(2);
/// A subscription not renewed within this window is dropped.
const LEASE: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct Shared {
    latest: Mutex<Option<Arc<AgentsSnapshot>>>,
    /// Subscriber id → last renewal.
    subs: Mutex<Vec<(u64, Instant)>>,
    wake: Condvar,
    wake_lock: Mutex<bool>,
    next_id: AtomicU64,
}

/// Managed state. Cheap to clone the inner `Arc`.
#[derive(Debug, Default, Clone)]
pub struct Agents(Arc<Shared>);

impl Agents {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts the reader thread (parked until someone subscribes).
    pub fn spawn(&self, app: AppHandle) {
        let shared = Arc::clone(&self.0);
        let spawned = std::thread::Builder::new()
            .name("vitals-agents".into())
            .spawn(move || run(&shared, &app));
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "agents thread did not start");
        }
    }

    fn active(&self) -> bool {
        let now = Instant::now();
        let mut subs = self.0.subs.lock();
        subs.retain(|(_, at)| now.duration_since(*at) < LEASE);
        !subs.is_empty()
    }

    fn poke(&self) {
        *self.0.wake_lock.lock() = true;
        self.0.wake.notify_all();
    }

    /// The latest snapshot, for LAN and first paint.
    #[must_use]
    pub fn latest(&self) -> Option<Arc<AgentsSnapshot>> {
        self.0.latest.lock().clone()
    }

    /// Registers or renews a subscriber; returns its id.
    pub fn subscribe(&self, id: Option<u64>) -> u64 {
        let id = id.unwrap_or_else(|| self.0.next_id.fetch_add(1, Ordering::Relaxed) + 1);
        {
            let mut subs = self.0.subs.lock();
            let now = Instant::now();
            match subs.iter_mut().find(|(s, _)| *s == id) {
                Some(entry) => entry.1 = now,
                None => subs.push((id, now)),
            }
        }
        self.poke();
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.0.subs.lock().retain(|(s, _)| *s != id);
    }
}

fn run(shared: &Arc<Shared>, app: &AppHandle) {
    let agents = Agents(Arc::clone(shared));
    let mut reader = vitals_agents::Reader::detect();
    let mut cloud = CloudCache::default();
    loop {
        if !agents.active() {
            // park until subscribe() pokes
            let mut woken = shared.wake_lock.lock();
            while !*woken {
                shared.wake.wait(&mut woken);
            }
            *woken = false;
            continue;
        }
        let mut snap = reader.read(&probe);
        cloud.merge_into(&mut snap);
        let snapshot = Arc::new(snap);
        *shared.latest.lock() = Some(Arc::clone(&snapshot));
        if let Err(e) = app.emit(AGENTS_EVENT, &*snapshot) {
            tracing::debug!(error = %e, "agents emit failed");
        }
        let mut woken = shared.wake_lock.lock();
        if !*woken {
            shared.wake.wait_for(&mut woken, REFRESH);
        }
        *woken = false;
    }
}

/// Cloud sessions, fetched at most every [`CLOUD_EVERY`] while the page is
/// open and the user has signed in. A failed fetch keeps the last list (and
/// says so through `cloud_error`) rather than blanking it.
const CLOUD_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct CloudCache {
    at: Option<Instant>,
    /// The last good answer: hosts plus the raw session list. Kept raw so
    /// each tick filters it against the local sessions *of that tick* — a
    /// conversation that just appeared locally must not also show as cloud.
    fetched: Option<(
        Vec<vitals_agents::cloud::Host>,
        vitals_agents::cloud::Response,
    )>,
}

impl CloudCache {
    fn merge_into(&mut self, snap: &mut AgentsSnapshot) {
        if self.at.is_none_or(|t| t.elapsed() >= CLOUD_EVERY) {
            self.at = Some(Instant::now());
            if let Some(key) = cloud_key() {
                match fetch_cloud(&key) {
                    Ok(f) => self.fetched = Some(f),
                    Err(e) => tracing::debug!(error = %e, "cloud sessions"),
                }
            } else {
                self.fetched = None;
            }
        }
        let Some((hosts, res)) = &self.fetched else {
            return;
        };
        let local: std::collections::HashSet<&str> =
            snap.sessions.iter().map(|s| s.id.as_str()).collect();
        match vitals_agents::cloud::parse_sessions(
            res,
            hosts,
            &|id| local.contains(id),
            vitals_agents::time::now_ms(),
            24 * 3_600_000,
        ) {
            Ok(list) => snap.sessions.extend(list),
            Err(e) => tracing::debug!(error = %e, "cloud sessions"),
        }
    }
}

#[cfg(windows)]
fn cloud_key() -> Option<String> {
    vitals_win::credentials::read(vitals_agents::cloud::CREDENTIAL_TARGET)
        .ok()
        .flatten()
}

#[cfg(not(windows))]
fn cloud_key() -> Option<String> {
    None
}

/// One client for every codai call: a connection pool is reused across the
/// 30 s polls instead of a TLS handshake each time. 3 s to connect and 10 s
/// overall keep a dead network from holding the agents reader (and a sign-out)
/// for long.
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

fn http(req: &vitals_agents::cloud::Request) -> Result<vitals_agents::cloud::Response, String> {
    let client = client()?;
    let rt = tauri::async_runtime::handle();
    let req = req.clone();
    rt.block_on(async move {
        let mut b = if req.method == "POST" {
            client.post(&req.url)
        } else {
            client.get(&req.url)
        };
        if let Some(form) = &req.form {
            b = b.form(form);
        }
        if let Some(t) = &req.bearer {
            b = b.bearer_auth(t);
        }
        for (k, v) in &req.headers {
            b = b.header(k, v);
        }
        let res = b.send().await.map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        let body = res.text().await.map_err(|e| e.to_string())?;
        Ok((status, body))
    })
}

fn fetch_cloud(
    key: &str,
) -> Result<
    (
        Vec<vitals_agents::cloud::Host>,
        vitals_agents::cloud::Response,
    ),
    String,
> {
    use vitals_agents::cloud;
    let hosts = cloud::parse_hosts(&http(&cloud::hosts_request(cloud::DEFAULT_GATEWAY, key))?)?;
    let res = http(&cloud::sessions_request(cloud::DEFAULT_GATEWAY, key))?;
    // an error status is surfaced now, not on every later tick
    cloud::parse_sessions(&res, &hosts, &|_| false, 0, u64::MAX)?;
    Ok((hosts, res))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    pub signed_in: bool,
    pub hosts: Vec<vitals_agents::cloud::Host>,
    pub error: Option<String>,
}

/// Whether a codai key is stored, and the connected hosts when it is.
#[tauri::command]
pub async fn get_cloud_status() -> Result<CloudStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use vitals_agents::cloud;
        let Some(key) = cloud_key() else {
            return CloudStatus {
                signed_in: false,
                hosts: Vec::new(),
                error: None,
            };
        };
        match http(&cloud::hosts_request(cloud::DEFAULT_GATEWAY, &key))
            .and_then(|r| cloud::parse_hosts(&r))
        {
            Ok(hosts) => CloudStatus {
                signed_in: true,
                hosts,
                error: None,
            },
            Err(e) => CloudStatus {
                signed_in: e != "signed-out",
                hosts: Vec::new(),
                error: Some(e),
            },
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Device flows in progress; the webview only ever holds their handles.
static SIGN_INS: vitals_agents::cloud::SignIns = vitals_agents::cloud::SignIns::new();

/// Step one of the codai device flow: returns the code to show and the URL
/// to open. The secret device code stays here; the webview gets an opaque
/// handle to pass to `finish_cloud_sign_in` / `cancel_cloud_sign_in`.
#[tauri::command]
pub async fn start_cloud_sign_in() -> Result<vitals_agents::cloud::SignInPrompt, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use vitals_agents::cloud;
        let auth =
            cloud::parse_device_auth(&http(&cloud::device_auth_request(cloud::DEFAULT_AUTH))?)?;
        Ok(SIGN_INS.begin(auth))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Polls the token endpoint until the user approves (or it expires), then
/// exchanges the token for a key and stores it in Credential Manager.
/// `Ok(false)` when the flow was cancelled.
#[tauri::command]
pub async fn finish_cloud_sign_in(handle: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = poll_sign_in(&handle);
        SIGN_INS.finish(&handle);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

fn poll_sign_in(handle: &str) -> Result<bool, String> {
    use vitals_agents::cloud::{self, Poll};
    let pending = SIGN_INS
        .get(handle)
        .ok_or_else(|| "sign-in expired".to_owned())?;
    let mut wait = pending.interval.clamp(1, 30);
    let deadline = Instant::now() + Duration::from_secs(15 * 60);
    loop {
        if Instant::now() > deadline {
            return Err("sign-in expired".to_owned());
        }
        // sleep in 250 ms steps so a cancel lands within a quarter second
        let until = Instant::now() + Duration::from_secs(wait);
        while Instant::now() < until {
            if pending.cancelled.load(Ordering::Relaxed) {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if pending.cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        match cloud::parse_token(&http(&cloud::token_request(
            cloud::DEFAULT_AUTH,
            &pending.device_code,
        ))?) {
            Poll::Pending => {}
            Poll::SlowDown => wait += 5,
            Poll::Failed(e) => return Err(e),
            Poll::Granted { access_token } => {
                let key = cloud::parse_key(&http(&cloud::key_request(
                    cloud::DEFAULT_AUTH,
                    &access_token,
                ))?)?;
                // a cancel that raced the approval must not leave a key behind
                if pending.cancelled.load(Ordering::Relaxed) {
                    let _ = http(&cloud::revoke_request(cloud::DEFAULT_GATEWAY, &key));
                    return Ok(false);
                }
                store_key(&key)?;
                return Ok(true);
            }
        }
    }
}

/// Stops a sign-in the user abandoned (ENG2-13). Harmless when it already ended.
#[tauri::command]
pub async fn cancel_cloud_sign_in(handle: String) -> Result<bool, String> {
    Ok(SIGN_INS.cancel(&handle))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignOutResult {
    /// Whether codai confirmed the key no longer works. `false` (offline,
    /// codai down) still signs out here; the UI says the key may live on
    /// until it is revoked in the codai hub.
    pub revoked: bool,
}

/// Revokes the key on codai with the key itself, then deletes it here. The
/// local delete happens whatever the revoke answered: signing out must work
/// offline.
#[tauri::command]
pub async fn cloud_sign_out() -> Result<SignOutResult, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use vitals_agents::cloud;
        let revoked = cloud_key().is_none_or(|key| {
            http(&cloud::revoke_request(cloud::DEFAULT_GATEWAY, &key))
                .and_then(|r| cloud::parse_revoke(&r))
                .inspect_err(|e| tracing::warn!(error = %e, "codai key revoke failed"))
                .is_ok()
        });
        clear_key()?;
        Ok(SignOutResult { revoked })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn store_key(key: &str) -> Result<(), String> {
    vitals_win::credentials::write(vitals_agents::cloud::CREDENTIAL_TARGET, "codai", key)
        .map_err(|e| e.to_string())
}
#[cfg(windows)]
fn clear_key() -> Result<(), String> {
    vitals_win::credentials::delete(vitals_agents::cloud::CREDENTIAL_TARGET)
        .map_err(|e| e.to_string())
}
#[cfg(not(windows))]
fn store_key(_key: &str) -> Result<(), String> {
    Err("cloud sign-in needs Windows Credential Manager".into())
}
#[cfg(not(windows))]
// The signature must match the Windows `clear_key`, which can fail: both are
// handed to the same `spawn_blocking` in `cloud_sign_out`.
#[allow(clippy::unnecessary_wraps)]
fn clear_key() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn probe(pid: u32) -> Option<u64> {
    vitals_win::actions::process_start_ms(pid)
}

#[cfg(not(windows))]
fn probe(_pid: u32) -> Option<u64> {
    None
}

/// Subscribes this window to `vitals://agents` and returns the current
/// snapshot (or a fresh one) for first paint. Call again within 30 s with
/// the returned id to stay subscribed.
#[tauri::command]
pub async fn subscribe_agents(
    app: AppHandle,
    id: Option<u64>,
) -> Result<AgentsSubscription, String> {
    let agents = app.state::<Agents>().inner().clone();
    let id = agents.subscribe(id);
    let snapshot = match agents.latest() {
        Some(s) => s,
        None => tauri::async_runtime::spawn_blocking(|| {
            Arc::new(vitals_agents::Reader::detect().read(&probe))
        })
        .await
        .map_err(|e| e.to_string())?,
    };
    Ok(AgentsSubscription {
        id,
        snapshot: (*snapshot).clone(),
    })
}

#[tauri::command]
pub async fn unsubscribe_agents(app: AppHandle, id: u64) {
    app.state::<Agents>().unsubscribe(id);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsSubscription {
    pub id: u64,
    pub snapshot: AgentsSnapshot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentqActionResult {
    pub ok: bool,
    pub code: Option<i32>,
    pub output: String,
}

/// Runs an agentq action through agentq's own CLI. Desktop-only: a paired
/// phone gets the Control-scoped LAN route instead, never this command.
#[tauri::command]
pub async fn run_agentq_action(
    app: AppHandle,
    action: AgentqAction,
) -> Result<AgentqActionResult, String> {
    let out = tauri::async_runtime::spawn_blocking(move || run_action(&action))
        .await
        .map_err(|e| e.to_string())??;
    // show the effect at once instead of on the next 2 s tick
    app.state::<Agents>().poke();
    Ok(AgentqActionResult {
        ok: out.ok,
        code: out.code,
        output: out.output,
    })
}

/// Runs `action` behind the guards in `vitals_agents::actions` (ENG2,
/// S21-04): refused while elevated, script only from the shell's profile
/// folder and only if owned by this account or administrators, 120 s limit.
#[cfg(windows)]
fn run_action(action: &AgentqAction) -> Result<vitals_agents::actions::Outcome, String> {
    let profile = vitals_win::ownership::user_profile().ok_or("no user profile")?;
    let bin = vitals_agents::actions::bin_in(&profile);
    vitals_agents::actions::Runner {
        bin: &bin,
        elevated: vitals_win::actions::is_elevated(),
        trusted: &vitals_win::ownership::owned_by_user_or_admins,
        shell: "pwsh",
        timeout: vitals_agents::actions::TIMEOUT,
    }
    .run(action)
}

/// agentq is a Windows tool; elsewhere there is nothing to run.
#[cfg(not(windows))]
fn run_action(action: &AgentqAction) -> Result<vitals_agents::actions::Outcome, String> {
    vitals_agents::actions::plan(action)?;
    Err("agentq actions need Windows".into())
}

/// The subscriber id every LAN caller shares. Far above anything
/// [`Agents::subscribe`] hands out, so it can never collide with a window.
const LAN_SUBSCRIBER: u64 = u64::MAX;

/// What the LAN server serves at `/api/v1/agents` (S18-07).
///
/// A request counts as someone looking: it renews one shared subscription,
/// so the reader thread runs while a phone (or a Prometheus scraper) keeps
/// asking and parks [`LEASE`] after the last request — the same rule as the
/// window. The scraper case is deliberate: a user who points Prometheus at
/// the AI series has asked for them to be current.
pub struct LanAgents {
    app: AppHandle,
}

impl std::fmt::Debug for LanAgents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanAgents").finish_non_exhaustive()
    }
}

impl LanAgents {
    #[must_use]
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn agents(&self) -> Option<Agents> {
        self.app.try_state::<Agents>().map(|s| s.inner().clone())
    }
}

impl vitals_server::AgentsProvider for LanAgents {
    fn snapshot(&self) -> AgentsSnapshot {
        let Some(agents) = self.agents() else {
            return AgentsSnapshot::default();
        };
        agents.subscribe(Some(LAN_SUBSCRIBER));
        match agents.latest() {
            Some(s) => (*s).clone(),
            // First request since start: read once now rather than answer
            // with an empty snapshot that looks like "nothing running".
            None => vitals_agents::Reader::detect().read(&probe),
        }
    }

    fn act(
        &self,
        action: AgentqAction,
    ) -> Result<vitals_server::ActionOutcome, vitals_server::ActionError> {
        use vitals_server::ActionError;
        // Validation first, so a short reason is a 422 and never a "could
        // not start pwsh" that sends the user looking for the wrong problem.
        vitals_agents::actions::plan(&action).map_err(ActionError::Invalid)?;
        let out = run_action(&action).map_err(ActionError::Unavailable)?;
        if let Some(agents) = self.agents() {
            agents.poke();
        }
        Ok(vitals_server::ActionOutcome {
            ok: out.ok,
            code: out.code,
            output: out.output,
        })
    }
}
