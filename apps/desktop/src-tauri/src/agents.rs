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

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use vitals_core::agents::{AgentqAction, AgentqStatus, AgentsSnapshot, CollabSnapshot};

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
    use vitals_agents::cloud;
    if let Some(key) = vitals_win::credentials::read(cloud::CREDENTIAL_TARGET)
        .ok()
        .flatten()
    {
        return Some(key);
    }
    // A copy that signed in before a rename has its key under an older target
    // — one per rename, so the newest first. Move it so
    // an update does not silently sign the user out; the old targets are then
    // deleted only on uninstall.
    for old in cloud::LEGACY_CREDENTIAL_TARGETS {
        let Some(key) = vitals_win::credentials::read(old).ok().flatten() else {
            continue;
        };
        if vitals_win::credentials::write(cloud::CREDENTIAL_TARGET, "codwen", &key).is_ok() {
            tracing::info!("migrated the Codwen API key to its new credential target");
        }
        return Some(key);
    }
    None
}

#[cfg(not(windows))]
fn cloud_key() -> Option<String> {
    None
}

/// One client for every Codwen call: a connection pool is reused across the
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

/// Whether a Codwen key is stored, and the connected hosts when it is.
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

/// Step one of the Codwen device flow: returns the code to show and the URL
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
    /// Whether the service confirmed the key no longer works. `false`
    /// (offline, Codwen down) still signs out here; the UI says the key may
    /// live on until it is revoked in the Codwen hub.
    pub revoked: bool,
}

