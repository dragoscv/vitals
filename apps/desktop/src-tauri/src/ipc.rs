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
use vitals_ipc::{AttachServer, WidgetHost};

/// Managed state wrapping the listener. `None` when the pipe could not be
/// bound — the app keeps working; only attaching does not.
pub struct AttachPipe {
    /// Taken by [`Self::stop`], so [`Self::restart`] can tell a stopped pipe
    /// from a running one. A `Mutex` because managed state is shared by
    /// reference; uncontended, the sampler's per-tick check stays cheap.
    server: parking_lot::Mutex<Option<AttachServer>>,
    /// The handler that answers `vitals widget` requests, kept here rather than
    /// only on the server so a rebind does not lose it: every `AttachServer`
    /// starts with no host, so after `restart` a widget request would otherwise
    /// be refused for the rest of the session.
    host: parking_lot::Mutex<Option<Arc<dyn WidgetHost>>>,
    name: String,
    version: String,
}

// Derived would not work: the trait object the host slot holds is not `Debug`,
// and what the host *is* is not worth printing anyway — only whether there is
// one.
impl std::fmt::Debug for AttachPipe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachPipe")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("listening", &self.server.lock().is_some())
            .field("widgetHost", &self.host.lock().is_some())
            .finish()
    }
}

impl AttachPipe {
    /// Binds the per-user pipe. Failure is logged, not fatal: a second
    /// instance (which the single-instance plugin should already refuse) or
    /// an unusual account name must not stop the monitor from starting.
    #[must_use]
    pub fn start(version: &str) -> Self {
        Self::start_named(vitals_ipc::attach::default_pipe_name(), version)
    }

    fn start_named(name: String, version: &str) -> Self {
        let pipe = Self {
            server: parking_lot::Mutex::new(None),
            host: parking_lot::Mutex::new(None),
            name,
            version: version.to_owned(),
        };
        pipe.bind();
        pipe
    }

    /// Starts a server and attaches the stored widget host to it.
    ///
    /// Does not touch `self.server`; the caller owns that lock, and this takes
    /// it (through the host slot), so taking it here too would deadlock.
    fn start_server(&self) -> Option<AttachServer> {
        let server = match AttachServer::start(&self.name, self.version.clone()) {
            Ok(server) => {
                tracing::info!(pipe = %self.name, "attach pipe listening");
                Some(server)
            }
            Err(error) => {
                tracing::warn!(pipe = %self.name, %error, "attach pipe not started");
                None
            }
        };
        if let (Some(server), Some(host)) = (&server, self.host.lock().clone()) {
            server.set_widget_host(host);
        }
        server
    }

    fn bind(&self) {
        let server = self.start_server();
        *self.server.lock() = server;
    }

    /// Installs the handler that answers `vitals widget` requests.
    ///
    /// Applied to the running server, and remembered for any later rebind.
    pub fn set_widget_host(&self, host: Arc<dyn WidgetHost>) {
        *self.host.lock() = Some(Arc::clone(&host));
        if let Some(server) = self.server.lock().as_ref() {
            server.set_widget_host(host);
        }
    }

    /// One atomic load; the sampler reads this before cloning a frame.
    #[must_use]
    pub fn has_clients(&self) -> bool {
        self.server
            .lock()
            .as_ref()
            .is_some_and(AttachServer::has_clients)
    }

    /// A client is waiting for a complete view and the pipe has none. The
    /// sampler forces its next frame to be a keyframe in response.
    #[must_use]
    pub fn wants_keyframe(&self) -> bool {
        self.server
            .lock()
            .as_ref()
            .is_some_and(AttachServer::wants_keyframe)
    }

    pub fn publish(&self, frame: &Arc<Frame>) {
        if let Some(server) = self.server.lock().as_ref() {
            server.publish(frame);
        }
    }

    /// Stops accepting; dropping the server stops it.
    pub fn stop(&self) {
        drop(self.server.lock().take());
    }

