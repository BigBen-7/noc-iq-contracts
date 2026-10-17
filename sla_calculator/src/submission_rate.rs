use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::{SLAError, ADMIN_KEY};

// -----------------------------------------------------------------------
// Storage keys
// -----------------------------------------------------------------------
const SUB_RATE_KEY: Symbol = symbol_short!("SRATE");

/// Per-operator tracking key (a typed key, since `no_std` has no `format!`).
#[soroban_sdk::contracttype]
enum SubRateKey {
    Operator(Address),
}

// -----------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------
const EVENT_SR_SET: Symbol = symbol_short!("sr_set");
const EVENT_SR_CLR: Symbol = symbol_short!("sr_clr");
const EVENT_VERSION: Symbol = symbol_short!("v1");

// -----------------------------------------------------------------------
// Types
// -----------------------------------------------------------------------

/// Submission rate limiting configuration.
#[soroban_sdk::contracttype]
pub struct SubmissionRateConfig {
    /// Maximum submissions per operator per window.
    pub max_per_operator: u32,
    /// Maximum total submissions per window.
    pub max_total: u32,
    /// Window duration in seconds.
    pub window_seconds: u64,
    /// Whether rate limiting is enabled.
    pub enabled: bool,
}

/// Per-operator submission tracking.
#[soroban_sdk::contracttype]
pub struct OperatorSubmissionTrack {
    /// Operator address.
    pub operator: Address,
    /// Submissions in current window.
    pub count: u32,
    /// Window start timestamp.
    pub window_start: u64,
}

/// Global submission tracking.
#[soroban_sdk::contracttype]
pub struct GlobalSubmissionTrack {
    /// Total submissions in current window.
    pub total_count: u32,
    /// Window start timestamp.
    pub window_start: u64,
}

// -----------------------------------------------------------------------
// Functions
// -----------------------------------------------------------------------

/// Initialize submission rate limiting as disabled.
pub fn init_submission_rate(env: &Env) {
    env.storage().instance().set(
        &SUB_RATE_KEY,
        &SubmissionRateConfig {
            max_per_operator: 50,
            max_total: 200,
            window_seconds: 3600,
            enabled: false,
        },
    );
}

/// Configure submission rate limiting (admin only).
///
/// # Arguments
/// - `caller`: Must be the current admin.
/// - `max_per_operator`: Maximum submissions per operator per window.
/// - `max_total`: Maximum total submissions per window.
/// - `window_seconds`: Window duration in seconds.
///
/// # Events
/// - `sr_set`: Emitted with the new configuration.
pub fn set_submission_rate(
    env: &Env,
    caller: &Address,
    max_per_operator: u32,
    max_total: u32,
    window_seconds: u64,
) -> Result<(), SLAError> {
    require_admin(env, caller)?;

    if max_per_operator == 0 || max_total == 0 || window_seconds == 0 {
        return Err(SLAError::InvalidThreshold);
    }

    if max_per_operator > max_total {
        return Err(SLAError::InvalidThreshold);
    }

    env.storage().instance().set(
        &SUB_RATE_KEY,
        &SubmissionRateConfig {
            max_per_operator,
            max_total,
            window_seconds,
            enabled: true,
        },
    );

    env.events().publish(
        (EVENT_SR_SET, EVENT_VERSION, caller),
        (max_per_operator, max_total, window_seconds),
    );

    Ok(())
}

/// Disable submission rate limiting (admin only).
///
/// # Events
/// - `sr_clr`: Emitted when rate limiting is disabled.
pub fn disable_submission_rate(env: &Env, caller: &Address) -> Result<(), SLAError> {
    require_admin(env, caller)?;

    let mut config: SubmissionRateConfig = env
        .storage()
        .instance()
        .get(&SUB_RATE_KEY)
        .unwrap_or(SubmissionRateConfig {
            max_per_operator: 50,
            max_total: 200,
            window_seconds: 3600,
            enabled: false,
        });

    config.enabled = false;
    env.storage().instance().set(&SUB_RATE_KEY, &config);

    env.events()
        .publish((EVENT_SR_CLR, EVENT_VERSION, caller), ());

    Ok(())
}

