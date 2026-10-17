#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

#[contract]
pub struct SLACalculatorContract;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod batch_downtime_tests;

pub mod adaptive_tuning;
pub mod storage_version;
pub mod submission_rate;
pub mod symbol_constants;
pub mod batch;
pub mod breach_escalation;
pub mod claim_deadline;
pub mod collateral_lock;
pub mod coordination_harness;
pub mod cross_contract_safety;
pub mod dependency_propagation;
pub mod event_correlation;
pub mod event_log_compaction;
pub mod event_payload_guard;
pub mod event_schema;
pub mod event_ttl_refresh;
pub mod force_majeure;
pub mod history_snapshot;
pub mod outage_correlation_tag;
pub mod resolution_delay;
pub mod state_integrity;
pub mod storage_helpers;
pub mod threshold_config;
pub mod trend;
pub mod version_negotiation;
pub mod whitelist;
pub mod zero_downtime_bonus;
// -----------------------------------------------------------------------
// Storage keys
// -----------------------------------------------------------------------
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");
const OPERATOR_KEY: Symbol = symbol_short!("OPERATOR"); // #28
const PENDING_ADMIN_KEY: Symbol = symbol_short!("PADMIN"); // #63
const PENDING_OP_KEY: Symbol = symbol_short!("POP"); // #64
const CONFIG_KEY: Symbol = symbol_short!("CONFIG");
const PAUSED_KEY: Symbol = symbol_short!("PAUSED"); // #27
const PAUSE_INFO_KEY: Symbol = symbol_short!("PAUSEINF"); // #66
const STATS_KEY: Symbol = symbol_short!("STATS"); // #29
const HISTORY_KEY: Symbol = symbol_short!("HIST");
const INIT_TIME_KEY: Symbol = symbol_short!("INIT_TIME");
const STORAGE_VERSION_KEY: Symbol = symbol_short!("VER");
const STORAGE_VERSION: u32 = 1;
const RESULT_SCHEMA_VERSION: u32 = 1;
const MAX_HISTORY_SIZE: u32 = 1000; // SC-062: bounded retention cap
const RETENTION_LIMIT_KEY: Symbol = symbol_short!("RETLIM"); // SC-013: configurable retention
const PROPOSAL_EXPIRATION_SECONDS: u64 = 604800; // 7 days in seconds
const MIGRATION_KEY: Symbol = symbol_short!("MIGKEY"); // #577
const MIGRATION_TIME_KEY: Symbol = symbol_short!("MIGTIME"); // #577
const MIGRATION_TIMELOCK: u64 = 1_209_600; // 14 days in seconds
const CONFIG_UPD_COUNT_KEY: Symbol = symbol_short!("CFGUPDCT"); // #560: total config updates
const MAX_OUTAGE_DURATION_KEY: Symbol = symbol_short!("MAX_DUR"); // #671: max single outage duration cap
const DEFAULT_MAX_OUTAGE_DURATION_MINUTES: u32 = 10_080; // 7 days in minutes (7 * 24 * 60)

// -----------------------------------------------------------------------
// Events
//
// All events share the same topic layout:
//   topic[0] = event name (Symbol constant below)
//   topic[1] = event version ("v1")
//   topic[2] = event-specific context (severity, caller address, etc.)
//
// Event payloads (data tuple field order):
//
//   sla_calc  → (outage_id: Symbol, status: Symbol, payment_type: Symbol,
//                rating: Symbol, mttr_minutes: u32, threshold_minutes: u32,
//                amount: i128)
//
//   cfg_upd   → (threshold_minutes: u32, penalty_per_minute: i128,
//                reward_base: i128)
//             context = severity Symbol
//
//   paused    → (true,)
//   unpause   → (false,)
//             context = caller Address
//
//   op_set    → (new_operator: Address,)
//             context = caller Address
//
//   pruned    → (removed_count: u32, kept_count: u32)
//             context = caller Address
//
// Versioning: breaking payload changes increment the version symbol (v2, …).
// Additive fields are not considered breaking.
// -----------------------------------------------------------------------
const EVENT_SLA_CALC: Symbol = symbol_short!("sla_calc");
const EVENT_SETTLE_INTENT: Symbol = symbol_short!("set_int");
const EVENT_CONFIG_UPD: Symbol = symbol_short!("cfg_upd");
const EVENT_PAUSED: Symbol = symbol_short!("paused"); // #27
const EVENT_UNPAUSED: Symbol = symbol_short!("unpause"); // #27
const EVENT_OP_SET: Symbol = symbol_short!("op_set"); // #28
const EVENT_PRUNED: Symbol = symbol_short!("pruned");
const EVENT_PRUNED_AGE: Symbol = symbol_short!("pruned_a"); // SC-063
const EVENT_ADMIN_PROP: Symbol = symbol_short!("adm_prop"); // #63
const EVENT_ADMIN_ACC: Symbol = symbol_short!("adm_acc"); // #63
const EVENT_ADMIN_CAN: Symbol = symbol_short!("adm_can"); // SC-024
const EVENT_ADMIN_REN: Symbol = symbol_short!("adm_ren"); // #65
const EVENT_OP_PROP: Symbol = symbol_short!("op_prop"); // #64
const EVENT_OP_ACC: Symbol = symbol_short!("op_acc"); // #64
const EVENT_OP_CAN: Symbol = symbol_short!("op_can"); // SC-024
const EVENT_OP_REV: Symbol = symbol_short!("op_rev"); // #472
const EVENT_SLA_VIOLATED: Symbol = symbol_short!("sla_viol"); // #594
const EVENT_SLA_MET: Symbol = symbol_short!("sla_met"); // #595
const EVENT_ROLE_AUDIT: Symbol = symbol_short!("role_aud"); // #576
const EVENT_MIGRATION_SET: Symbol = symbol_short!("migr_set"); // #577
const EVENT_MIGRATION_ACT: Symbol = symbol_short!("migr_act"); // #577
const EVENT_HISTORY_PRUNED_Q: Symbol = symbol_short!("hist_pq"); // #578
const EVENT_VERSION: Symbol = symbol_short!("v1");

// -----------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum SLAError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ConfigNotFound = 4,
    VersionMismatch = 5,
    ContractPaused = 6,            // #27
    NoPendingTransfer = 7,         // #63 #64
    InvalidThreshold = 8,          // #70
    InvalidPenalty = 9,            // #70
    InvalidReward = 10,            // #70
    InvalidSeverity = 11,          // #70
    RetentionLimitOutOfRange = 12, // SC-013
    DuplicateOutageInput = 13,     // SC-W5-046
    InvalidPenaltyAmount = 14,     // SC-W5-046
    InvalidRewardAmount = 15,      // SC-W5-046
    InvalidOutageId = 16,          // SC-W5-077
    MalformedSymbolInput = 17,     // SC-W5-077
    InvalidMTTR = 18,              // SC-W5-471
    ThresholdOutOfBounds = 19,
    PenaltyOutOfBounds = 20,
    RewardOutOfBounds = 21,
    InvalidMonth = 22,
    InvalidTimestampSequence = 23, // #664 – out-of-order outage ingestion
    DivisionByZero = 24,           // #663 – zero-denominator SLA precision math
}

// -----------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAConfig {
    pub threshold_minutes: u32,
    pub penalty_per_minute: i128,
    pub reward_base: i128,
    pub top_tier_multiplier: u32,
    pub excel_tier_multiplier: u32,
    pub good_tier_multiplier: u32,
}

/// Input type for outage data used in SLA calculations
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutageInput {
    pub outage_id: Symbol,
    pub severity: Symbol,
    pub mttr_minutes: u32,
}

/// Result type specifically for simulation outputs, containing only computational results
/// without any on-chain metadata
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlaSimulationResult {
    pub is_breach: bool,
    pub penalty_amount: i128,
    pub uptime_bps: u32,
    pub applied_tier: Option<Symbol>,
}

/// Original SLAResult type for persisted on-chain calculations
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAResult {
    pub outage_id: Symbol,
    pub status: Symbol, // "met" | "viol"
    pub mttr_minutes: u32,
    pub threshold_minutes: u32,
    pub amount: i128,         // negative = penalty, positive = reward
    pub payment_type: Symbol, // "rew" | "pen"
    pub rating: Symbol,       // "top" | "excel" | "good" | "poor"
    pub config_version_hash: soroban_sdk::BytesN<32>, // deterministic binding to config used for evaluation
    pub recorded_at: u64, // SC-063: ledger timestamp at calculation time
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAConfigEntry {
    pub severity: Symbol,
    pub config: SLAConfig,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAConfigSnapshot {
    pub version: Symbol,
    /// Deterministic hash of all severity configs (same algorithm as get_config_version_hash).
    pub version_hash: u64,
    pub entries: Vec<SLAConfigEntry>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAResultSchema {
    pub version: Symbol,
    pub schema_version: u32,
    pub status_met: Symbol,
    pub status_violated: Symbol,
    pub payment_reward: Symbol,
    pub payment_penalty: Symbol,
    pub rating_exceptional: Symbol,
    pub rating_excellent: Symbol,
    pub rating_good: Symbol,
    pub rating_poor: Symbol,
    pub includes_config_version_hash: bool,
}

/// #60 – Single introspection call for backend clients.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractMetadata {
    pub contract_name: Symbol,
    pub storage_version: u32,
    pub result_schema_version: u32,
    pub supported_severities: Vec<Symbol>,
    pub features: Vec<Symbol>,
}

/// #29 – Cumulative on-chain SLA performance metrics.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SLAStats {
    pub total_calculations: u64,
    pub total_violations: u64,
    pub total_rewards: i128,   // sum of all reward amounts paid out
    pub total_penalties: i128, // sum of all penalty amounts (stored positive)
}

/// #66 – Pause metadata stored when the contract is paused.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseInfo {
    pub reason: String,
    pub paused_at: u64, // ledger timestamp (seconds)
    pub paused_by: Address,
}

/// Pending governance proposal with timestamp for expiration tracking.
/// Stores the target address and when the proposal was created.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingProposalInfo {
    pub target: Address,
    pub proposed_at: u64, // ledger timestamp (seconds) when proposal was created
}

/// SC-021 – Storage version and migration posture for off-chain consumers.
///
/// Backend consumers should call `get_migration_state` after any contract upgrade
/// to confirm the storage version matches expectations before resuming operations.
/// If `needs_migration` is true, the admin must call `migrate` before the contract
/// will accept versioned calls.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageVersionInfo {
    /// The version currently stamped in storage.
    pub stored_version: u32,
    /// The version this contract binary expects.
    pub expected_version: u32,
    /// True when stored_version != expected_version (migration required).
    pub needs_migration: bool,
}

/// SC-W5-046 – Typed failure code mapping entry for backend bridge consumption.
///
/// Each `FailureCode` maps a numeric error code to a machine-readable Symbol
/// label and a short human-readable description. Backends call
/// `get_failure_schema` to obtain the full catalogue at startup.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureCode {
    /// The numeric error code matching the SLAError discriminant.
    pub code: u32,
    /// A machine-readable Symbol label (e.g. "AlreadyInitialized").
    pub label: Symbol,
    /// A short description of the failure condition.
    pub description: Symbol,
}

/// SC-W5-046 – Full catalogue of typed failure codes for backend bridge.
///
/// Backend consumers can call `get_failure_schema` once at startup to
/// pre-load all possible failure codes the contract may return. The schema
/// is versioned to allow backwards-compatible additions.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureSchema {
    /// Schema version for the failure code catalogue.
    pub version: Symbol,
    /// All known failure codes in ascending order.
    pub codes: Vec<FailureCode>,
}