/// Revokes the key with the key itself, then deletes it here. The
/// local delete happens whatever the revoke answered: signing out must work
/// offline.
#[tauri::command]
pub async fn cloud_sign_out() -> Result<SignOutResult, String> {
    tauri::async_runtime::spawn_blocking(|| {
        use vitals_agents::cloud;
        let revoked = cloud_key().is_none_or(|key| {
            http(&cloud::revoke_request(cloud::DEFAULT_GATEWAY, &key))
                .and_then(|r| cloud::parse_revoke(&r))
                .inspect_err(|e| tracing::warn!(error = %e, "Codwen key revoke failed"))
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
    vitals_win::credentials::write(vitals_agents::cloud::CREDENTIAL_TARGET, "codwen", key)
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

/// Whether agentq is usable here, so the AI page's navigation item can appear
/// only where it would show something. Three `stat` calls, safe at startup.
#[tauri::command]
pub async fn get_agentq_status() -> Result<AgentqStatus, String> {
    tauri::async_runtime::spawn_blocking(agentq_status)
        .await
        .map_err(|e| e.to_string())
}

/// The profile path comes from the OS rather than the environment, exactly as
/// [`run_action`] does it: whoever started Vitals must not choose the folder
/// agentq is looked for in.
#[cfg(windows)]
fn agentq_status() -> AgentqStatus {
    vitals_agents::actions::status(vitals_win::ownership::user_profile().as_deref())
}

/// agentq is a Windows tool; elsewhere the page is read-only, so only the
/// session roots can justify showing it.
#[cfg(not(windows))]
fn agentq_status() -> AgentqStatus {
    vitals_agents::actions::status(None)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentqActionResult {
    pub ok: bool,
    pub code: Option<i32>,
    pub output: String,
}

/// What agentq and each AI client are using right now: cpu, memory, disk,
/// network and gpu, folded from the live process table.
///
/// # Why the webview hands over the process table
///
/// The same reason as `commands::diagnose`: those numbers exist in one place,
/// the frame the sampler already built, and per-process cpu is a *delta*
/// against the previous tick — a second, unsynchronised enumeration would
/// report 0 % for every process on its first call and disagree with every
/// other screen. The webview keeps the durable table from `lib/metrics`, so
/// the panel and the Processes screen describe the same instant.
///
/// Async and on the blocking pool: reading agentq's state means up to a
/// thousand small files on a busy machine, which must not hold the webview.
#[tauri::command]
pub async fn get_agent_usage(
    processes: Vec<vitals_core::process::Process>,
) -> Result<vitals_core::agents::AgentUsageSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let taken_at_ms = vitals_agents::time::now_ms();
        let owned = vitals_agents::agentq::default_root()
            .map(|root| vitals_agents::usage::read_owned(&root, &probe, taken_at_ms))
            .unwrap_or_default();
        vitals_agents::usage::rollup(&processes, &owned, &probe, taken_at_ms)
    })
    .await
    .map_err(|e| e.to_string())
}

/// The collaboration view: who is present, what the agents are asking each
/// other, and what work is in flight (ADR-0039, ADR-0049).
///
/// Read-only — the webview only ever wants the numbers, and agentq's own CLI
/// remains the only writer. Async and on the blocking pool, exactly like
/// `get_agent_usage`: the read opens hundreds of small files on a busy
/// machine, which must not hold the webview. A machine with no coordination
/// directory gets a snapshot that says so rather than an error.
#[tauri::command]
pub async fn get_collab() -> Result<CollabSnapshot, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let now = vitals_agents::time::now_ms();
        match vitals_agents::agentq::default_root() {
            Some(root) => vitals_agents::collab::read(&root, now),
            None => vitals_agents::collab::read(std::path::Path::new(""), now),
        }
    })
    .await
    .map_err(|e| e.to_string())
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

// ---------------------------------------------------------------------------
// Devcloud live status (S26-23)
// ---------------------------------------------------------------------------

/// How long `agentq-cloud.ps1 -Status -Json` may run.
///
/// The document makes a handful of `gcloud` calls (4-5 s each on this machine,
/// so several seconds for two workspaces), but a wedged `gcloud` must not hold
/// a blocking thread for ever.
const DEVCLOUD_TIMEOUT: Duration = Duration::from_secs(120);

/// One devcloud workspace's live reading, as agentq's status document carries
/// it. Every figure that could not be measured is `None`, never `0` — an
/// unmeasured heartbeat and a heartbeat of 0 s are different facts (AGENTS.md).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcloudWorkspace {
    /// The workspace name, e.g. `vitals-win`. The document's key is `ws`.
    pub ws: String,
    pub vm: Option<String>,
    pub project: Option<String>,
    pub zone: Option<String>,
    /// `idle`, `busy`, `suspended`, `stopped`, `suspending` or `unknown`.
    pub state: String,
    /// Seconds since the workspace's last heartbeat; `None` when it has none.
    pub heartbeat_age_sec: Option<i64>,
    /// Seconds the workspace agent has been idle; `None` when unmeasured.
    pub idle_sec: Option<i64>,
    /// Jobs queued for the workspace. A real measurement, so an empty queue is
    /// `Some(0)` and a failed listing is `None`.
    pub queue_depth: Option<i64>,
    /// Estimated cost in thousandths of a euro.
    ///
    /// Always `None` today: the status document does not carry a cost (that
    /// figure lives in the target repo's run journal, which this read never
    /// touches). The field exists so the panel has one place to draw it, and it
    /// draws an em dash rather than a fabricated zero.
    pub cost_milli_eur: Option<i64>,
}

/// A workspace agentq's own document flagged as impossible to measure, with
/// the reason it gave.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcloudUnreadable {
    pub ws: String,
    pub why: String,
}

/// Why the live status could not be read. A code, so the panel can say it in
/// the user's language, with a detail string for support.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DevcloudFailure {
    /// agentq is not installed, so there is no status document to read.
    NoAgentq { detail: String },
    /// Running it was refused on purpose: an elevated Vitals, or a script this
    /// account does not own — the same guards the agentq actions use.
    Blocked { detail: String },
    /// It ran and failed: `pwsh` would not start, a non-zero exit, a deadline.
    Failed { detail: String },
    /// It exited 0 but printed something this client cannot read.
    Unparsable { detail: String },
}

