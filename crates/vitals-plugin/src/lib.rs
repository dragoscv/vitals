//! # vitals-plugin
//!
//! The plugin contract.
//!
//! Plugins exist to keep the core small and its licence clean. GPU vendor
//! SDKs, the `LibreHardwareMonitor` sidecar and third-party benchmarks all have
//! licensing or size characteristics that make bundling them into an MIT core
//! a bad trade. They are separate, optional, and versioned.
//!
//! ## Process isolation
//!
//! Plugins run **out of process**. An in-process ABI would be faster, but a
//! crashing plugin would take the whole app down — and these plugins talk to
//! vendor SDKs and kernel drivers, which is exactly the code most likely to
//! crash. Losing a sensor panel is acceptable; losing the task manager while
//! diagnosing a hang is not.

use serde::{Deserialize, Serialize};

/// The plugin API version this build speaks.
///
/// Bumped on any breaking change to the contract. A plugin declaring a
/// different major version is refused at load with a clear message rather
/// than being allowed to misbehave.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Metadata a plugin declares in its manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// SPDX identifier. Surfaced in the UI so a user installing a GPL plugin
    /// into an MIT app knows what they are doing.
    pub license: String,
    /// Must match [`PLUGIN_API_VERSION`]'s major.
    pub api_version: u32,
    pub capabilities: Vec<PluginCapability>,
    /// Whether the plugin needs the elevated helper.
    pub requires_elevation: bool,
    pub homepage: Option<String>,
}

impl PluginManifest {
    /// Whether this plugin can be loaded by the current build.
    #[must_use]
    pub const fn is_compatible(&self) -> bool {
        self.api_version == PLUGIN_API_VERSION
    }
}

/// What a plugin contributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PluginCapability {
    /// Supplies additional hardware sensors.
    Sensors,
    /// Supplies a benchmark suite.
    Benchmark,
    /// Supplies dashboard widgets.
    Widgets,
    /// Supplies an export format for reports.
    Exporter,
    /// Supplies hardware control (fan curves, overclocking).
    HardwareControl,
}

impl PluginCapability {
    /// Whether granting this to a plugin is a meaningful trust decision.
    ///
    /// Hardware control from third-party code can brick a GPU, so the install
    /// flow demands an explicit, separate confirmation for it.
    #[must_use]
    pub const fn requires_explicit_consent(self) -> bool {
        matches!(self, Self::HardwareControl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(api_version: u32) -> PluginManifest {
        PluginManifest {
            id: "test".into(),
            name: "Test".into(),
            version: "1.0.0".into(),
            author: "nobody".into(),
            description: String::new(),
            license: "MIT".into(),
            api_version,
            capabilities: vec![],
            requires_elevation: false,
            homepage: None,
        }
    }

    #[test]
    fn mismatched_api_version_is_incompatible() {
        assert!(manifest(PLUGIN_API_VERSION).is_compatible());
        assert!(!manifest(PLUGIN_API_VERSION + 1).is_compatible());
    }

    #[test]
    fn hardware_control_needs_explicit_consent() {
        assert!(PluginCapability::HardwareControl.requires_explicit_consent());
        assert!(!PluginCapability::Sensors.requires_explicit_consent());
    }
}