/// SC-W5-029 – Combined version negotiation response for backend startup handshake.
///
/// Backend consumers call `get_version_info` once at startup (or after an upgrade)
/// to determine whether the contract is safe to use. All version-relevant fields
/// are returned in a single read to minimise round-trips.
///
/// Decision logic for backends:
/// - `needs_migration == true`  → block operations, alert admin to call `migrate`
/// - `is_paused == true`        → surface pause reason, retry after `unpause`
/// - `storage_version != result_schema_version` (unexpected) → log and alert
/// - otherwise                  → contract is ready; proceed normally
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionInfo {
    /// Storage schema version stamped in contract storage.
    pub storage_version: u32,
    /// Result schema version for SLAResult field layout.
    pub result_schema_version: u32,
    /// True when stored storage version differs from the binary's expected version.
    pub needs_migration: bool,
    /// True when the contract is currently paused.
    pub is_paused: bool,
    /// Human-readable contract name for log correlation.
    pub contract_name: Symbol,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InitializationStatus {
    pub is_initialized: bool,
    pub initialized_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistorySummary {
    pub total: u32,
    pub met: u32,
    pub violated: u32,
}

use soroban_sdk::xdr::ToXdr;

pub fn compute_config_version_hash(
    env: &Env,
    configs: &soroban_sdk::Map<Symbol, SLAConfig>,
) -> soroban_sdk::BytesN<32> {
    env.crypto().sha256(&configs.clone().to_xdr(env)).into()
}

// -----------------------------------------------------------------------
// Contract implementation
// -----------------------------------------------------------------------
#[contractimpl]
impl SLACalculatorContract {
    // -------------------------------------------------------------------
    // Initialisation
    // -------------------------------------------------------------------

    /// Deploy the contract.
    /// `admin`    – may update config, pause/unpause, and assign the operator.
    /// `operator` – may call `calculate_sla`.
    pub fn initialize(env: Env, admin: Address, operator: Address) -> Result<(), SLAError> {
        if env.storage().instance().has(&ADMIN_KEY) {
            return Err(SLAError::AlreadyInitialized);
        }

        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&OPERATOR_KEY, &operator); // #28
        env.storage().instance().set(&PAUSED_KEY, &false); // #27

        // #29 – initialise zeroed stats
        env.storage().instance().set(
            &STATS_KEY,
            &SLAStats {
                total_calculations: 0,
                total_violations: 0,
                total_rewards: 0,
                total_penalties: 0,
            },
        );
        env.storage()
            .instance()
            .set(&HISTORY_KEY, &Vec::<SLAResult>::new(&env));
        env.storage()
            .instance()
            .set(&INIT_TIME_KEY, &env.ledger().timestamp());

        let configs = Self::initialize_default_configs(&env);

        env.storage().instance().set(&CONFIG_KEY, &configs);
        Self::write_version(&env);
        Ok(())
    }

    // -------------------------------------------------------------------
    // #61 – Storage migration harness
    // -------------------------------------------------------------------

    /// Migrate storage from a previous version to the current one.
    ///
    /// Must be called by admin after a contract upgrade that bumps STORAGE_VERSION.
    /// The harness applies each step in sequence (v0→v1, v1→v2, …) so a contract
    /// that is multiple versions behind is brought fully up to date in one call.
    /// Re-invoking when already current is a safe no-op (idempotent).
    /// If an unknown stored version is encountered the call returns
    /// `VersionMismatch` without mutating any state.
    pub fn migrate(env: Env, caller: Address) -> Result<(), SLAError> {
        // Require admin without going through check_version (state may be unversioned)
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(SLAError::NotInitialized)?;
        if caller != admin {
            return Err(SLAError::Unauthorized);
        }

        let stored: u32 = env
            .storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .unwrap_or(0);

        // Already current – idempotent no-op
        if stored == STORAGE_VERSION {
            return Ok(());
        }

        // Reject versions newer than what this binary knows about
        if stored > STORAGE_VERSION {
            return Err(SLAError::VersionMismatch);
        }

        // Apply each step in sequence.  Each arm must be a pure, atomic
        // transformation: read old state → write new state → bump version.
        // A future version bump adds a new arm here; existing arms are never
        // modified so older migration paths remain auditable.
        let mut current = stored;

        // v0 → v1: stamp the version; all other fields were set by initialize
        if current == 0 {
            env.storage().instance().set(&STORAGE_VERSION_KEY, &1u32);
            current = 1;
        }

        // v1 → v2 (placeholder for the next breaking state change):
        // if current == 1 {
        //     // … transform state …
        //     env.storage().instance().set(&STORAGE_VERSION_KEY, &2u32);
        //     current = 2;
        // }

        // Sanity: after all steps we must be at STORAGE_VERSION
        assert!(current == STORAGE_VERSION, "migration validation failed");
        if current != STORAGE_VERSION {
            return Err(SLAError::VersionMismatch);
        }

        Ok(())
    }

    // -------------------------------------------------------------------
    // Role queries
    // -------------------------------------------------------------------

    pub fn get_admin(env: Env) -> Result<Address, SLAError> {
        Self::check_version(&env)?;
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    /// #28 – Returns the current operator address.
    pub fn get_operator(env: Env) -> Result<Address, SLAError> {
        Self::check_version(&env)?;
        env.storage()
            .instance()
            .get(&OPERATOR_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    /// #569 – Soroban address authorization check.
    ///
    /// Enforces that `caller` actually authorized this invocation via
    /// `require_auth()` (not merely that the passed address string matches),
    /// then verifies the caller holds the admin role. Returns `Unauthorized`
    /// for any non-admin caller. This is the on-chain authorization primitive
    /// admin-gated entry points should build on.
    pub fn verify_admin_auth(env: Env, caller: Address) -> Result<(), SLAError> {
        caller.require_auth();
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        Ok(())
    }

    // -------------------------------------------------------------------
    // #28 – Operator management (admin only)
    // -------------------------------------------------------------------

    /// Replace the operator address (admin only).
    /// Emits an `op_set` event and a `role_aud` audit trail event (#576).
    pub fn set_operator(env: Env, caller: Address, new_operator: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let prev_operator: Option<Address> = env.storage().instance().get(&OPERATOR_KEY);
        env.storage().instance().set(&OPERATOR_KEY, &new_operator);

        env.events().publish(
            (EVENT_OP_SET, EVENT_VERSION, caller.clone()),
            (new_operator.clone(),),
        );

        // #576 – Role audit trail event
        Self::publish_role_audit_event(
            &env,
            &caller,
            symbol_short!("operator"),
            prev_operator.as_ref(),
            Some(&new_operator),
        );

        Ok(())
    }

    /// #472 – Revoke the operator role entirely (admin only).
    ///
    /// Removes the operator address and any pending operator proposal from
    /// contract storage. After revocation, `calculate_sla` will fail with
    /// `Unauthorized` until a new operator is configured via `set_operator`
    /// or the two-step handoff (`propose_operator` / `accept_operator`).
    /// Emits an `op_rev` event with the caller address context.
    pub fn revoke_operator(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        env.storage().instance().remove(&OPERATOR_KEY);
        env.storage().instance().remove(&PENDING_OP_KEY);

        env.events()
            .publish((EVENT_OP_REV, EVENT_VERSION, caller), ());

        Ok(())
    }

    /// #575 – The current operator renounces their own role.
    ///
    /// Only the active operator may call this; a non-operator caller is rejected
    /// with `Unauthorized`. Removes the operator address and any pending operator
    /// proposal from storage, after which `calculate_sla` fails with
    /// `Unauthorized` until a new operator is configured. Emits an `op_rev` event
    /// carrying the renouncing caller as context.
    pub fn renounce_operator(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_operator(&env, &caller)?;

        env.storage().instance().remove(&OPERATOR_KEY);
        env.storage().instance().remove(&PENDING_OP_KEY);

        env.events()
            .publish((EVENT_OP_REV, EVENT_VERSION, caller), ());

        Ok(())
    }

    // -------------------------------------------------------------------
    // #63 – Two-step admin transfer
    // -------------------------------------------------------------------

    /// Propose a new admin. The current admin initiates; the new admin must call `accept_admin`.
    /// Proposals expire after 7 days.
    pub fn propose_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        let proposal = PendingProposalInfo {
            target: new_admin.clone(),
            proposed_at: env.ledger().timestamp(),
        };
        env.storage().instance().set(&PENDING_ADMIN_KEY, &proposal);
        env.events()
            .publish((EVENT_ADMIN_PROP, EVENT_VERSION, caller), (new_admin,));
        Ok(())
    }

    /// Accept a pending admin transfer. Must be called by the proposed new admin.
    /// Proposals expire after 7 days.
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        let pending: PendingProposalInfo = env
            .storage()
            .instance()
            .get(&PENDING_ADMIN_KEY)
            .ok_or(SLAError::NoPendingTransfer)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now - pending.proposed_at > PROPOSAL_EXPIRATION_SECONDS {
            env.storage().instance().remove(&PENDING_ADMIN_KEY);
            return Err(SLAError::NoPendingTransfer);
        }

        if caller != pending.target {
            return Err(SLAError::Unauthorized);
        }
        let prev_admin: Option<Address> = env.storage().instance().get(&ADMIN_KEY);
        env.storage().instance().set(&ADMIN_KEY, &caller);
        env.storage().instance().remove(&PENDING_ADMIN_KEY);
        env.events()
            .publish((EVENT_ADMIN_ACC, EVENT_VERSION, caller.clone()), ());

        // #576 – Role audit trail event
        Self::publish_role_audit_event(
            &env,
            &caller,
            symbol_short!("admin"),
            prev_admin.as_ref(),
            Some(&caller),
        );

        Ok(())
    }

    /// Cancel a pending admin transfer. Only the current admin may cancel.
    /// Clears the pending proposal without changing the active admin.
    /// Returns `NoPendingTransfer` if there is nothing to cancel.
    pub fn cancel_admin_proposal(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        if !env.storage().instance().has(&PENDING_ADMIN_KEY) {
            return Err(SLAError::NoPendingTransfer);
        }
        env.storage().instance().remove(&PENDING_ADMIN_KEY);
        env.events()
            .publish((EVENT_ADMIN_CAN, EVENT_VERSION, caller), ());
        Ok(())
    }

    /// Returns the pending admin address, if any (only returns if proposal is still valid).
    pub fn get_pending_admin(env: Env) -> Result<Option<Address>, SLAError> {
        Self::check_version(&env)?;
        let pending: Option<PendingProposalInfo> = env.storage().instance().get(&PENDING_ADMIN_KEY);
        if let Some(proposal) = pending {
            let now = env.ledger().timestamp();
            if now - proposal.proposed_at <= PROPOSAL_EXPIRATION_SECONDS {
                return Ok(Some(proposal.target));
            } else {
                // Remove expired proposal
                env.storage().instance().remove(&PENDING_ADMIN_KEY);
            }
        }
        Ok(None)
    }

    /// Returns the pending operator address, if any (only returns if proposal is still valid).
    pub fn get_pending_operator(env: Env) -> Result<Option<Address>, SLAError> {
        Self::check_version(&env)?;
        let pending: Option<PendingProposalInfo> = env.storage().instance().get(&PENDING_OP_KEY);
        if let Some(proposal) = pending {
            let now = env.ledger().timestamp();
            if now - proposal.proposed_at <= PROPOSAL_EXPIRATION_SECONDS {
                return Ok(Some(proposal.target));
            } else {
                // Remove expired proposal
                env.storage().instance().remove(&PENDING_OP_KEY);
            }
        }
        Ok(None)
    }

    // -------------------------------------------------------------------
    // #64 – Two-step operator handoff
    // -------------------------------------------------------------------

    /// Propose a new operator. The current admin initiates; the new operator must call `accept_operator`.
    /// Proposals expire after 7 days.
    pub fn propose_operator(
        env: Env,
        caller: Address,
        new_operator: Address,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        let proposal = PendingProposalInfo {
            target: new_operator.clone(),
            proposed_at: env.ledger().timestamp(),
        };
        env.storage().instance().set(&PENDING_OP_KEY, &proposal);
        env.events()
            .publish((EVENT_OP_PROP, EVENT_VERSION, caller), (new_operator,));
        Ok(())
    }

    /// Accept a pending operator handoff. Must be called by the proposed new operator.
    /// Proposals expire after 7 days.
    pub fn accept_operator(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        let pending: PendingProposalInfo = env
            .storage()
            .instance()
            .get(&PENDING_OP_KEY)
            .ok_or(SLAError::NoPendingTransfer)?;

        // Check if proposal has expired
        let now = env.ledger().timestamp();
        if now - pending.proposed_at > PROPOSAL_EXPIRATION_SECONDS {
            env.storage().instance().remove(&PENDING_OP_KEY);
            return Err(SLAError::NoPendingTransfer);
        }

        if caller != pending.target {
            return Err(SLAError::Unauthorized);
        }
        let prev_operator: Option<Address> = env.storage().instance().get(&OPERATOR_KEY);
        env.storage().instance().set(&OPERATOR_KEY, &caller);
        env.storage().instance().remove(&PENDING_OP_KEY);
        env.events()
            .publish((EVENT_OP_ACC, EVENT_VERSION, caller.clone()), ());

        // #576 – Role audit trail event
        Self::publish_role_audit_event(
            &env,
            &caller,
            symbol_short!("operator"),
            prev_operator.as_ref(),
            Some(&caller),
        );

        Ok(())
    }

    /// Cancel a pending operator proposal. Only the current admin may cancel.
    /// Clears the pending proposal without changing the active operator.
    /// Returns `NoPendingTransfer` if there is nothing to cancel.
    pub fn cancel_operator_proposal(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        if !env.storage().instance().has(&PENDING_OP_KEY) {
            return Err(SLAError::NoPendingTransfer);
        }
        env.storage().instance().remove(&PENDING_OP_KEY);
        env.events()
            .publish((EVENT_OP_CAN, EVENT_VERSION, caller), ());
        Ok(())
    }

    // -------------------------------------------------------------------
    // #65 – Admin renounce
    // -------------------------------------------------------------------

    /// Permanently renounce admin authority. This is irreversible: no admin will
    /// exist after this call and admin-gated functions will be permanently locked.
    /// Any pending admin proposal is also cleared.
    pub fn renounce_admin(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        env.storage().instance().remove(&ADMIN_KEY);
        env.storage().instance().remove(&PENDING_ADMIN_KEY);
        env.events()
            .publish((EVENT_ADMIN_REN, EVENT_VERSION, caller), ());
        Ok(())
    }

    /// Pause the contract with a reason and timestamp.
    /// `calculate_sla` will be blocked until unpaused.
    /// Emits a `paused` event.
    pub fn pause(env: Env, caller: Address, reason: String) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let paused_at = env.ledger().timestamp();
        env.storage().instance().set(&PAUSED_KEY, &true);
        env.storage().instance().set(
            &PAUSE_INFO_KEY,
            &PauseInfo {
                reason,
                paused_at,
                paused_by: caller.clone(),
            },
        );
        env.events()
            .publish((EVENT_PAUSED, EVENT_VERSION, caller), (true,));
        Ok(())
    }

    /// Unpause the contract. Clears pause metadata.
    /// Emits an `unpause` event.
    pub fn unpause(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        env.storage().instance().set(&PAUSED_KEY, &false);
        env.storage().instance().remove(&PAUSE_INFO_KEY);
        env.events()
            .publish((EVENT_UNPAUSED, EVENT_VERSION, caller), (false,));
        Ok(())
    }

    /// Returns `true` when the contract is paused.
    pub fn is_paused(env: Env) -> Result<bool, SLAError> {
        Self::check_version(&env)?;
        Ok(env.storage().instance().get(&PAUSED_KEY).unwrap_or(false))
    }

    /// Returns pause metadata (reason + timestamp) if currently paused, else None.
    pub fn get_pause_info(env: Env) -> Result<Option<PauseInfo>, SLAError> {
        Self::check_version(&env)?;
        Ok(env.storage().instance().get(&PAUSE_INFO_KEY))
    }

    // -------------------------------------------------------------------
    // Config management (admin only)                                 #28
    // -------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn set_config(
        env: Env,
        caller: Address,
        severity: Symbol,
        threshold_minutes: u32,
        penalty_per_minute: i128,
        reward_base: i128,
        top_tier_multiplier: u32,
        excel_tier_multiplier: u32,
        good_tier_multiplier: u32,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?; // #28 – admin role enforced

        // Load existing configs first to validate against
        let mut configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;

        // #70 – Validate configuration parameters, including cross-severity relationships
        Self::validate_config(
            &env,
            &severity,
            threshold_minutes,
            penalty_per_minute,
            reward_base,
            &configs,
        )?;

        let new_config = SLAConfig {
            threshold_minutes,
            penalty_per_minute,
            reward_base,
            top_tier_multiplier,
            excel_tier_multiplier,
            good_tier_multiplier,
        };

        // #556 – idempotency: a re-submission of the identical configuration for
        // this severity is a no-op (no storage write, no event emitted).
        if let Some(existing) = configs.get(severity.clone()) {
            if existing == new_config {
                return Ok(());
            }
        }

        configs.set(severity.clone(), new_config);
        env.storage().instance().set(&CONFIG_KEY, &configs);

        // #560 – track total successful configuration updates.
        let updates: u32 = env
            .storage()
            .instance()
            .get(&CONFIG_UPD_COUNT_KEY)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&CONFIG_UPD_COUNT_KEY, &(updates + 1));

        env.events().publish(
            (EVENT_CONFIG_UPD, EVENT_VERSION, severity),
            (threshold_minutes, penalty_per_minute, reward_base),
        );
        Ok(())
    }

    /// #560 – Total number of successful `set_config` updates over the contract
    /// lifetime.
    pub fn get_config_update_count(env: Env) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&CONFIG_UPD_COUNT_KEY)
            .unwrap_or(0))
    }

    /// #562 – Convert a configuration threshold expressed in minutes to ledger
    /// seconds.
    pub fn threshold_to_seconds(threshold_minutes: u32) -> u64 {
        (threshold_minutes as u64) * 60
    }

    /// #561 – Export the complete configuration map for contract upgrades.
    pub fn export_config_map(env: Env) -> Result<Map<Symbol, SLAConfig>, SLAError> {
        Self::check_version(&env)?;
        env.storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    /// #561 – Import a complete configuration map (admin only) during an upgrade.
    ///
    /// Validates integrity before persisting: the map must be non-empty and every
    /// severity key must be canonical, otherwise the import is rejected and the
    /// existing configuration is left untouched.
    pub fn import_config_map(
        env: Env,
        caller: Address,
        configs: Map<Symbol, SLAConfig>,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        if configs.is_empty() {
            return Err(SLAError::ConfigNotFound);
        }
        for (severity, _config) in configs.iter() {
            if !Self::is_canonical_severity(&severity) {
                return Err(SLAError::InvalidSeverity);
            }
        }
        env.storage().instance().set(&CONFIG_KEY, &configs);
        Ok(())
    }

    pub fn get_config(env: Env, severity: Symbol) -> Result<SLAConfig, SLAError> {
        Self::check_version(&env)?;
        Self::load_config(&env, &severity)
    }

    pub fn list_configs(env: Env) -> Result<Map<Symbol, SLAConfig>, SLAError> {
        Self::check_version(&env)?;
        env.storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    /// Returns a deterministic backend-friendly snapshot of all config values.
    pub fn get_config_snapshot(env: Env) -> Result<SLAConfigSnapshot, SLAError> {
        Self::check_version(&env)?;

        let mut entries = Vec::new(&env);

        for severity in Self::canonical_severities(&env) {
            let config = Self::load_config(&env, &severity)?;
            entries.push_back(SLAConfigEntry { severity, config });
        }

        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        let version_hash = Self::compute_config_version_hash_u64(&env, &configs)?;

        Ok(SLAConfigSnapshot {
            version: symbol_short!("v1"),
            version_hash,
            entries,
        })
    }

    pub fn reset_config_to_defaults(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let configs = Self::initialize_default_configs(&env);

        env.storage().instance().set(&CONFIG_KEY, &configs);
        Ok(())
    }

    /// Returns a deterministic config version hash so backend sync logic can
    /// detect meaningful config changes cheaply.
    ///
    /// The hash uses a polynomial rolling hash with a prime base and modulus
    /// to provide strong collision resistance while remaining deterministic.
    /// It processes all severity config fields in canonical order
    /// (critical → high → medium → low) and is stable across repeated reads
    /// when config is unchanged.
    pub fn get_config_version_hash(env: Env) -> Result<u64, SLAError> {
        Self::check_version(&env)?;
        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        Self::compute_config_version_hash_u64(&env, &configs)
    }

    /// Returns a deterministic hash of an SLAResult payload using the same
    /// polynomial rolling hash algorithm as `get_config_version_hash`.
    ///
    /// Off-chain backend services can use this to verify whether a given
    /// SLAResult matches an authentic contract outcome without re-executing
    /// stateful calculation calls.
    pub fn compute_result_hash(env: Env, result: SLAResult) -> Result<u64, SLAError> {
        Self::check_version(&env)?;
        Ok(Self::compute_result_hash_inner(&result))
    }

    /// SC-W5-046 – Returns the full catalogue of typed failure codes.
    ///
    /// Backend bridge consumers call this once at startup to pre-load all
    /// contract error codes and their human-readable labels. The schema is
    /// versioned ("v1") so backends can detect additions across upgrades.
    /// SC-W5-046 – Returns the full catalogue of typed failure codes.
    ///
    /// Backend bridge consumers call this once at startup to pre-load all
    /// contract error codes and their human-readable labels. The schema is
    /// versioned ("v1") so backends can detect additions across upgrades.
    pub fn get_failure_schema(env: Env) -> Result<FailureSchema, SLAError> {
        Self::check_version(&env)?;
        let mut codes = Vec::new(&env);

        // Emit in numeric order for deterministic consumption
        // All descriptions must be <= 32 bytes (Soroban Symbol constraint)
        let entries: [(u32, &str, &str); 24] = [
            (1, "AlreadyInitialized", "Contract already initialized"),
            (2, "NotInitialized", "Contract not yet initialized"),
            (3, "Unauthorized", "Caller lacks required role"),
            (4, "ConfigNotFound", "No config for severity"),
            (5, "VersionMismatch", "Storage version mismatch"),
            (6, "ContractPaused", "Contract is paused"),
            (7, "NoPendingTransfer", "No pending transfer"),
            (8, "InvalidThreshold", "Threshold out of range"),
            (9, "InvalidPenalty", "Penalty out of range"),
            (10, "InvalidReward", "Reward out of range"),
            (11, "InvalidSeverity", "Severity not supported"),
            (
                12,
                "RetentionLimitOutOfRange",
                "Retention limit out of range",
            ),
            (13, "DuplicateOutageInput", "Duplicate outage input"),
            (14, "InvalidPenaltyAmount", "Invalid penalty amount"),
            (15, "InvalidRewardAmount", "Invalid reward amount"),
            (16, "InvalidOutageId", "Outage ID rejected by validator"),
            (17, "MalformedSymbolInput", "Symbol contains invalid chars"),
            (18, "InvalidMTTR", "MTTR must be greater than zero"),
            (19, "ThresholdOutOfBounds", "Threshold out of bounds"),
            (20, "PenaltyOutOfBounds", "Penalty out of bounds"),
            (21, "RewardOutOfBounds", "Reward out of bounds"),
            (22, "InvalidMonth", "Month out of range"),
            (23, "InvalidTimestampSequence", "Timestamps out of order"),
            (24, "DivisionByZero", "Division by zero attempted"),
        ];

        for (code, label, description) in entries {
            codes.push_back(FailureCode {
                code,
                label: Symbol::new(&env, label),
                description: Symbol::new(&env, description),
            });
        }

        Ok(FailureSchema {
            version: symbol_short!("v1"),
            codes,
        })
    }

    pub fn get_result_schema(env: Env) -> Result<SLAResultSchema, SLAError> {
        Self::check_version(&env)?;
        Ok(SLAResultSchema {
            version: symbol_short!("v1"),
            schema_version: RESULT_SCHEMA_VERSION,
            status_met: symbol_short!("met"),
            status_violated: symbol_short!("viol"),
            payment_reward: symbol_short!("rew"),
            payment_penalty: symbol_short!("pen"),
            rating_exceptional: symbol_short!("top"),
            rating_excellent: symbol_short!("excel"),
            rating_good: symbol_short!("good"),
            rating_poor: symbol_short!("poor"),
            includes_config_version_hash: true,
        })
    }

    /// #60 – Returns static contract capabilities for backend introspection.
    pub fn get_contract_metadata(env: Env) -> Result<ContractMetadata, SLAError> {
        Self::check_version(&env)?;
        let severities = Self::canonical_severities(&env);

        let mut features = Vec::new(&env);
        features.push_back(symbol_short!("calc"));
        features.push_back(symbol_short!("audit"));
        features.push_back(symbol_short!("pause"));
        features.push_back(symbol_short!("stats"));
        features.push_back(symbol_short!("history"));
        features.push_back(symbol_short!("failcode"));
        features.push_back(symbol_short!("safe_call"));
        features.push_back(symbol_short!("ver_nego"));
        features.push_back(symbol_short!("corr_id"));

        Ok(ContractMetadata {
            contract_name: symbol_short!("sla_calc"),
            storage_version: STORAGE_VERSION,
            result_schema_version: RESULT_SCHEMA_VERSION,
            supported_severities: severities,
            features,
        })
    }

    // -------------------------------------------------------------------
    // #29 – Stats view
    // -------------------------------------------------------------------

    /// Returns the cumulative SLA performance statistics.
    pub fn get_stats(env: Env) -> Result<SLAStats, SLAError> {
        Self::check_version(&env)?;
        env.storage()
            .instance()
            .get(&STATS_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    // -------------------------------------------------------------------
    // #31 - SLA Audit Mode (View-only calculation)
    // -------------------------------------------------------------------

    /// Recalculates SLA deterministically without mutating any state or emitting events.
    /// Can be called by anyone for verification and audit purposes.
    /// Simulates SLA calculation for hypothetical outage data without modifying any persistent
    /// storage or emitting any events. This is a pure read-only function that can be called
    /// safely and repeatedly without side effects. It returns only the computational results
    /// of the SLA calculation.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `outage` - The outage input data containing outage ID, severity, and MTTR minutes
    ///
    /// # Returns
    /// * `SlaResult` - Contains breach status, penalty amount, uptime basis points, and the
    ///   applied severity tier
    pub fn simulate_sla(env: Env, outage: OutageInput) -> Result<SlaSimulationResult, SLAError> {
        Self::check_version(&env)?;
        // Validate inputs just like in production calculations
        Self::validate_symbol_input(&env, &outage.outage_id, true)?;
        Self::validate_symbol_input(&env, &outage.severity, false)?;
        // We bypass pause and operator checks to allow public simulation
        let cfg = Self::load_config(&env, &outage.severity)?;
        // Delegate to pure core calculation logic - no storage writes, no events emitted
        Ok(Self::compute_sla(&env, &cfg, &outage))
    }

    pub fn calculate_sla_view(
        env: Env,
        outage_id: Symbol,
        severity: Symbol,
        mttr_minutes: u32,
    ) -> Result<SLAResult, SLAError> {
        Self::check_version(&env)?;
        // Graceful degradation: validate inputs before processing
        Self::validate_symbol_input(&env, &outage_id, true)?;
        Self::validate_symbol_input(&env, &severity, false)?;
        // We bypass pause and operator checks to allow continuous, public verification
        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        let cfg = configs
            .get(severity.clone())
            .ok_or(SLAError::ConfigNotFound)?;
        let config_version_hash = Self::compute_config_version_hash(&env, &configs)?;

        let max_duration = env
            .storage()
            .instance()
            .get(&MAX_OUTAGE_DURATION_KEY)
            .unwrap_or(DEFAULT_MAX_OUTAGE_DURATION_MINUTES);
        let effective_mttr = if mttr_minutes > max_duration {
            max_duration
        } else {
            mttr_minutes
        };

        // Delegate to pure internal math without mutating state or emitting events.

        // Use the current ledger timestamp so the view result matches the mutating
        // path for the same inputs executed in the same ledger, while still avoiding
        // any state writes or event emission.
        Self::compute_result(
            outage_id,
            effective_mttr,
            &cfg,
            config_version_hash,
            env.ledger().timestamp(),
        )
    }

    /// Exposes a monthly SLA compliance percentage calculator view endpoint.
    /// Takes year and month (1-12) and returns compliance in basis points (10000 = 100%).
    pub fn calculate_monthly_sla_compliance(
        env: Env,
        year: u32,
        month: u32,
    ) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        if !(1..=12).contains(&month) {
            return Err(SLAError::InvalidMonth);
        }

        let mut days = 0;
        for y in 1970..year {
            days += if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
                366
            } else {
                365
            };
        }
        for m in 1..month {
            let m_days = match m {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 => {
                    if year.is_multiple_of(4)
                        && (!year.is_multiple_of(100) || year.is_multiple_of(400))
                    {
                        29
                    } else {
                        28
                    }
                }
                _ => 0,
            };
            days += m_days;
        }

        let start_timestamp = (days as u64) * 86400;

        let m_days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
                {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        };
        let end_timestamp = start_timestamp + (m_days as u64) * 86400;

        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));

        let mut calculation_count = 0;
        let mut violations = 0;

        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            if entry.recorded_at >= start_timestamp && entry.recorded_at < end_timestamp {
                calculation_count += 1;
                if entry.status == symbol_short!("viol") {
                    violations += 1;
                }
            }
        }

        let compliance_rate_bps = if calculation_count > 0 {
            ((calculation_count - violations) as u64 * 10000 / calculation_count as u64) as u32
        } else {
            10000 // 100% compliance if no incidents
        };

        Ok(compliance_rate_bps)
    }

    /// Recalculates SLA for multiple outages in a single transaction without mutating any state.
    /// Can be called by anyone for verification and audit purposes.
    pub fn batch_calculate_view(
        env: Env,
        requests: soroban_sdk::Vec<crate::batch::BatchRequest>,
    ) -> Result<
        (
            crate::batch::BatchSummary,
            soroban_sdk::Vec<crate::batch::BatchResult>,
        ),
        SLAError,
    > {
        Self::check_version(&env)?;
        // Validate batch inputs before processing
        crate::batch::validate_batch(&env, &requests)?;

        // Bypass all authorization and pause checks to allow public verification
        let configs: soroban_sdk::Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;

        let mut results = soroban_sdk::Vec::new(&env);
        let mut succeeded: u32 = 0;
        let mut failed: u32 = 0;
        let mut total_rewards: i128 = 0;
        let mut total_penalties: i128 = 0;

        for i in 0..requests.len() {
            let req = requests.get(i).unwrap();

            // Try to calculate (no persistence, pure computation only)
            match crate::batch::process_single(&env, &configs, &req) {
                Ok(result) => {
                    succeeded = succeeded.saturating_add(1);
                    if result.status == symbol_short!("viol") {
                        total_penalties = total_penalties.saturating_add(result.amount);
                    } else {
                        total_rewards = total_rewards.saturating_add(result.amount);
                    }
                    let mut res_vec = Vec::new(&env);
                    res_vec.push_back(result);
                    results.push_back(crate::batch::BatchResult {
                        outage_id: req.outage_id.clone(),
                        success: true,
                        result: res_vec,
                        error: None,
                    });
                }
                Err(e) => {
                    failed = failed.saturating_add(1);
                    let error_msg = match e {
                        SLAError::ConfigNotFound => symbol_short!("no_config"),
                        SLAError::InvalidSeverity => symbol_short!("bad_sev"),
                        SLAError::InvalidThreshold => symbol_short!("bad_thres"),
                        _ => symbol_short!("unknown"),
                    };
                    results.push_back(crate::batch::BatchResult {
                        outage_id: req.outage_id.clone(),
                        success: false,
                        result: Vec::new(&env),
                        error: Some(error_msg),
                    });
                }
            }
        }

        let summary = crate::batch::BatchSummary {
            total: requests.len(),
            succeeded,
            failed,
            total_rewards,
            total_penalties,
        };

        Ok((summary, results))
    }

    /// Calculate SLA for multiple outages in a single transaction (operator only).
    /// Processes each item in the batch sequentially. Failed items do not
    /// halt the batch; they are recorded as failures in the results.
    pub fn batch_calculate(
        env: Env,
        caller: Address,
        requests: soroban_sdk::Vec<crate::batch::BatchRequest>,
    ) -> Result<
        (
            crate::batch::BatchSummary,
            soroban_sdk::Vec<crate::batch::BatchResult>,
        ),
        SLAError,
    > {
        crate::batch::batch_calculate(&env, &caller, requests)
    }

    /// Return aggregated downtime and compliance for up to 50 site IDs.
    ///
    /// This is a read-only batch endpoint; it does not mutate SLA history.
    pub fn get_batch_downtime(
        env: Env,
        site_ids: soroban_sdk::Vec<Symbol>,
    ) -> Result<soroban_sdk::Vec<crate::batch::BatchDowntimeResult>, SLAError> {
        Self::check_version(&env)?;
        crate::batch::get_batch_downtime(&env, &site_ids)
    }

    // -------------------------------------------------------------------
    // SLA calculation (operator only)                                #28
    // -------------------------------------------------------------------

    pub fn calculate_sla(
        env: Env,
        caller: Address, // #28 – operator must identify themselves
        outage_id: Symbol,
        severity: Symbol,
        mttr_minutes: u32,
    ) -> Result<SLAResult, SLAError> {
        Self::check_version(&env)?;
        // Graceful degradation: validate inputs before processing
        Self::validate_symbol_input(&env, &outage_id, true)?;
        Self::validate_symbol_input(&env, &severity, false)?;
        Self::require_not_paused(&env)?; // #27
        Self::require_operator(&env, &caller)?; // #28

        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        let cfg = configs
            .get(severity.clone())
            .ok_or(SLAError::ConfigNotFound)?;
        let config_version_hash = Self::compute_config_version_hash(&env, &configs)?;

        // #671: Enforce maximum single outage duration cap
        let max_duration = env
            .storage()
            .instance()
            .get(&MAX_OUTAGE_DURATION_KEY)
            .unwrap_or(DEFAULT_MAX_OUTAGE_DURATION_MINUTES);

        let effective_mttr = if mttr_minutes > max_duration {
            Self::publish_outage_duration_capped_event(
                &env,
                &caller,
                &outage_id,
                mttr_minutes,
                max_duration,
            );
            max_duration
        } else {
            mttr_minutes
        };

        let result = Self::compute_result(
            outage_id.clone(),
            effective_mttr,
            &cfg,
            config_version_hash,
            env.ledger().timestamp(),
        )?;
        let mut history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));

        let mut existing: Option<SLAResult> = None;
        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            if entry.outage_id == outage_id {
                existing = Some(entry);
            }
        }
        if let Some(prev) = existing {
            // Explicit duplicate policy: same outage_id is idempotent only when
            // execution inputs resolve to the same deterministic result.
            if prev.mttr_minutes != effective_mttr
                || prev.threshold_minutes != cfg.threshold_minutes
            {
                return Err(SLAError::DuplicateOutageInput);
            }
            return Ok(prev);
        }

        history.push_back(result.clone());

        // SC-013: use configurable retention limit (falls back to MAX_HISTORY_SIZE)
        let retention_limit: u32 = env
            .storage()
            .instance()
            .get(&RETENTION_LIMIT_KEY)
            .unwrap_or(MAX_HISTORY_SIZE);

        // SC-062: enforce bounded retention – drop oldest entry when cap is exceeded
        if history.len() > retention_limit {
            let mut trimmed = Vec::new(&env);
            for i in 1..history.len() {
                trimmed.push_back(history.get(i).unwrap());
            }
            env.storage().instance().set(&HISTORY_KEY, &trimmed);
        } else {
            env.storage().instance().set(&HISTORY_KEY, &history);
        }

        // Mutate stats and emit events depending on outcome
        if result.status == symbol_short!("viol") {
            // #29 – update stats (pass positive penalty value)
            Self::increment_stats(&env, false, 0, -result.amount);
            Self::publish_sla_violated_event(&env, &result);
        } else {
            // #29 – update stats
            Self::increment_stats(&env, true, result.amount, 0);
            Self::publish_sla_met_event(&env, &result);
        }

        Self::publish_sla_event(&env, severity.clone(), &result);
        Self::publish_settlement_intent_event(&env, severity, &result);

        Ok(result)
    }

    // -------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------

    /// Pure helper to generate the SLAResult deterministically.
    /// `config_version_hash` binds the result to the exact config snapshot used
    /// during evaluation. `recorded_at` is the ledger timestamp at call time
    /// (0 in view/audit mode).
    /// Pure core calculation logic used by both calculate_sla and simulate_sla
    /// Extracts the essential SLA computation without any side effects
    fn compute_sla(env: &Env, config: &SLAConfig, outage: &OutageInput) -> SlaSimulationResult {
        let max_duration = env
            .storage()
            .instance()
            .get(&MAX_OUTAGE_DURATION_KEY)
            .unwrap_or(DEFAULT_MAX_OUTAGE_DURATION_MINUTES);
        let mttr_minutes = if outage.mttr_minutes > max_duration {
            max_duration
        } else {
            outage.mttr_minutes
        };
        let threshold = config.threshold_minutes;

        let is_breach = mttr_minutes > threshold;
        let penalty_amount = if is_breach {
            let overtime = (mttr_minutes - threshold) as i128;
            overtime.saturating_mul(config.penalty_per_minute)
        } else {
            0
        };

        // Calculate uptime basis points (10000 bps = 100%) - (mttr / threshold) as percentage in bps
        let uptime_bps = (mttr_minutes as i128)
            .saturating_mul(10000)
            .div_euclid(threshold as i128) as u32;

        // Map severity to the applied tier
        let applied_tier = Some(outage.severity.clone());

        SlaSimulationResult {
            is_breach,
            penalty_amount,
            uptime_bps,
            applied_tier,
        }
    }

    fn compute_result(
        outage_id: Symbol,
        mttr_minutes: u32,
        cfg: &SLAConfig,
        config_version_hash: soroban_sdk::BytesN<32>,
        recorded_at: u64,
    ) -> Result<SLAResult, SLAError> {
        let threshold = cfg.threshold_minutes;

        if mttr_minutes == 0 {
            return Ok(SLAResult {
                outage_id,
                status: symbol_short!("met"),
                mttr_minutes: 0,
                threshold_minutes: threshold,
                amount: 0,
                payment_type: symbol_short!("rew"),
                rating: symbol_short!("top"),
                config_version_hash,
                recorded_at,
            });
        }

        // Case 1: SLA violated → penalty
        if mttr_minutes > threshold {
            let overtime = (mttr_minutes - threshold) as i128;
            let penalty = overtime.saturating_mul(cfg.penalty_per_minute);
            let amount = -penalty;
            if amount >= 0 {
                return Err(SLAError::InvalidPenaltyAmount);
            }

            Ok(SLAResult {
                outage_id,
                status: symbol_short!("viol"),
                mttr_minutes,
                threshold_minutes: threshold,
                amount,
                payment_type: symbol_short!("pen"),
                rating: symbol_short!("poor"),
                config_version_hash,
                recorded_at,
            })
        } else {
            // Case 2: SLA met → reward
            let performance_ratio = (mttr_minutes as i128)
                .saturating_mul(100)
                .div_euclid(threshold as i128);

            let (multiplier, rating) = if performance_ratio < 50 {
                (cfg.top_tier_multiplier, symbol_short!("top"))
            } else if performance_ratio < 75 {
                (cfg.excel_tier_multiplier, symbol_short!("excel"))
            } else {
                (cfg.good_tier_multiplier, symbol_short!("good"))
            };

            let reward = cfg
                .reward_base
                .saturating_mul(multiplier as i128)
                .div_euclid(100);

            Ok(SLAResult {
                outage_id,
                status: symbol_short!("met"),
                mttr_minutes,
                threshold_minutes: threshold,
                amount: reward,
                payment_type: symbol_short!("rew"),
                rating,
                config_version_hash,
                recorded_at,
            })
        }
    }

    fn write_version(env: &Env) {
        env.storage()
            .instance()
            .set(&STORAGE_VERSION_KEY, &STORAGE_VERSION);
    }

    fn check_version(env: &Env) -> Result<(), SLAError> {
        let v: u32 = env
            .storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .ok_or(SLAError::NotInitialized)?;
        if v != STORAGE_VERSION {
            return Err(SLAError::VersionMismatch);
        }
        Ok(())
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), SLAError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(SLAError::NotInitialized)?;
        if caller != &admin {
            return Err(SLAError::Unauthorized);
        }
        Ok(())
    }

    /// #28 – Ensures the caller holds the operator role.
    fn require_operator(env: &Env, caller: &Address) -> Result<(), SLAError> {
        let operator: Address = env
            .storage()
            .instance()
            .get(&OPERATOR_KEY)
            .ok_or(SLAError::NotInitialized)?;
        if caller != &operator {
            return Err(SLAError::Unauthorized);
        }
        Ok(())
    }

    /// #551 – Populate sensible default SLA configurations for all severity tiers.
    ///
    /// Defaults:
    /// - critical: threshold=15m, penalty=100, reward=750
    /// - high:     threshold=30m, penalty=50,  reward=500
    /// - medium:   threshold=60m, penalty=20,  reward=250
    /// - low:      threshold=120m, penalty=5,   reward=100
    fn initialize_default_configs(env: &Env) -> Map<Symbol, SLAConfig> {
        let mut configs = Map::<Symbol, SLAConfig>::new(env);
        configs.set(
            symbol_short!("critical"),
            SLAConfig {
                threshold_minutes: 15,
                penalty_per_minute: 100,
                reward_base: 750,
                top_tier_multiplier: 200,
                excel_tier_multiplier: 150,
                good_tier_multiplier: 100,
            },
        );
        configs.set(
            symbol_short!("high"),
            SLAConfig {
                threshold_minutes: 30,
                penalty_per_minute: 50,
                reward_base: 500,
                top_tier_multiplier: 200,
                excel_tier_multiplier: 150,
                good_tier_multiplier: 100,
            },
        );
        configs.set(
            symbol_short!("medium"),
            SLAConfig {
                threshold_minutes: 60,
                penalty_per_minute: 20,
                reward_base: 250,
                top_tier_multiplier: 200,
                excel_tier_multiplier: 150,
                good_tier_multiplier: 100,
            },
        );
        configs.set(
            symbol_short!("low"),
            SLAConfig {
                threshold_minutes: 120,
                penalty_per_minute: 5,
                reward_base: 100,
                top_tier_multiplier: 200,
                excel_tier_multiplier: 150,
                good_tier_multiplier: 100,
            },
        );
        configs
    }

    /// Validates symbol inputs used in outage / severity fields.
    ///
    /// - `_env` is accepted for API consistency with other validation helpers
    ///   (and future Env-backed checks).
    /// - When `is_outage_id` is false the symbol must match a canonical severity
    ///   (`critical`, `high`, `medium`, `low`); otherwise `InvalidSeverity` is returned.
    /// - Outage IDs rely on Soroban's native Symbol constraints (non-empty, ≤32, charset).
    fn validate_symbol_input(
        _env: &Env,
        symbol: &Symbol,
        is_outage_id: bool,
    ) -> Result<(), SLAError> {
        if is_outage_id {
            // Soroban Symbol is natively restricted to valid characters and max length 32.
            return Ok(());
        }

        // Severity path: must match canonical severity tiers.
        if !Self::is_canonical_severity(symbol) {
            return Err(SLAError::InvalidSeverity);
        }
        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), SLAError> {
        let paused: bool = env.storage().instance().get(&PAUSED_KEY).unwrap_or(false);
        if paused {
            return Err(SLAError::ContractPaused);
        }
        Ok(())
    }

    /// #70 – Validates configuration parameters to ensure safe and meaningful values.
    fn validate_config(
        env: &Env,
        severity: &Symbol,
        threshold_minutes: u32,
        penalty_per_minute: i128,
        reward_base: i128,
        existing_configs: &Map<Symbol, SLAConfig>,
    ) -> Result<(), SLAError> {
        // Validate severity is one of the supported values
        if !Self::is_canonical_severity(severity) {
            return Err(SLAError::InvalidSeverity);
        }

        // Threshold must be between 1 and 1440 minutes (24 hours max)
        if threshold_minutes == 0 || threshold_minutes > 1440 {
            return Err(SLAError::ThresholdOutOfBounds);
        }

        // Penalty must be positive and reasonable (1 to 10000 per minute)
        if penalty_per_minute <= 0 || penalty_per_minute > 10000 {
            return Err(SLAError::PenaltyOutOfBounds);
        }

        // Reward base must be positive and reasonable (1 to 100000)
        if reward_base <= 0 || reward_base > 100000 {
            return Err(SLAError::RewardOutOfBounds);
        }

        // Severity-specific validation to ensure logical consistency
        if *severity == symbol_short!("critical") {
            // Critical should have shortest thresholds and highest penalties
            if threshold_minutes > 60 {
                return Err(SLAError::InvalidThreshold);
            }
            if penalty_per_minute < 50 {
                return Err(SLAError::InvalidPenalty);
            }
        } else if *severity == symbol_short!("high") {
            // High severity thresholds should be reasonable
            if threshold_minutes > 120 {
                return Err(SLAError::InvalidThreshold);
            }
            if penalty_per_minute < 25 {
                return Err(SLAError::InvalidPenalty);
            }
        } else if *severity == symbol_short!("medium") {
            // Medium severity thresholds
            if threshold_minutes > 240 {
                return Err(SLAError::InvalidThreshold);
            }
            if penalty_per_minute < 10 {
                return Err(SLAError::InvalidPenalty);
            }
        } else if *severity == symbol_short!("low") {
            // Low severity can have longer thresholds but lower penalties
            if penalty_per_minute > 100 {
                return Err(SLAError::InvalidPenalty);
            }
        } else {
            return Err(SLAError::InvalidSeverity);
        }

        // Cross-severity monotonicity validation
        let severities = Self::canonical_severities(env); // order: critical, high, medium, low

        // Get the index of the severity being updated
        let current_index = Self::canonical_severity_index(severity).unwrap();

        // Validate against all other severities to maintain proper ordering
        for (i, other_severity) in severities.iter().enumerate() {
            if other_severity == *severity {
                continue;
            }

            let other_config = existing_configs.get(other_severity).unwrap();

            // For any severity that comes before the current one (lower index = higher severity)
            if (i as u32) < current_index {
                // Higher severity should have lower threshold than current
                if threshold_minutes <= other_config.threshold_minutes {
                    return Err(SLAError::InvalidThreshold);
                }
                // Higher severity should have higher penalty than current
                if penalty_per_minute >= other_config.penalty_per_minute {
                    return Err(SLAError::InvalidPenalty);
                }
            } else {
                // Lower severity should have higher threshold than current
                if threshold_minutes >= other_config.threshold_minutes {
                    return Err(SLAError::InvalidThreshold);
                }
                // Lower severity should have lower penalty than current
                if penalty_per_minute <= other_config.penalty_per_minute {
                    return Err(SLAError::InvalidPenalty);
                }
            }
        }

        Ok(())
    }

    fn canonical_severities(env: &Env) -> Vec<Symbol> {
        let mut severities = Vec::new(env);
        severities.push_back(symbol_short!("critical"));
        severities.push_back(symbol_short!("high"));
        severities.push_back(symbol_short!("medium"));
        severities.push_back(symbol_short!("low"));
        severities
    }

    fn canonical_severity_index(severity: &Symbol) -> Option<u32> {
        if *severity == symbol_short!("critical") {
            Some(0)
        } else if *severity == symbol_short!("high") {
            Some(1)
        } else if *severity == symbol_short!("medium") {
            Some(2)
        } else if *severity == symbol_short!("low") {
            Some(3)
        } else {
            None
        }
    }

    fn is_canonical_severity(severity: &Symbol) -> bool {
        Self::canonical_severity_index(severity).is_some()
    }

    /// Return the canonical 32-byte config hash bound to persisted SLA results.
    fn compute_config_version_hash(
        env: &Env,
        configs: &Map<Symbol, SLAConfig>,
    ) -> Result<soroban_sdk::BytesN<32>, SLAError> {
        Ok(crate::compute_config_version_hash(env, configs))
    }

    /// Return the legacy compact u64 config hash used by backend snapshot APIs.
    fn compute_config_version_hash_u64(
        _env: &Env,
        configs: &Map<Symbol, SLAConfig>,
    ) -> Result<u64, SLAError> {
        let severities = [
            symbol_short!("critical"),
            symbol_short!("high"),
            symbol_short!("medium"),
            symbol_short!("low"),
        ];

        const BASE: u64 = 91138233;
        const MODULUS: u64 = (1u64 << 63) - 25;

        let mut hash: u64 = 1;
        let mut power: u64 = 1;

        for sev in severities {
            let cfg = configs.get(sev.clone()).ok_or(SLAError::ConfigNotFound)?;

            hash = hash
                .wrapping_mul(BASE)
                .wrapping_add(cfg.threshold_minutes as u64)
                .wrapping_mul(power)
                % MODULUS;
            power = power.wrapping_mul(BASE) % MODULUS;

            hash = hash
                .wrapping_mul(BASE)
                .wrapping_add(cfg.penalty_per_minute as u64)
                .wrapping_mul(power)
                % MODULUS;
            power = power.wrapping_mul(BASE) % MODULUS;

            hash = hash
                .wrapping_mul(BASE)
                .wrapping_add(cfg.reward_base as u64)
                .wrapping_mul(power)
                % MODULUS;
            power = power.wrapping_mul(BASE) % MODULUS;
        }

        Ok(hash.wrapping_mul(BASE).wrapping_add(0x9e3779b97f4a7c15u64) % MODULUS)
    }

    #[allow(unused_assignments)]
    fn compute_result_hash_inner(result: &SLAResult) -> u64 {
        const BASE: u64 = 91138233;
        const MODULUS: u64 = (1u64 << 63) - 25;

        let mut hash: u64 = 1;
        let mut power: u64 = 1;

        macro_rules! mix {
            ($v:expr) => {
                hash = hash
                    .wrapping_mul(BASE)
                    .wrapping_add($v as u64)
                    .wrapping_mul(power)
                    % MODULUS;
                power = power.wrapping_mul(BASE) % MODULUS;
            };
        }

        mix!(result.outage_id.to_val().get_payload());
        mix!(result.status.to_val().get_payload());
        mix!(result.mttr_minutes);
        mix!(result.threshold_minutes);
        mix!(result.amount as u64);
        mix!((result.amount >> 64) as u64);
        mix!(result.payment_type.to_val().get_payload());
        mix!(result.rating.to_val().get_payload());
        for byte in result.config_version_hash.to_array().iter() {
            mix!(*byte);
        }
        mix!(result.recorded_at);

        hash.wrapping_mul(BASE).wrapping_add(0x9e3779b97f4a7c15u64) % MODULUS
    }

    /// Optimised config lookup with severity index pre-check for early rejection.
    /// Skips the storage read when the severity is not one of the four canonical
    /// values, avoiding an unnecessary Map deserialisation for invalid inputs.
    fn load_config(env: &Env, severity: &Symbol) -> Result<SLAConfig, SLAError> {
        // Early rejection for non-canonical severities — avoids storage read.
        if !Self::is_canonical_severity(severity) {
            return Err(SLAError::ConfigNotFound);
        }
        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        configs
            .get(severity.clone())
            .ok_or(SLAError::ConfigNotFound)
    }

    /// #29 – Read-modify-write the stats entry.
    /// `met`     – true when SLA was met (reward path), false for violation.
    /// `reward`  – reward amount to add (0 on violation path).
    /// `penalty` – penalty amount to add, stored positive (0 on met path).
    fn increment_stats(env: &Env, met: bool, reward: i128, penalty: i128) {
        let mut stats: SLAStats = env
            .storage()
            .instance()
            .get(&STATS_KEY)
            .unwrap_or(SLAStats {
                total_calculations: 0,
                total_violations: 0,
                total_rewards: 0,
                total_penalties: 0,
            });

        stats.total_calculations = stats.total_calculations.saturating_add(1);

        if met {
            stats.total_rewards = stats.total_rewards.saturating_add(reward);
        } else {
            stats.total_violations = stats.total_violations.saturating_add(1);
            stats.total_penalties = stats.total_penalties.saturating_add(penalty);
        }

        env.storage().instance().set(&STATS_KEY, &stats);
    }

    fn publish_sla_event(env: &Env, severity: Symbol, result: &SLAResult) {
        env.events().publish(
            (EVENT_SLA_CALC, EVENT_VERSION, severity),
            (
                result.outage_id.clone(),
                result.status.clone(),
                result.payment_type.clone(),
                result.rating.clone(),
                result.mttr_minutes,
                result.threshold_minutes,
                result.amount,
            ),
        );
    }

    fn publish_settlement_intent_event(env: &Env, severity: Symbol, result: &SLAResult) {
        env.events().publish(
            (EVENT_SETTLE_INTENT, EVENT_VERSION, severity),
            (
                result.outage_id.clone(),
                result.status.clone(),
                result.payment_type.clone(),
                result.amount,
                result.config_version_hash.clone(),
                result.recorded_at,
            ),
        );
    }

    fn publish_sla_violated_event(env: &Env, result: &SLAResult) {
        env.events().publish(
            (EVENT_SLA_VIOLATED, EVENT_VERSION, result.outage_id.clone()),
            (result.mttr_minutes, result.threshold_minutes, result.amount),
        );
    }

    fn publish_sla_met_event(env: &Env, result: &SLAResult) {
        env.events().publish(
            (EVENT_SLA_MET, EVENT_VERSION, result.outage_id.clone()),
            (result.mttr_minutes, result.threshold_minutes, result.amount),
        );
    }

    fn publish_outage_duration_capped_event(
        env: &Env,
        caller: &Address,
        outage_id: &Symbol,
        original_duration: u32,
        capped_duration: u32,
    ) {
        env.events().publish(
            (
                Symbol::new(env, "OutageDurationCapped"),
                EVENT_VERSION,
                caller.clone(),
            ),
            (outage_id.clone(), original_duration, capped_duration),
        );
    }

    // -------------------------------------------------------------------
    // #576 – Role audit trail event logger
    // -------------------------------------------------------------------

    fn publish_role_audit_event(
        env: &Env,
        caller: &Address,
        role: Symbol,
        prev_address: Option<&Address>,
        new_address: Option<&Address>,
    ) {
        let prev_present = prev_address.is_some();
        let new_present = new_address.is_some();
        env.events().publish(
            (EVENT_ROLE_AUDIT, EVENT_VERSION, caller.clone()),
            (role, prev_present, new_present),
        );
    }

    // -------------------------------------------------------------------
    // #33 - History & Compaction (Admin only)
    // -------------------------------------------------------------------

    pub fn get_initialization_status(env: Env) -> InitializationStatus {
        let is_initialized = env.storage().instance().has(&ADMIN_KEY);
        let initialized_at: u64 = env.storage().instance().get(&INIT_TIME_KEY).unwrap_or(0);
        InitializationStatus {
            is_initialized,
            initialized_at,
        }
    }

    pub fn get_history_summary(env: Env) -> Result<HistorySummary, SLAError> {
        Self::check_version(&env)?;
        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));

        let mut total = 0;
        let mut met = 0;
        let mut violated = 0;

        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            total += 1;
            if entry.status == symbol_short!("met") {
                met += 1;
            } else if entry.status == symbol_short!("viol") {
                violated += 1;
            }
        }

        Ok(HistorySummary {
            total,
            met,
            violated,
        })
    }

    /// Returns the raw log of recent SLA calculations stored on-chain.
    pub fn get_history(env: Env) -> Result<Vec<SLAResult>, SLAError> {
        Self::check_version(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env)))
    }

    /// Prunes the SLA calculation history to prevent indefinite storage growth.
    /// `keep_latest` dictates how many of the most recent records to retain.
    pub fn prune_history(env: Env, caller: Address, keep_latest: u32) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        let len = history.len();

        if len > keep_latest {
            let remove_count = len - keep_latest;
            let mut new_history = Vec::new(&env);

            // Rebuild the vector keeping only the most recent entries
            for i in remove_count..len {
                new_history.push_back(history.get(i).unwrap());
            }

            env.storage().instance().set(&HISTORY_KEY, &new_history);
            env.events().publish(
                (EVENT_PRUNED, EVENT_VERSION, caller),
                (remove_count, keep_latest),
            );
        }

        Ok(())
    }

    /// SC-063 – Prune history entries older than `min_age_seconds` before the
    /// current ledger timestamp.  Entries with `recorded_at == 0` (view-mode
    /// results that were never stored with a real timestamp) are always kept.
    /// Admin-only.  Emits a `pruned_a` event.
    pub fn prune_history_by_age(
        env: Env,
        caller: Address,
        min_age_seconds: u64,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let now = env.ledger().timestamp();
        let cutoff = now.saturating_sub(min_age_seconds);

        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));

        let mut new_history = Vec::new(&env);
        let mut removed: u32 = 0;

        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            // Keep entries that are recent enough
            if entry.recorded_at >= cutoff {
                new_history.push_back(entry);
            } else {
                removed += 1;
            }
        }

        if removed > 0 {
            let kept = new_history.len();
            env.storage().instance().set(&HISTORY_KEY, &new_history);
            env.events()
                .publish((EVENT_PRUNED_AGE, EVENT_VERSION, caller), (removed, kept));
        }

        Ok(())
    }

    // -------------------------------------------------------------------
    // SC-059: History pagination
    // -------------------------------------------------------------------

    /// Returns a bounded page of history entries.
    /// `offset` is zero-based; entries are ordered oldest-first (insertion order).
    /// Returns an empty Vec when `offset` is beyond the end of history.
    pub fn get_history_page(env: Env, offset: u32, limit: u32) -> Result<Vec<SLAResult>, SLAError> {
        Self::check_version(&env)?;
        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        let len = history.len();
        let mut page = Vec::new(&env);
        if offset >= len || limit == 0 {
            return Ok(page);
        }
        let end = (offset + limit).min(len);
        for i in offset..end {
            page.push_back(history.get(i).unwrap());
        }
        Ok(page)
    }

    // -------------------------------------------------------------------
    // SC-060: History query by outage identifier
    // -------------------------------------------------------------------

    /// Returns all history entries whose `outage_id` matches the given value.
    /// Returns an empty Vec when no matching entries exist.
    pub fn get_history_by_outage(env: Env, outage_id: Symbol) -> Result<Vec<SLAResult>, SLAError> {
        Self::check_version(&env)?;
        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        let mut matches = Vec::new(&env);
        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            if entry.outage_id == outage_id {
                matches.push_back(entry);
            }
        }
        Ok(matches)
    }

    // -------------------------------------------------------------------
    // SC-061: Latest result by outage identifier
    // -------------------------------------------------------------------

    /// Returns the most recent history entry for the given `outage_id`, or `None`
    /// if no entry exists for that outage.
    pub fn get_latest_by_outage(
        env: Env,
        outage_id: Symbol,
    ) -> Result<Option<SLAResult>, SLAError> {
        Self::check_version(&env)?;
        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));
        let mut latest: Option<SLAResult> = None;
        for i in 0..history.len() {
            let entry = history.get(i).unwrap();
            if entry.outage_id == outage_id {
                latest = Some(entry);
            }
        }
        Ok(latest)
    }

    // -------------------------------------------------------------------
    // SC-079: Read-only history / retention helpers
    // -------------------------------------------------------------------

    /// Returns the number of severity tiers currently configured.
    /// Off-chain consumers can inspect retention state without fetching the full map.
    pub fn get_config_count(env: Env) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        let configs: Map<Symbol, SLAConfig> = env
            .storage()
            .instance()
            .get(&CONFIG_KEY)
            .ok_or(SLAError::NotInitialized)?;
        Ok(configs.len())
    }

    /// Returns the current storage schema version so off-chain consumers can
    /// detect whether a migration has occurred.
    pub fn get_storage_version(env: Env) -> Result<u32, SLAError> {
        env.storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .ok_or(SLAError::NotInitialized)
    }

    // -------------------------------------------------------------------
    // SC-013 – Configurable retention limit (admin only)
    // -------------------------------------------------------------------

    /// Set the maximum number of history entries to retain (#579).
    /// Must be between 50 and 5,000. Admin only.
    /// The new limit takes effect on the next `calculate_sla` call.
    pub fn set_retention_limit(env: Env, caller: Address, limit: u32) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        if !(50..=5000).contains(&limit) {
            return Err(SLAError::RetentionLimitOutOfRange);
        }
        env.storage().instance().set(&RETENTION_LIMIT_KEY, &limit);
        Ok(())
    }

    /// Returns the current configurable retention limit.
    /// Defaults to MAX_HISTORY_SIZE (1000) if never explicitly set.
    pub fn get_retention_limit(env: Env) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&RETENTION_LIMIT_KEY)
            .unwrap_or(MAX_HISTORY_SIZE))
    }

    /// Enforce a maximum single outage duration cap (#671).
    /// Must be at least 1 minute. Admin only.
    pub fn set_max_outage_duration(
        env: Env,
        caller: Address,
        limit_minutes: u32,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;
        if limit_minutes == 0 {
            return Err(SLAError::InvalidMTTR);
        }
        env.storage()
            .instance()
            .set(&MAX_OUTAGE_DURATION_KEY, &limit_minutes);
        Ok(())
    }

    /// Returns the current maximum outage duration cap in minutes (#671).
    /// Defaults to DEFAULT_MAX_OUTAGE_DURATION_MINUTES (10,080 = 7 days) if never explicitly set.
    pub fn get_max_outage_duration(env: Env) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        Ok(env
            .storage()
            .instance()
            .get(&MAX_OUTAGE_DURATION_KEY)
            .unwrap_or(DEFAULT_MAX_OUTAGE_DURATION_MINUTES))
    }

    /// SC-021 – Migration state read helper
    // -------------------------------------------------------------------
    /// Returns the storage version and migration posture.
    ///
    /// Backend consumers should call this after any contract upgrade to confirm
    /// the storage version matches expectations. If `needs_migration` is true,
    /// the admin must call `migrate` before versioned endpoints will respond.
    ///
    /// This function intentionally bypasses `check_version` so it remains
    /// callable even when the contract is in a pre-migration state.
    pub fn get_migration_state(env: Env) -> Result<StorageVersionInfo, SLAError> {
        let stored_version: u32 = env
            .storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .ok_or(SLAError::NotInitialized)?;
        Ok(StorageVersionInfo {
            stored_version,
            expected_version: STORAGE_VERSION,
            needs_migration: stored_version != STORAGE_VERSION,
        })
    }

    // -------------------------------------------------------------------
    // SC-W5-029 – Version negotiation endpoint for backend handshake
    // -------------------------------------------------------------------

    /// Returns a combined version negotiation snapshot for backend startup.
    ///
    /// Intentionally bypasses `check_version` so it remains callable even when
    /// the contract is in a pre-migration state — backends must be able to read
    /// this before deciding whether to call `migrate`.
    pub fn get_version_info(env: Env) -> Result<VersionInfo, SLAError> {
        let stored_version: u32 = env
            .storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .ok_or(SLAError::NotInitialized)?;
        let is_paused: bool = env.storage().instance().get(&PAUSED_KEY).unwrap_or(false);
        Ok(VersionInfo {
            storage_version: stored_version,
            result_schema_version: RESULT_SCHEMA_VERSION,
            needs_migration: stored_version != STORAGE_VERSION,
            is_paused,
            contract_name: symbol_short!("sla_calc"),
        })
    }

    // -------------------------------------------------------------------
    // #577 – Emergency contract migration key
    // -------------------------------------------------------------------

    /// Set an emergency migration key for contract upgrades (#577).
    /// Enforces a 14-day timelock before the key becomes active.
    /// Admin only. Emits a `migr_set` event.
    pub fn set_migration_key(
        env: Env,
        caller: Address,
        migration_address: Address,
    ) -> Result<(), SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let now = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&MIGRATION_KEY, &migration_address);
        env.storage().instance().set(&MIGRATION_TIME_KEY, &now);

        env.events().publish(
            (EVENT_MIGRATION_SET, EVENT_VERSION, caller),
            (migration_address,),
        );

        Ok(())
    }

    /// Execute emergency migration using the migration key (#577).
    /// Only callable after the 14-day timelock has elapsed.
    /// Emits a `migr_act` event on success.
    pub fn execute_migration(env: Env, caller: Address) -> Result<(), SLAError> {
        Self::check_version(&env)?;

        let migration_addr: Address = env
            .storage()
            .instance()
            .get(&MIGRATION_KEY)
            .ok_or(SLAError::NoPendingTransfer)?;

        if caller != migration_addr {
            return Err(SLAError::Unauthorized);
        }

        let set_time: u64 = env
            .storage()
            .instance()
            .get(&MIGRATION_TIME_KEY)
            .ok_or(SLAError::NotInitialized)?;

        let now = env.ledger().timestamp();
        if now.saturating_sub(set_time) < MIGRATION_TIMELOCK {
            return Err(SLAError::ThresholdOutOfBounds);
        }

        // Bump storage version to trigger migration path
        let current_version: u32 = env
            .storage()
            .instance()
            .get(&STORAGE_VERSION_KEY)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&STORAGE_VERSION_KEY, &(current_version + 1));

        // Clean up migration state
        env.storage().instance().remove(&MIGRATION_KEY);
        env.storage().instance().remove(&MIGRATION_TIME_KEY);

        env.events()
            .publish((EVENT_MIGRATION_ACT, EVENT_VERSION, caller), ());

        Ok(())
    }

    // -------------------------------------------------------------------
    // #578 – History pruning queue
    // -------------------------------------------------------------------

    /// Automatically prune oldest history entries when count exceeds
    /// the configured retention limit (#578).
    /// Emits `hist_pq` event with count of pruned entries.
    pub fn prune_history_queue(env: Env, caller: Address) -> Result<u32, SLAError> {
        Self::check_version(&env)?;
        Self::require_admin(&env, &caller)?;

        let retention_limit: u32 = env
            .storage()
            .instance()
            .get(&RETENTION_LIMIT_KEY)
            .unwrap_or(MAX_HISTORY_SIZE);

        let history: Vec<SLAResult> = env
            .storage()
            .instance()
            .get(&HISTORY_KEY)
            .unwrap_or_else(|| Vec::new(&env));

        let len = history.len();
        if len <= retention_limit {
            return Ok(0);
        }

        let remove_count = len - retention_limit;
        let mut new_history = Vec::new(&env);

        for i in remove_count..len {
            new_history.push_back(history.get(i).unwrap());
        }

        env.storage().instance().set(&HISTORY_KEY, &new_history);
        env.events().publish(
            (EVENT_HISTORY_PRUNED_Q, EVENT_VERSION, caller),
            (remove_count, retention_limit),
        );

        Ok(remove_count)
    }
}

