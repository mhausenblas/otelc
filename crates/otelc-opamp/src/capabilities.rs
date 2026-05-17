//! Helpers for the OpAMP capability bitmasks.

use crate::pb::{AgentCapabilities, ServerCapabilities};

/// True if the agent capability bitmask `caps` advertises `cap`.
pub fn agent_has(caps: u64, cap: AgentCapabilities) -> bool {
    let bit = cap as u64;
    bit != 0 && (caps & bit) == bit
}

/// The capability bitmask this server advertises to agents.
pub fn server_capabilities() -> u64 {
    ServerCapabilities::AcceptsStatus as u64
        | ServerCapabilities::OffersRemoteConfig as u64
        | ServerCapabilities::AcceptsEffectiveConfig as u64
        | ServerCapabilities::OffersConnectionSettings as u64
}

/// A human-readable `(label, enabled)` list of agent capabilities, for the UI.
pub fn describe(caps: u64) -> Vec<(&'static str, bool)> {
    use AgentCapabilities::*;
    [
        ("ReportsStatus", ReportsStatus),
        ("AcceptsRemoteConfig", AcceptsRemoteConfig),
        ("ReportsEffectiveConfig", ReportsEffectiveConfig),
        ("ReportsHealth", ReportsHealth),
        ("ReportsRemoteConfig", ReportsRemoteConfig),
        ("ReportsOwnMetrics", ReportsOwnMetrics),
        ("ReportsOwnLogs", ReportsOwnLogs),
        ("ReportsOwnTraces", ReportsOwnTraces),
        ("AcceptsRestartCommand", AcceptsRestartCommand),
        (
            "AcceptsOpAMPConnectionSettings",
            AcceptsOpAmpConnectionSettings,
        ),
        ("ReportsAvailableComponents", ReportsAvailableComponents),
    ]
    .into_iter()
    .map(|(label, cap)| (label, agent_has(caps, cap)))
    .collect()
}