/// Check if a submission is allowed under rate limits.
///
/// Checks both per-operator and global limits.
pub fn check_submission_rate(
    env: &Env,
    operator: &Address,
) -> Result<(), SLAError> {
    let config: SubmissionRateConfig = match env.storage().instance().get(&SUB_RATE_KEY) {
        Some(c) => c,
        None => return Ok(()),
    };

    if !config.enabled {
        return Ok(());
    }

    let now = env.ledger().timestamp();

    // Check per-operator limit
    let op_key = SubRateKey::Operator(operator.clone());
    let op_track: OperatorSubmissionTrack = env
        .storage()
        .instance()
        .get(&op_key)
        .unwrap_or(OperatorSubmissionTrack {
            operator: operator.clone(),
            count: 0,
            window_start: now,
        });

    let op_count = if now.saturating_sub(op_track.window_start) >= config.window_seconds {
        0 // New window
    } else {
        op_track.count
    };

    if op_count >= config.max_per_operator {
        return Err(SLAError::ContractPaused);
    }

    // Check global limit
    let global_track: GlobalSubmissionTrack = env
        .storage()
        .instance()
        .get(&symbol_short!("SGLOB"))
        .unwrap_or(GlobalSubmissionTrack {
            total_count: 0,
            window_start: now,
        });

    let global_count = if now.saturating_sub(global_track.window_start) >= config.window_seconds {
        0 // New window
    } else {
        global_track.total_count
    };

    if global_count >= config.max_total {
        return Err(SLAError::ContractPaused);
    }

    Ok(())
}

/// Record a submission for rate limiting.
pub fn record_submission(env: &Env, operator: &Address) {
    let config: SubmissionRateConfig = match env.storage().instance().get(&SUB_RATE_KEY) {
        Some(c) => c,
        None => return,
    };

    if !config.enabled {
        return;
    }

    let now = env.ledger().timestamp();

    // Update per-operator tracking
    let op_key = SubRateKey::Operator(operator.clone());
    let mut op_track: OperatorSubmissionTrack = env
        .storage()
        .instance()
        .get(&op_key)
        .unwrap_or(OperatorSubmissionTrack {
            operator: operator.clone(),
            count: 0,
            window_start: now,
        });

    if now.saturating_sub(op_track.window_start) >= config.window_seconds {
        op_track.window_start = now;
        op_track.count = 0;
    }

    op_track.count = op_track.count.saturating_add(1);
    env.storage().instance().set(&op_key, &op_track);

    // Update global tracking
    let mut global_track: GlobalSubmissionTrack = env
        .storage()
        .instance()
        .get(&symbol_short!("SGLOB"))
        .unwrap_or(GlobalSubmissionTrack {
            total_count: 0,
            window_start: now,
        });

    if now.saturating_sub(global_track.window_start) >= config.window_seconds {
        global_track.window_start = now;
        global_track.total_count = 0;
    }

    global_track.total_count = global_track.total_count.saturating_add(1);
    env.storage().instance().set(&symbol_short!("SGLOB"), &global_track);
}

/// Get submission rate configuration.
pub fn get_submission_rate_config(
    env: &Env,
) -> Result<SubmissionRateConfig, SLAError> {
    Ok(env
        .storage()
        .instance()
        .get(&SUB_RATE_KEY)
        .unwrap_or(SubmissionRateConfig {
            max_per_operator: 50,
            max_total: 200,
            window_seconds: 3600,
            enabled: false,
        }))
}

/// Get operator's current submission count in window.
pub fn get_operator_submission_count(
    env: &Env,
    operator: &Address,
) -> Result<u32, SLAError> {
    let config: SubmissionRateConfig = match env.storage().instance().get(&SUB_RATE_KEY) {
        Some(c) => c,
        None => return Ok(0),
    };

    let now = env.ledger().timestamp();
    let op_key = SubRateKey::Operator(operator.clone());
    let op_track: OperatorSubmissionTrack = match env.storage().instance().get(&op_key) {
        Some(t) => t,
        None => return Ok(0),
    };

    if now.saturating_sub(op_track.window_start) >= config.window_seconds {
        return Ok(0);
    }

    Ok(op_track.count)
}

/// Get global submission count in current window.
pub fn get_global_submission_count(env: &Env) -> Result<u32, SLAError> {
    let global_track: GlobalSubmissionTrack = match env
        .storage()
        .instance()
        .get(&symbol_short!("SGLOB"))
    {
        Some(t) => t,
        None => return Ok(0),
    };

    let now = env.ledger().timestamp();
    let config: SubmissionRateConfig = match env.storage().instance().get(&SUB_RATE_KEY) {
        Some(c) => c,
        None => return Ok(0),
    };

    if now.saturating_sub(global_track.window_start) >= config.window_seconds {
        return Ok(0);
    }

    Ok(global_track.total_count)
}