/// Computes the integer square root of a 128-bit unsigned integer
/// for quadratic SLA penalty scaling curves.
pub fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }

    let mut low: u128 = 1;
    let mut high: u128 = n / 2 + 1;
    let mut ans: u128 = 0;

    while low <= high {
        let mid = low + (high - low) / 2;
        // Check mid * mid <= n without overflowing by using division
        if mid <= n / mid {
            ans = mid;
            low = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            high = mid - 1;
        }
    }

    ans
}

// -----------------------------------------------------------------------
// #526 – Checked division helper
// -----------------------------------------------------------------------

/// Checked integer division for SLA math.
///
/// Returns `Err(SLAError::InvalidMTTR)` when the divisor is zero instead of
/// panicking, so callers can surface a typed contract error.
pub fn checked_div_sla(a: i128, b: i128) -> Result<i128, SLAError> {
    if b == 0 {
        return Err(SLAError::InvalidMTTR);
    }
    Ok(a / b)
}

// -----------------------------------------------------------------------
// #532 – Multi-site outage penalty discount
// -----------------------------------------------------------------------

/// Discount factor in basis points applied when multiple sites experience
/// simultaneous regional outages.
///
/// | Affected sites | Factor (bps) | Effective charge |
/// |----------------|--------------|------------------|
/// | 0 or 1         | 10_000       | 100% (no discount) |
/// | 2              | 9_000        | 90% |
/// | 3              | 8_000        | 80% |
/// | 4+             | 7_000        | 70% (floor) |
pub fn multi_site_discount_factor_bps(affected_site_count: u32) -> u32 {
    match affected_site_count {
        0 | 1 => 10_000,
        2 => 9_000,
        3 => 8_000,
        _ => 7_000,
    }
}