/// The live devcloud status: every workspace agentq's status document lists, or
/// an honest failure when it could not be read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevcloudStatus {
    pub available: bool,
    pub failure: Option<DevcloudFailure>,
    /// The document's own generation stamp (its `at`).
    pub generated_at: Option<String>,
    pub workspaces: Vec<DevcloudWorkspace>,
    /// Workspaces the document itself could not measure.
    pub unreadable: Vec<DevcloudUnreadable>,
}

impl DevcloudStatus {
    fn failed(failure: DevcloudFailure) -> Self {
        Self {
            available: false,
            failure: Some(failure),
            generated_at: None,
            workspaces: Vec::new(),
            unreadable: Vec::new(),
        }
    }
}

/// The live status of every devcloud workspace, read from agentq's own
/// read-only status document (ADR 0050).
///
/// The webview never spawns a process: this runs
/// `agentq-cloud.ps1 -Status -Json` through the same profile-and-ownership
/// guards the agentq actions apply in `vitals_agents::actions::Runner`, and
/// parses the one document it prints. The document is registry-wide — it lists
/// every workspace and ignores `-Ws` — which is exactly what the panel wants.
///
/// Failure is a state, not an error: a machine with no agentq, an elevated
/// Vitals, a non-zero exit or unreadable output all come back as a
/// [`DevcloudStatus`] with `available = false` and a `failure`. Only an
/// abandoned blocking task is an `Err`.
#[tauri::command]
pub async fn get_devcloud_status() -> Result<DevcloudStatus, String> {
    tauri::async_runtime::spawn_blocking(read_devcloud_status)
        .await
        .map_err(|e| e.to_string())
}

/// Reads the status document, turning every expected failure into a state.
fn read_devcloud_status() -> DevcloudStatus {
    if let Some(failure) = devcloud_blocked() {
        return DevcloudStatus::failed(failure);
    }
    let Some(script) = devcloud_script() else {
        return DevcloudStatus::failed(DevcloudFailure::NoAgentq {
            detail: "agentq-cloud.ps1 was not found in your profile's .copilot\\bin — is agentq \
                     installed?"
                .to_owned(),
        });
    };
    if !devcloud_trusted(&script) {
        return DevcloudStatus::failed(DevcloudFailure::Blocked {
            detail: format!(
                "{} is not owned by your account or by administrators; not running it",
                script.display()
            ),
        });
    }
    match run_status_script(&script) {
        Ok(out) => classify_run(out.ok, out.code, &out.stdout, &out.stderr),
        Err(detail) => DevcloudStatus::failed(DevcloudFailure::Failed { detail }),
    }
}

/// The first guard `vitals_agents::actions::Runner` applies: an elevated Vitals
/// never runs a script the user can rewrite, with administrator rights. A read
/// is no exception — the same user-owned file would execute.
#[cfg(windows)]
fn devcloud_blocked() -> Option<DevcloudFailure> {
    vitals_win::actions::is_elevated().then(|| DevcloudFailure::Blocked {
        detail: "Vitals is running as administrator, and agentq's scripts run only from a normal \
                 Vitals."
            .to_owned(),
    })
}

#[cfg(not(windows))]
fn devcloud_blocked() -> Option<DevcloudFailure> {
    None
}

/// The script's path. The profile comes from the OS, never from the
/// environment, exactly as `run_action` resolves it: whoever started Vitals
/// must not choose the folder a script is run from.
#[cfg(windows)]
fn devcloud_script() -> Option<PathBuf> {
    let profile = vitals_win::ownership::user_profile()?;
    let script = vitals_agents::actions::bin_in(&profile).join("agentq-cloud.ps1");
    script.is_file().then_some(script)
}

/// agentq is a Windows tool, so elsewhere there is no script to read.
#[cfg(not(windows))]
fn devcloud_script() -> Option<PathBuf> {
    None
}

/// The second guard: only a script this account (or administrators/SYSTEM)
/// owns. A script with no state directory is not enough; ownership is what
/// stops another program's file being run here.
#[cfg(windows)]
fn devcloud_trusted(script: &Path) -> bool {
    vitals_win::ownership::owned_by_user_or_admins(script)
}