/// Bounds of the current global rate window as `(start, end)`.
///
/// `start` is when the active window opened and `end` is when it rolls
/// over. With no recorded submissions, or an expired window, the window
/// that the next submission would open is returned (`now .. now + window`).
pub fn get_rate_window(env: &Env) -> Result<(u64, u64), SLAError> {
    let config = get_submission_rate_config(env)?;
    let now = env.ledger().timestamp();
    let start = match env
        .storage()
        .instance()
        .get::<Symbol, GlobalSubmissionTrack>(&symbol_short!("SGLOB"))
    {
        Some(t) if now.saturating_sub(t.window_start) < config.window_seconds => t.window_start,
        _ => now,
    };
    Ok((start, start.saturating_add(config.window_seconds)))
}

/// Helper to verify admin role.
fn require_admin(env: &Env, caller: &Address) -> Result<(), SLAError> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&ADMIN_KEY)
        .ok_or(SLAError::NotInitialized)?;
    if *caller != admin {
        return Err(SLAError::Unauthorized);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SLACalculatorContract;
    use soroban_sdk::testutils::{Address as _, Ledger as _};

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        let id = env.register_contract(None, SLACalculatorContract);
        let admin = Address::generate(&env);
        env.as_contract(&id, || {
            env.storage().instance().set(&ADMIN_KEY, &admin);
            init_submission_rate(&env);
        });
        (env, id, admin)
    }

    #[test]
    fn test_burst_beyond_per_operator_limit_is_throttled() {
        let (env, id, admin) = setup();
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            set_submission_rate(&env, &admin, 3, 100, 60).unwrap();
            for _ in 0..3 {
                assert!(check_submission_rate(&env, &op).is_ok());
                record_submission(&env, &op);
            }
            assert_eq!(check_submission_rate(&env, &op), Err(SLAError::ContractPaused));
            assert_eq!(get_operator_submission_count(&env, &op), Ok(3));
        });
    }

    #[test]
    fn test_burst_beyond_global_limit_is_throttled_across_operators() {
        let (env, id, admin) = setup();
        let (a, b) = (Address::generate(&env), Address::generate(&env));
        env.as_contract(&id, || {
            set_submission_rate(&env, &admin, 2, 3, 60).unwrap();
            record_submission(&env, &a);
            record_submission(&env, &a);
            record_submission(&env, &b);
            // `b` is under its own limit but the global budget is spent.
            assert_eq!(check_submission_rate(&env, &b), Err(SLAError::ContractPaused));
            assert_eq!(get_global_submission_count(&env), Ok(3));
        });
    }

    #[test]
    fn test_throttle_lifts_when_the_window_rolls_over() {
        let (env, id, admin) = setup();
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            set_submission_rate(&env, &admin, 1, 10, 60).unwrap();
            record_submission(&env, &op);
            assert!(check_submission_rate(&env, &op).is_err());
        });
        env.ledger().with_mut(|l| l.timestamp += 60);
        env.as_contract(&id, || {
            assert!(check_submission_rate(&env, &op).is_ok());
            assert_eq!(get_operator_submission_count(&env, &op), Ok(0));
        });
    }

    #[test]
    fn test_disabled_rate_limit_never_throttles() {
        let (env, id, _admin) = setup();
        let op = Address::generate(&env);
        env.as_contract(&id, || {
            for _ in 0..500 {
                record_submission(&env, &op);
            }
            assert!(check_submission_rate(&env, &op).is_ok());
        });
    }

    #[test]
    fn test_rate_window_view_reports_start_and_end() {
        let (env, id, admin) = setup();
        let op = Address::generate(&env);
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        env.as_contract(&id, || {
            set_submission_rate(&env, &admin, 5, 10, 60).unwrap();
            // Nothing recorded yet: the window the next submission opens.
            assert_eq!(get_rate_window(&env), Ok((1_000, 1_060)));
            record_submission(&env, &op);
        });
        env.ledger().with_mut(|l| l.timestamp = 1_030);
        env.as_contract(&id, || {
            assert_eq!(get_rate_window(&env), Ok((1_000, 1_060)));
        });
        env.ledger().with_mut(|l| l.timestamp = 1_060);
        env.as_contract(&id, || {
            // Expired: reports the fresh window.
            assert_eq!(get_rate_window(&env), Ok((1_060, 1_120)));
        });
    }
}