/// Apply the multi-site discount factor to a base penalty amount.
///
/// `base_penalty` is expected to be non-negative (absolute penalty units).
/// Returns the discounted penalty, floored at zero via saturating math.
pub fn apply_multi_site_discount(base_penalty: i128, affected_site_count: u32) -> i128 {
    if base_penalty <= 0 {
        return 0;
    }
    let factor = multi_site_discount_factor_bps(affected_site_count) as i128;
    // base * factor / 10_000
    base_penalty.saturating_mul(factor).saturating_div(10_000)
}

// -----------------------------------------------------------------------
// #663 – Sub-minute SLA calculation precision helper
// -----------------------------------------------------------------------

/// Whole basis points that represent 100% availability.
pub const BPS_DENOMINATOR: u32 = 10_000;

/// Milliseconds in one second — the fixed-point unit used by
/// [`SubMinutePrecision`].
pub const MILLIS_PER_SECOND: u64 = 1_000;

/// Milliseconds in one minute.
pub const MILLIS_PER_MINUTE: u64 = 60 * MILLIS_PER_SECOND;

/// Scale factor that lifts a basis point to three decimal places
/// (`10_000` bps × `1_000` == 100%).
pub const MILLI_BPS_SCALE: u64 = 1_000;

/// Sub-minute fixed-point helpers for SLA availability math.
///
/// Whole-minute MTTR values truncate fractional seconds, which loses precision
/// when an outage only lasts a few seconds. These helpers keep millisecond
/// precision (three decimal places of a percentage point) while widening every
/// intermediate product to `u128`, so multiplying two large durations can never
/// wrap the accumulator.
pub struct SubMinutePrecision;