#[cfg(not(windows))]
fn devcloud_trusted(_script: &Path) -> bool {
    false
}

/// What one run of the status script produced.
#[derive(Debug)]
struct RunOutput {
    ok: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs `agentq-cloud.ps1 -Status -Json` and collects its two streams.
///
/// The streams are drained on threads — the document prints one line, but an
/// error path could print a lot — and the run is killed at [`DEVCLOUD_TIMEOUT`]
/// so a wedged `gcloud` cannot hold the blocking thread for ever.
fn run_status_script(script: &Path) -> Result<RunOutput, String> {
    let mut cmd = Command::new("pwsh");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
    ])
    .arg(script)
    .args(["-Status", "-Json"])
    // The journal names who did it, and a read never touches the registry.
    .env("AGENTQ_SESSION", "vitals")
    .env("AGENTQ_CLIENT", "vitals")
    .env("AGENTQ_NO_REGISTRY", "1")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start pwsh: {e}"))?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = Instant::now() + DEVCLOUD_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "agentq-cloud.ps1 did not finish within {} s and was stopped",
                    DEVCLOUD_TIMEOUT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("waiting for pwsh: {e}")),
        }
    };
    Ok(RunOutput {
        ok: status.success(),
        code: status.code(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Reads one pipe to the end on its own thread, so a chatty script cannot fill
/// the pipe and stall.
fn drain(pipe: Option<impl std::io::Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = std::io::Read::read_to_end(&mut pipe, &mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Turns a finished run into a status. Split from the spawn so every failure
/// state can be tested without a machine that has agentq.
fn classify_run(ok: bool, code: Option<i32>, stdout: &str, stderr: &str) -> DevcloudStatus {
    if !ok {
        let ended = code.map_or_else(|| "a signal".to_owned(), |c| format!("exit {c}"));
        let detail = if stderr.trim().is_empty() {
            format!("agentq-cloud.ps1 ended with {ended}")
        } else {
            format!(
                "agentq-cloud.ps1 ended with {ended}: {}",
                tail(stderr.trim(), 400)
            )
        };
        return DevcloudStatus::failed(DevcloudFailure::Failed { detail });
    }
    match parse_devcloud_doc(stdout) {
        Ok(status) => status,
        Err(detail) => DevcloudStatus::failed(DevcloudFailure::Unparsable { detail }),
    }
}

/// The last `max` bytes of `text`, cut on a character boundary.
fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// The shape `Get-DevcloudStatusDoc` prints. The field names are read from
/// `agentq-cloud.ps1` itself, never guessed: the top level is
/// `{ v, at, workspaces, unavailable }` and each workspace row is
/// `{ ws, vm, project, zone, state, heartbeatAt, heartbeatAgeSec, idleSec,
/// queueDepth }`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDoc {
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    workspaces: Vec<RawWorkspace>,
    #[serde(default)]
    unavailable: Vec<RawUnavailable>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorkspace {
    ws: String,
    #[serde(default)]
    vm: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    zone: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    heartbeat_age_sec: Option<i64>,
    #[serde(default)]
    idle_sec: Option<i64>,
    #[serde(default)]
    queue_depth: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUnavailable {
    ws: String,
    #[serde(default)]
    why: Option<String>,
}

/// Parses the status document. `Err` is the detail for a document this client
/// cannot read.
fn parse_devcloud_doc(json: &str) -> Result<DevcloudStatus, String> {
    let raw: RawDoc = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(DevcloudStatus {
        available: true,
        failure: None,
        generated_at: raw.at,
        workspaces: raw
            .workspaces
            .into_iter()
            .map(|w| DevcloudWorkspace {
                ws: w.ws,
                vm: w.vm,
                project: w.project,
                zone: w.zone,
                state: w.state.unwrap_or_else(|| "unknown".to_owned()),
                heartbeat_age_sec: w.heartbeat_age_sec,
                idle_sec: w.idle_sec,
                queue_depth: w.queue_depth,
                cost_milli_eur: None,
            })
            .collect(),
        unreadable: raw
            .unavailable
            .into_iter()
            .map(|u| DevcloudUnreadable {
                ws: u.ws,
                why: u.why.unwrap_or_default(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document shaped exactly as `Get-DevcloudStatusDoc` prints it: one
    /// workspace fully measured, one with a missing heartbeat (so every figure
    /// is `null`, which is what the script writes rather than a zero).
    const DOC: &str = r#"{
        "v": 1,
        "at": "2026-11-25T10:00:00.0000000Z",
        "workspaces": [
            {"ws": "codai", "vm": "ws-codai", "project": "codai-training",
             "zone": "europe-west4-a", "state": "busy",
             "heartbeatAt": "2026-11-25T09:59:30.0000000Z", "heartbeatAgeSec": 30,
             "idleSec": 30, "queueDepth": 0},
            {"ws": "vitals-win", "vm": "ws-vitals-win", "project": "codai-training",
             "zone": "europe-west4-a", "state": "unknown", "heartbeatAt": null,
             "heartbeatAgeSec": null, "idleSec": null, "queueDepth": null}
        ],
        "unavailable": [{"ws": "vitals-win", "why": "no heartbeat at ws/vitals-win/heartbeat"}]
    }"#;

    #[test]
    fn an_unmeasured_figure_is_none_and_never_reads_as_zero() {
        let status = parse_devcloud_doc(DOC).expect("the document is well formed");
        let unmeasured = &status.workspaces[1];
        assert_eq!(unmeasured.ws, "vitals-win");
        assert_eq!(unmeasured.state, "unknown");
        assert_eq!(unmeasured.idle_sec, None);
        assert_eq!(unmeasured.heartbeat_age_sec, None);
        assert_eq!(unmeasured.queue_depth, None);
        // A real zero stays a zero, so the two facts cannot be confused.
        assert_eq!(status.workspaces[0].queue_depth, Some(0));
    }

    #[test]
    fn the_generation_stamp_and_the_unreadable_list_survive_the_parse() {
        let status = parse_devcloud_doc(DOC).expect("the document is well formed");
        assert!(status.available);
        assert!(status.failure.is_none());
        assert_eq!(
            status.generated_at.as_deref(),
            Some("2026-11-25T10:00:00.0000000Z")
        );
        assert_eq!(status.workspaces.len(), 2);
        assert_eq!(status.unreadable.len(), 1);
        assert_eq!(status.unreadable[0].ws, "vitals-win");
        assert_eq!(
            status.unreadable[0].why,
            "no heartbeat at ws/vitals-win/heartbeat"
        );
        // The document carries no cost, so it is absent rather than zero.
        assert_eq!(status.workspaces[0].cost_milli_eur, None);
    }

    #[test]
    fn a_document_this_client_cannot_read_is_a_state_and_not_a_panic() {
        assert!(parse_devcloud_doc("").is_err());
        assert!(parse_devcloud_doc("<html>not the document</html>").is_err());
        let status = classify_run(true, Some(0), "not the document", "");
        assert!(!status.available);
        assert!(matches!(
            status.failure,
            Some(DevcloudFailure::Unparsable { .. })
        ));
    }

    #[test]
    fn a_non_zero_exit_reports_the_code_and_the_stderr() {
        let status = classify_run(false, Some(3), "", "agentq-cloud: unknown workspace");
        assert!(!status.available);
        match status.failure {
            Some(DevcloudFailure::Failed { detail }) => {
                assert!(detail.contains("exit 3"), "{detail}");
                assert!(detail.contains("unknown workspace"), "{detail}");
            }
            other => panic!("expected a Failed state, got {other:?}"),
        }
    }

    #[test]
    fn a_successful_run_reads_the_document() {
        let status = classify_run(true, Some(0), DOC, "");
        assert!(status.available);
        assert_eq!(status.workspaces.len(), 2);
    }
}
