//! A thin blocking client for the local API.
//!
//! Every method maps one route in `docs/api/openapi.yaml`. Errors carry
//! the route so a user sees "snapshot answered 500", not a bare status code.
//! `POST /control` exists for exactly the actions a subcommand issues
//! (`vitals eco`); a client method nothing calls is a promise nothing tests.

use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use vitals_core::alerts::Alert;
use vitals_core::provider::HostInfo;
use vitals_core::sample::Frame;
use vitals_server::{ControlError, ControlRequest};

/// What `/api/v1/health` returns.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub ok: bool,
    pub version: String,
    pub model_version: u32,
}

#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    agent: ureq::Agent,
    /// A separate agent for `/stream`: it must have no read timeout, since a
    /// quiet server legitimately sends nothing for up to 15 s between
    /// keep-alives, and the global one is deliberately short so a hung
    /// listener fails `vitals ps` fast rather than never.
    streaming: ureq::Agent,
}

impl Client {
    /// `base` is `http://host:port` with no trailing slash.
    #[must_use]
    pub fn new(base: &str) -> Self {
        let base = base.trim_end_matches('/').to_owned();
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        let streaming = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self {
            base,
            agent,
            streaming,
        }
    }

    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// # Errors
    /// Connection refused, a non-2xx status, or a body that is not `Health`.
    pub fn health(&self) -> Result<Health> {
        self.get_json("/api/v1/health")
    }

    /// The current, complete frame, or `None` before the first tick (204).
    ///
    /// # Errors
    /// Connection failure or an unexpected status.
    pub fn snapshot(&self) -> Result<Option<Frame>> {
        self.get_optional_json("/api/v1/snapshot")
    }

    /// # Errors
    /// Connection failure or an unexpected status. `None` when the host has
    /// no platform backend (204).
    pub fn host(&self) -> Result<Option<HostInfo>> {
        self.get_optional_json("/api/v1/host")
    }

    /// # Errors
    /// Connection failure or a non-2xx status.
    pub fn alerts(&self) -> Result<Vec<Alert>> {
        self.get_json("/api/v1/alerts")
    }

    /// `POST /api/v1/control`. A 204 is success; anything else carries a
    /// [`ControlError`] whose message is what the user sees, so "Windows
    /// protects this process" is not flattened into "answered 403".
    ///
    /// # Errors
    /// Connection failure, or the host refused the request.
    pub fn control(&self, request: &ControlRequest) -> Result<()> {
        let path = "/api/v1/control";
        let response = self
            .agent
            .post(format!("{}{path}", self.base))
            .header("content-type", "application/json")
            .send_json(request)
            .with_context(|| format!("POST {path}"))?;
        let status = response.status();
        if status == ureq::http::StatusCode::NO_CONTENT {
            return Ok(());
        }
        let text = response.into_body().read_to_string().unwrap_or_default();
        match serde_json::from_str::<ControlError>(&text) {
            Ok(error) => bail!("{error} (POST {path} answered {status})"),
            Err(_) => bail!("POST {path} answered {status}"),
        }
    }

    /// Opens `/api/v1/stream` and returns the raw body for an SSE reader.
    ///
    /// # Errors
    /// Connection failure or a non-2xx status.
    pub fn stream(&self) -> Result<Box<dyn Read + Send + Sync + 'static>> {
        let path = "/api/v1/stream";
        let response = self
            .streaming
            .get(format!("{}{path}", self.base))
            .call()
            .with_context(|| format!("GET {path}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("GET {path} answered {status}");
        }
        // No size cap: the stream is unbounded by design.
        Ok(Box::new(response.into_body().into_reader()))
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        match self.get_optional_json(path)? {
            Some(value) => Ok(value),
            None => bail!("GET {path} answered 204 where a body was required"),
        }
    }

    fn get_optional_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<Option<T>> {
        let response = self
            .agent
            .get(format!("{}{path}", self.base))
            .call()
            .with_context(|| format!("GET {path}"))?;
        let status = response.status();
        if status == ureq::http::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        if !status.is_success() {
            bail!("GET {path} answered {status}");
        }
        // A keyframe on a busy machine is a few hundred KB; ureq's default
        // 10 MB cap would be fine, but be explicit so a future change to the
        // default cannot silently truncate a large process list.
        let value = response
            .into_body()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_json::<T>()
            .with_context(|| format!("GET {path}: unexpected body"))?;
        Ok(Some(value))
    }
}