impl SubMinutePrecision {
    /// Availability of a fully available window: 100%, in milli-basis-points.
    pub const FULL_AVAILABILITY_MILLI_BPS: u64 = BPS_DENOMINATOR as u64 * MILLI_BPS_SCALE;

    /// Availability in milli-basis-points (`10_000_000` == 100%), i.e. three
    /// decimal places of a percentage point: `99.999%` → `9_999_900`.
    ///
    /// `uptime_millis` is clamped to `window_millis` so availability can never
    /// exceed 100%. Returns [`SLAError::DivisionByZero`] when the observation
    /// window is zero instead of panicking.
    pub fn availability_milli_bps(uptime_millis: u64, window_millis: u64) -> Result<u64, SLAError> {
        if window_millis == 0 {
            return Err(SLAError::DivisionByZero);
        }

        // Widening to u128 before multiplying keeps the intermediate product
        // overflow-free even for durations close to u64::MAX.
        let numerator = (uptime_millis.min(window_millis) as u128)
            * (BPS_DENOMINATOR as u128)
            * (MILLI_BPS_SCALE as u128);

        Ok((numerator / window_millis as u128) as u64)
    }

    /// Availability in milli-basis-points for a window during which
    /// `downtime_millis` were spent unavailable.
    pub fn downtime_availability_milli_bps(
        downtime_millis: u64,
        window_millis: u64,
    ) -> Result<u64, SLAError> {
        let downtime = downtime_millis.min(window_millis);
        Self::availability_milli_bps(window_millis - downtime, window_millis)
    }

