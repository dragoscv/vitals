//! The attach pipe: lets `vitals top` read this app's frames instead of
//! running a second sampler.
//!
//! Local user only, no tokens. The pipe is created with the default named-pipe
//! security descriptor, which admits the creating user's SID and
//! administrators; the name carries the account (`vitals-<user>`) so another
//! user on the same machine gets a different pipe. A process that can open it
//! already runs as this user and could read every counter itself, so a token
//! would prove nothing the kernel has not already checked — see the module
//! documentation in `vitals_ipc::attach` for the full argument. The LAN server
//! is the surface that needs tokens, because a network socket has no caller
//! identity.
//!
//! Always on, unlike the LAN server, because there is no consent question for
//! a same-user pipe — but it must cost nothing while unused. `publish` is
//! gated on `has_clients()` in `sampling.rs`, so an idle pipe costs one atomic
//! load per tick.

use std::sync::Arc;

use tauri::Manager;
use vitals_core::sample::Frame;
use vitals_ipc::AttachServer;

/// Managed state wrapping the listener. `None` when the pipe could not be
/// bound — the app keeps working; only attaching does not.
#[derive(Debug)]
pub struct AttachPipe {
    server: Option<AttachServer>,
}

impl AttachPipe {
    /// Binds the per-user pipe. Failure is logged, not fatal: a second
    /// instance (which the single-instance plugin should already refuse) or
    /// an unusual account name must not stop the monitor from starting.
    #[must_use]
    pub fn start(version: &str) -> Self {
        let name = vitals_ipc::attach::default_pipe_name();
        let server = match AttachServer::start(&name, version.to_owned()) {
            Ok(server) => {
                tracing::info!(pipe = %name, "attach pipe listening");
                Some(server)
            }
            Err(error) => {
                tracing::warn!(pipe = %name, %error, "attach pipe not started");
                None
            }
        };
        Self { server }
    }

    /// One atomic load; the sampler reads this before cloning a frame.
    #[must_use]
    pub fn has_clients(&self) -> bool {
        self.server.as_ref().is_some_and(AttachServer::has_clients)
    }

    /// A client is waiting for a complete view and the pipe has none. The
    /// sampler forces its next frame to be a keyframe in response.
    #[must_use]
    pub fn wants_keyframe(&self) -> bool {
        self.server
            .as_ref()
            .is_some_and(AttachServer::wants_keyframe)
    }

    pub fn publish(&self, frame: &Arc<Frame>) {
        if let Some(server) = &self.server {
            server.publish(frame);
        }
    }

    pub fn stop(&self) {
        if let Some(server) = &self.server {
            server.stop();
        }
    }
}

/// Installs the pipe as managed state.
pub fn start(app: &tauri::AppHandle) {
    let version = app.package_info().version.to_string();
    app.manage(AttachPipe::start(&version));
}

/// Stops accepting. Existing `top` sessions see their stream end.
pub fn stop(app: &tauri::AppHandle) {
    if let Some(pipe) = app.try_state::<AttachPipe>() {
        pipe.stop();
    }
}
