//! The Vitals elevated helper.
//!
//! Runs as a Windows service under SYSTEM and performs the operations the
//! unprivileged UI cannot: starting the ETW kernel trace session, writing
//! firewall rules, changing power plans, and acting on protected processes.
//!
//! ## Why a service rather than elevating the app
//!
//! Running the whole UI as administrator would mean a webview — the largest
//! attack surface in the product — executes with full privilege. Isolating
//! the privileged surface into a small binary with an enumerable command set
//! (see [`vitals_ipc::Command`]) means the code running as SYSTEM is a few
//! hundred lines that can be read in one sitting.
//!
//! The service is **optional**. Everything it enables degrades gracefully
//! when it is absent, reported through the capability model rather than
//! discovered by failure.

use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("VITALS_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    #[cfg(not(windows))]
    {
        anyhow::bail!("the helper service is currently Windows-only");
    }

    #[cfg(windows)]
    {
        tracing::info!(
            model_version = vitals_core::MODEL_VERSION,
            "vitals helper starting"
        );
        // Service entry point and pipe server land with the ETW work.
        anyhow::bail!("not yet implemented — service host is in progress")
    }
}