    /// Returns `true` when availability meets or exceeds `threshold_milli_bps`.
    ///
    /// Milli-basis-points allow thresholds such as `99.999%` (`9_999_900`) that
    /// cannot be expressed with whole basis points.
    pub fn meets_threshold_milli_bps(
        uptime_millis: u64,
        window_millis: u64,
        threshold_milli_bps: u64,
    ) -> Result<bool, SLAError> {
        Ok(Self::availability_milli_bps(uptime_millis, window_millis)? >= threshold_milli_bps)
    }

    /// Converts whole minutes to milliseconds, saturating at [`u64::MAX`]
    /// rather than overflowing.
    pub fn minutes_to_millis(minutes: u64) -> u64 {
        minutes.saturating_mul(MILLIS_PER_MINUTE)
    }

    /// Converts milliseconds to whole minutes, rounding *up* so a sub-minute
    /// outage is never reported as zero minutes of downtime.
    pub fn millis_to_minutes_ceil(millis: u64) -> u32 {
        u32::try_from(millis.div_ceil(MILLIS_PER_MINUTE)).unwrap_or(u32::MAX)
    }

    /// Truncates a milli-basis-point value to whole basis points.
    ///
    /// Truncation (rather than rounding) keeps availability figures
    /// conservative: `99.999%` (`9_999_900`) reports as `9_999` bps.
    pub fn milli_bps_to_bps(milli_bps: u64) -> u32 {
        (milli_bps / MILLI_BPS_SCALE) as u32
    }
}

