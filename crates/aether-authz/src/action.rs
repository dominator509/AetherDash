use crate::Tier;
use serde::{Deserialize, Serialize};

/// Closed action vocabulary. Strings are stable audit/scope identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Subscribe,
    Query,
    Explain,
    Metrics,
    Simulate,
    DraftOrder,
    ConfigureAlerts,
    RegenerateVault,
    SubmitPaperOrder,
    LaunchSwarm,
    ReprocessInbox,
    SubmitLiveOrder,
    InstallSignedPlugin,
    ScheduleAutomation,
    GuardianApproval,
    ActivateCaps,
    CreateOrElevateGrant,
    ApprovePlugin,
    AddAllowlist,
    RevokeOtherSession,
    ReadSecretMaterial,
    SetLiveEnabled,
    RaiseCapsProgrammatically,
    DisableSafetyControl,
    WalletTransfer,
}

impl Action {
    pub const ALL_TIER_ACTIONS: [Self; 21] = [
        Self::Subscribe,
        Self::Query,
        Self::Explain,
        Self::Metrics,
        Self::Simulate,
        Self::DraftOrder,
        Self::ConfigureAlerts,
        Self::RegenerateVault,
        Self::SubmitPaperOrder,
        Self::LaunchSwarm,
        Self::ReprocessInbox,
        Self::SubmitLiveOrder,
        Self::InstallSignedPlugin,
        Self::ScheduleAutomation,
        Self::GuardianApproval,
        Self::ActivateCaps,
        Self::CreateOrElevateGrant,
        Self::ApprovePlugin,
        Self::AddAllowlist,
        Self::RevokeOtherSession,
        Self::WalletTransfer,
    ];

    #[must_use]
    pub const fn minimum_tier(self) -> Tier {
        match self {
            Self::Subscribe | Self::Query | Self::Explain | Self::Metrics => Tier::ReadOnly,
            Self::Simulate | Self::DraftOrder | Self::ConfigureAlerts | Self::RegenerateVault => {
                Tier::DraftOnly
            }
            Self::SubmitPaperOrder | Self::LaunchSwarm | Self::ReprocessInbox => {
                Tier::ConfirmEveryAction
            }
            Self::SubmitLiveOrder
            | Self::InstallSignedPlugin
            | Self::ScheduleAutomation
            | Self::GuardianApproval
            | Self::ActivateCaps
            | Self::CreateOrElevateGrant
            | Self::ApprovePlugin
            | Self::AddAllowlist
            | Self::RevokeOtherSession
            | Self::WalletTransfer => Tier::BoundedAutopilot,
            // Structural hard-denies do not become legal at a higher tier.
            Self::ReadSecretMaterial
            | Self::SetLiveEnabled
            | Self::RaiseCapsProgrammatically
            | Self::DisableSafetyControl => Tier::YoloWithinHardCaps,
        }
    }

    #[must_use]
    pub const fn is_mutating(self) -> bool {
        !matches!(self, Self::Subscribe | Self::Query | Self::Explain | Self::Metrics)
    }

    #[must_use]
    pub const fn always_requires_step_up(self) -> bool {
        matches!(
            self,
            Self::GuardianApproval
                | Self::ActivateCaps
                | Self::CreateOrElevateGrant
                | Self::ApprovePlugin
                | Self::AddAllowlist
                | Self::RevokeOtherSession
        )
    }

    #[must_use]
    pub const fn scope(self) -> &'static str {
        match self {
            Self::Subscribe => "stream.subscribe",
            Self::Query => "data.query",
            Self::Explain => "opps.explain",
            Self::Metrics => "metrics.snapshot",
            Self::Simulate => "sim.run",
            Self::DraftOrder => "orders.draft",
            Self::ConfigureAlerts => "alerts.configure",
            Self::RegenerateVault => "vault.regenerate",
            Self::SubmitPaperOrder => "orders.submit_paper",
            Self::LaunchSwarm => "swarm.launch",
            Self::ReprocessInbox => "inbox.reprocess",
            Self::SubmitLiveOrder => "orders.submit",
            Self::InstallSignedPlugin => "plugins.install_signed",
            Self::ScheduleAutomation => "automation.schedule",
            Self::GuardianApproval => "guardian.approve",
            Self::ActivateCaps => "caps.activate",
            Self::CreateOrElevateGrant => "grants.elevate",
            Self::ApprovePlugin => "plugins.approve",
            Self::AddAllowlist => "guardian.allowlist_add",
            Self::RevokeOtherSession => "sessions.revoke_other",
            Self::ReadSecretMaterial => "secrets.read",
            Self::SetLiveEnabled => "execution.set_live_enabled",
            Self::RaiseCapsProgrammatically => "caps.raise_programmatically",
            Self::DisableSafetyControl => "safety.disable",
            Self::WalletTransfer => "guardian.transfer",
        }
    }
}