    /// Binds again after [`Self::stop`]; a no-op while running.
    pub fn restart(&self) {
        let mut slot = self.server.lock();
        if slot.is_none() {
            *slot = self.start_server();
            // The old accept thread drops its listener only once it has
            // woken and seen the stop; until then the name can still be held.
            if slot.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(250));
                *slot = self.start_server();
            }
        }
    }
}

/// Installs the pipe as managed state.
pub fn start(app: &tauri::AppHandle) {
    let version = app.package_info().version.to_string();
    app.manage(AttachPipe::start(&version));
}

/// Registers the handler that answers `vitals widget` on the attach pipe.
///
/// A no-op when the pipe was never bound, which is the same state as a request
/// arriving: the app keeps working and only attaching does not.
pub fn set_widget_host(app: &tauri::AppHandle, host: Arc<dyn WidgetHost>) {
    if let Some(pipe) = app.try_state::<AttachPipe>() {
        pipe.set_widget_host(host);
    }
}

/// Stops accepting. Existing `top` sessions see their stream end.
pub fn stop(app: &tauri::AppHandle) {
    if let Some(pipe) = app.try_state::<AttachPipe>() {
        pipe.stop();
    }
}

/// Listens again after [`stop`], for an update that did not start.
pub fn restart(app: &tauri::AppHandle) {
    if let Some(pipe) = app.try_state::<AttachPipe>() {
        pipe.restart();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use vitals_ipc::attach::AttachClient;
    use vitals_ipc::attach::{WidgetAction, WidgetsReply};

    fn unique_name(tag: &str) -> String {
        format!("vitals-test-{tag}-{}", std::process::id())
    }

    #[test]
    fn a_pipe_stopped_for_an_update_that_did_not_start_accepts_clients_again_after_restart() {
        // ENG2-1: a declined UAC prompt left `vitals top` unable to attach
        // for the rest of the session, because nothing bound the pipe again.
        let name = unique_name("restart");
        let pipe = AttachPipe::start_named(name.clone(), "9.9.9");
        assert!(
            AttachClient::connect(&name).is_ok(),
            "not listening at start"
        );

        pipe.stop();
        assert!(
            pipe.server.lock().is_none(),
            "stop left the server in place"
        );

        pipe.restart();
        assert!(pipe.server.lock().is_some(), "restart did not bind");
        let (_client, hello) = AttachClient::connect(&name).unwrap();
        assert_eq!(hello.version, "9.9.9");
    }

    #[test]
    fn restarting_a_running_pipe_keeps_the_listener_it_has() {
        // A second bind of the same name would fail and, if it replaced the
        // running server with `None`, silently stop attaching.
        let name = unique_name("running");
        let pipe = AttachPipe::start_named(name.clone(), "1.0.0");
        pipe.restart();
        assert!(pipe.server.lock().is_some());
        assert!(AttachClient::connect(&name).is_ok());
    }

    /// The guarantee: a widget host is still answering after the rebind that
    /// follows a declined update — `vitals widget` does not work once per
    /// session. Every `AttachServer` starts with no host of its own, so this
    /// only holds because the pipe keeps the handler as well as the listener.
    #[test]
    fn a_widget_host_installed_before_an_update_answers_after_the_rebind() {
        struct Answers;

        impl WidgetHost for Answers {
            fn widgets(&self, action: &WidgetAction) -> Result<WidgetsReply, String> {
                if matches!(action, WidgetAction::List) {
                    Ok(WidgetsReply::ok(Vec::new(), None))
                } else {
                    Err("this test only asks for a list".to_owned())
                }
            }
        }

        let name = unique_name("widget-host");
        let pipe = AttachPipe::start_named(name.clone(), "9.9.9");
        pipe.set_widget_host(Arc::new(Answers));

        let (client, _) = AttachClient::connect(&name).unwrap();
        assert_eq!(
            client.widget(WidgetAction::List).unwrap().status,
            "ok",
            "the host installed at startup answers"
        );

        pipe.stop();
        pipe.restart();

        let (client, _) = AttachClient::connect(&name).unwrap();
        assert_eq!(
            client.widget(WidgetAction::List).unwrap().status,
            "ok",
            "and it is still the one answering after the rebind"
        );
    }
}