#[cfg(test)]
mod math_helpers_tests {
    use super::*;

    // ── #526 ──────────────────────────────────────────────────────────
    #[test]
    fn test_checked_div_sla_happy_path() {
        assert_eq!(checked_div_sla(100, 4).unwrap(), 25);
        assert_eq!(checked_div_sla(-100, 4).unwrap(), -25);
        assert_eq!(checked_div_sla(7, 2).unwrap(), 3);
    }

    #[test]
    fn test_checked_div_sla_division_by_zero() {
        let err = checked_div_sla(42, 0).unwrap_err();
        assert_eq!(err, SLAError::InvalidMTTR);
        let err2 = checked_div_sla(0, 0).unwrap_err();
        assert_eq!(err2, SLAError::InvalidMTTR);
    }

    // ── #532 ──────────────────────────────────────────────────────────
    #[test]
    fn test_multi_site_discount_factor_by_count() {
        assert_eq!(multi_site_discount_factor_bps(0), 10_000);
        assert_eq!(multi_site_discount_factor_bps(1), 10_000);
        assert_eq!(multi_site_discount_factor_bps(2), 9_000);
        assert_eq!(multi_site_discount_factor_bps(3), 8_000);
        assert_eq!(multi_site_discount_factor_bps(4), 7_000);
        assert_eq!(multi_site_discount_factor_bps(10), 7_000);
    }

    #[test]
    fn test_apply_multi_site_discount_math() {
        // Single site: no discount
        assert_eq!(apply_multi_site_discount(1_000, 1), 1_000);
        // Two sites: 90%
        assert_eq!(apply_multi_site_discount(1_000, 2), 900);
        // Three sites: 80%
        assert_eq!(apply_multi_site_discount(1_000, 3), 800);
        // Four+ sites: 70%
        assert_eq!(apply_multi_site_discount(1_000, 5), 700);
        // Zero / negative base → 0
        assert_eq!(apply_multi_site_discount(0, 3), 0);
        assert_eq!(apply_multi_site_discount(-500, 2), 0);
    }
}

#[cfg(test)]
mod sub_minute_precision_tests {
    use super::*;

    const ONE_DAY_MILLIS: u64 = 24 * 60 * 60 * 1_000;

    #[test]
    fn minutes_convert_to_millis_without_overflow() {
        assert_eq!(SubMinutePrecision::minutes_to_millis(0), 0);
        assert_eq!(SubMinutePrecision::minutes_to_millis(1), 60_000);
        assert_eq!(SubMinutePrecision::minutes_to_millis(7), 420_000);
        // Saturates instead of wrapping for an absurd duration.
        assert_eq!(SubMinutePrecision::minutes_to_millis(u64::MAX), u64::MAX);
    }

    #[test]
    fn sub_minute_durations_round_up_to_a_full_minute() {
        assert_eq!(SubMinutePrecision::millis_to_minutes_ceil(0), 0);
        assert_eq!(SubMinutePrecision::millis_to_minutes_ceil(1), 1);
        assert_eq!(SubMinutePrecision::millis_to_minutes_ceil(59_999), 1);
        assert_eq!(SubMinutePrecision::millis_to_minutes_ceil(60_000), 1);
        assert_eq!(SubMinutePrecision::millis_to_minutes_ceil(60_001), 2);
        assert_eq!(
            SubMinutePrecision::millis_to_minutes_ceil(u64::MAX),
            u32::MAX
        );
    }

    #[test]
    fn availability_preserves_three_decimal_places() {
        // 99.999% uptime over one day == 864 ms of downtime.
        let milli_bps =
            SubMinutePrecision::downtime_availability_milli_bps(864, ONE_DAY_MILLIS).unwrap();
        assert_eq!(milli_bps, 9_999_900);
        // Whole-basis-point conversion truncates rather than overstating.
        assert_eq!(SubMinutePrecision::milli_bps_to_bps(milli_bps), 9_999);
    }

    #[test]
    fn rounding_is_stable_around_the_99_999_threshold() {
        let threshold = 9_999_900; // 99.999%
        let just_inside = SubMinutePrecision::meets_threshold_milli_bps(
            ONE_DAY_MILLIS - 864,
            ONE_DAY_MILLIS,
            threshold,
        )
        .unwrap();
        let just_outside = SubMinutePrecision::meets_threshold_milli_bps(
            ONE_DAY_MILLIS - 865,
            ONE_DAY_MILLIS,
            threshold,
        )
        .unwrap();

        assert!(just_inside, "99.999% must meet the 99.999% threshold");
        assert!(
            !just_outside,
            "99.998999...% must fall below the 99.999% threshold"
        );
        // The truncated whole-bps view of 99.999% is 99.99%, never 100%.
        assert_eq!(SubMinutePrecision::milli_bps_to_bps(threshold), 9_999);
    }

    #[test]
    fn large_durations_cannot_overflow_the_accumulator() {
        // Full uptime over the largest possible window is still exactly 100%.
        let milli_bps = SubMinutePrecision::availability_milli_bps(u64::MAX, u64::MAX).unwrap();
        assert_eq!(milli_bps, SubMinutePrecision::FULL_AVAILABILITY_MILLI_BPS);
    }

    #[test]
    fn uptime_is_clamped_to_the_window() {
        let milli_bps = SubMinutePrecision::availability_milli_bps(500, 100).unwrap();
        assert_eq!(milli_bps, SubMinutePrecision::FULL_AVAILABILITY_MILLI_BPS);
    }

    #[test]
    fn full_downtime_reports_zero_availability() {
        assert_eq!(
            SubMinutePrecision::downtime_availability_milli_bps(ONE_DAY_MILLIS, ONE_DAY_MILLIS)
                .unwrap(),
            0
        );
        // Downtime greater than the window is clamped.
        assert_eq!(
            SubMinutePrecision::downtime_availability_milli_bps(u64::MAX, ONE_DAY_MILLIS).unwrap(),
            0
        );
    }

    #[test]
    fn zero_window_returns_a_typed_error() {
        assert_eq!(
            SubMinutePrecision::availability_milli_bps(10, 0),
            Err(SLAError::DivisionByZero)
        );
        assert_eq!(
            SubMinutePrecision::downtime_availability_milli_bps(10, 0),
            Err(SLAError::DivisionByZero)
        );
        assert_eq!(
            SubMinutePrecision::meets_threshold_milli_bps(10, 0, 9_999_900),
            Err(SLAError::DivisionByZero)
        );
    }
}

#[cfg(test)]
mod outage_cap_tests {
    use super::*;
    use soroban_sdk::{
        symbol_short, testutils::Address as _, testutils::Events as _, Address, Env, IntoVal,
    };

    fn setup(env: &Env) -> (Address, Address, SLACalculatorContractClient) {
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SLACalculatorContract);
        let client = SLACalculatorContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let operator = Address::generate(env);
        client.initialize(&admin, &operator);
        (admin, operator, client)
    }

    #[test]
    fn test_outage_duration_capping_and_event() {
        let env = Env::default();
        let (admin, operator, client) = setup(&env);

        // Default cap is 10,080 minutes (7 days)
        assert_eq!(client.get_max_outage_duration(), 10_080);

        // Outage below cap is not capped
        let res_normal = client.calculate_sla(
            &operator,
            &symbol_short!("OUT1"),
            &symbol_short!("high"),
            &500,
        );
        assert_eq!(res_normal.mttr_minutes, 500);

        // Prolonged outage exceeding 7 days (e.g. 20,000 minutes)
        let prolonged_duration = 20_000;
        let res_capped = client.calculate_sla(
            &operator,
            &symbol_short!("OUT2"),
            &symbol_short!("high"),
            &prolonged_duration,
        );

        // Verified capped at max duration threshold value
        assert_eq!(res_capped.mttr_minutes, 10_080);
        // Penalty is calculated based on 10,080 minutes, not 20,000
        let cfg = client.get_config(&symbol_short!("high"));
        let expected_overtime = (10_080 - cfg.threshold_minutes) as i128;
        let expected_penalty = -expected_overtime * cfg.penalty_per_minute;
        assert_eq!(res_capped.amount, expected_penalty);

        // Verify OutageDurationCapped event was emitted
        let events = env.events().all();
        let mut found_cap_event = false;
        for i in 0..events.len() {
            let event = events.get(i).unwrap();
            let topics = event.0;
            if topics.len() > 0 {
                let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
                if topic0 == Symbol::new(&env, "OutageDurationCapped") {
                    found_cap_event = true;
                }
            }
        }
        assert!(found_cap_event);
    }

    #[test]
    fn test_configurable_max_duration_via_governance() {
        let env = Env::default();
        let (admin, operator, client) = setup(&env);
        let stranger = Address::generate(&env);

        // Non-admin cannot configure cap limit
        let res_unauth = client.try_set_max_outage_duration(&stranger, &1_000);
        assert_eq!(res_unauth.unwrap_err().unwrap(), SLAError::Unauthorized);

        // Cannot set 0
        let res_zero = client.try_set_max_outage_duration(&admin, &0);
        assert_eq!(res_zero.unwrap_err().unwrap(), SLAError::InvalidMTTR);

        // Admin updates cap limit to 1,000 minutes
        client.set_max_outage_duration(&admin, &1_000);
        assert_eq!(client.get_max_outage_duration(), 1_000);

        // Outage with 1,500 minutes gets capped at 1,000 minutes
        let res = client.calculate_sla(
            &operator,
            &symbol_short!("OUT_CAP"),
            &symbol_short!("high"),
            &1_500,
        );
        assert_eq!(res.mttr_minutes, 1_000);

        // View mode also respects the configured cap
        let view_res =
            client.calculate_sla_view(&symbol_short!("OUT_V"), &symbol_short!("high"), &3_000);
        assert_eq!(view_res.mttr_minutes, 1_000);
    }
}
