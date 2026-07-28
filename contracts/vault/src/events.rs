//! Event topic Symbol constructors for the Callora Vault contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.
//!
//! Many symbols in this module correspond to entrypoints that exist in the full
//! vault API but are not yet wired in the current simplified `lib.rs`. They are
//! kept here so off-chain indexers that subscribe to the `"vault"` event stream
//! receive stable symbol bytes when those entrypoints are added.
#![allow(dead_code)]

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the vault contract is first initialized with an owner and initial balance.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when an owner nominates a new admin. The new admin must call
/// `claim_admin` to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when a nominated admin claims ownership and completes the admin transfer.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

/// Returns the Symbol for the `"set_authorized_caller"` event topic.
///
/// Emitted when an owner adds a new authorized caller for `deduct` operations.
pub fn event_set_authorized_caller(env: &Env) -> Symbol {
    Symbol::new(env, "set_authorized_caller")
}

/// Returns the Symbol for the `"set_max_deduct"` event topic.
///
/// Emitted when the owner updates the maximum deductible amount per call.
pub fn event_set_max_deduct(env: &Env) -> Symbol {
    Symbol::new(env, "set_max_deduct")
}

/// Returns the Symbol for the `"vault_paused"` event topic.
///
/// Emitted when the vault is paused, blocking deposits and deducts.
/// Owner withdrawals and admin distributions remain allowed.
pub fn event_vault_paused(env: &Env) -> Symbol {
    Symbol::new(env, "vault_paused")
}

/// Returns the Symbol for the `"vault_unpaused"` event topic.
///
/// Emitted when the vault is unpaused, resuming normal operation.
pub fn event_vault_unpaused(env: &Env) -> Symbol {
    Symbol::new(env, "vault_unpaused")
}

/// Returns the Symbol for the `"deposit"` event topic.
///
/// Emitted when a caller deposits USDC into the vault.
pub fn event_deposit(env: &Env) -> Symbol {
    Symbol::new(env, "deposit")
}

/// Returns the Symbol for the `"deduct"` event topic.
///
/// Emitted when an authorized caller or admin deducts funds from the vault.
/// Includes an optional request ID for idempotency tracking.
pub fn event_deduct(env: &Env) -> Symbol {
    Symbol::new(env, "deduct")
}

/// Returns the Symbol for the `"ownership_nominated"` event topic.
///
/// Emitted when the current owner nominates a new owner.
/// The nominee must call `claim_ownership` to complete the transfer.
pub fn event_ownership_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "ownership_nominated")
}

/// Returns the Symbol for the `"ownership_accepted"` event topic.
///
/// Emitted when a nominated owner accepts and completes the ownership transfer.
pub fn event_ownership_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "ownership_accepted")
}

/// Returns the Symbol for the `"withdraw"` event topic.
///
/// Emitted when the vault owner withdraws funds from the vault.
pub fn event_withdraw(env: &Env) -> Symbol {
    Symbol::new(env, "withdraw")
}

/// Returns the Symbol for the `"withdraw_to"` event topic.
///
/// Emitted when the vault owner withdraws funds to a specified recipient address.
pub fn event_withdraw_to(env: &Env) -> Symbol {
    Symbol::new(env, "withdraw_to")
}

/// Returns the Symbol for the `"distribute"` event topic.
///
/// Emitted when the admin distributes funds to a designated recipient.
pub fn event_distribute(env: &Env) -> Symbol {
    Symbol::new(env, "distribute")
}

/// Returns the Symbol for the `"set_revenue_pool"` event topic.
///
/// Emitted when the owner configures a revenue pool address for fund settlements.
pub fn event_set_revenue_pool(env: &Env) -> Symbol {
    Symbol::new(env, "set_revenue_pool")
}

/// Returns the Symbol for the `"clear_revenue_pool"` event topic.
///
/// Emitted when the owner clears the configured revenue pool address.
pub fn event_clear_revenue_pool(env: &Env) -> Symbol {
    Symbol::new(env, "clear_revenue_pool")
}

/// Returns the Symbol for the `"set_settlement"` event topic.
///
/// Emitted when the admin sets or updates the settlement contract address.
pub fn event_set_settlement(env: &Env) -> Symbol {
    Symbol::new(env, "set_settlement")
}

/// Returns the Symbol for the `"metadata_set"` event topic.
///
/// Emitted when the admin sets metadata for an offering.
pub fn event_metadata_set(env: &Env) -> Symbol {
    Symbol::new(env, "metadata_set")
}

/// Returns the Symbol for the `"price_set"` event topic.
///
/// Emitted when the admin sets a price for an offering.
pub fn event_price_set(env: &Env) -> Symbol {
    Symbol::new(env, "price_set")
}

/// Returns the Symbol for the `"price_removed"` event topic.
///
/// Emitted when the admin removes a price for an offering.
pub fn event_price_removed(env: &Env) -> Symbol {
    Symbol::new(env, "price_removed")
}

/// Returns the Symbol for the `"metadata_updated"` event topic.
///
/// Emitted when the admin updates metadata for an offering.
pub fn event_metadata_updated(env: &Env) -> Symbol {
    Symbol::new(env, "metadata_updated")
}

/// Returns the Symbol for the `"metadata_removed"` event topic.
///
/// Emitted when the admin removes metadata for an offering.
pub fn event_metadata_removed(env: &Env) -> Symbol {
    Symbol::new(env, "metadata_removed")
}

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the vault contract is upgraded to a new WASM hash.
///
/// Retained for backwards compatibility with off-chain consumers that subscribed
/// to the original single-event shape. Newly written indexers should prefer the
/// structured [`event_upgrade_started`] / [`event_upgrade_completed`] pair.
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

/// Returns the Symbol for the `"upgrade_started"` event topic.
///
/// Emitted *before* the host swaps the contract WASM. Pairs with
/// [`event_upgrade_completed`] so indexers can distinguish a host-level trap
/// mid-upgrade from a fully applied upgrade.
pub fn event_upgrade_started(env: &Env) -> Symbol {
    Symbol::new(env, "upgrade_started")
}

/// Returns the Symbol for the `"upgrade_completed"` event topic.
///
/// Emitted *after* the WASM swap and the `ContractVersion` storage write.
/// Receipt of this event without a preceding `upgrade_started` at the same
/// `(ledger, timestamp)` would indicate event-emission tampering.
pub fn event_upgrade_completed(env: &Env) -> Symbol {
    Symbol::new(env, "upgrade_completed")
}

/// Returns the Symbol for the `"allowlist_add"` event topic.
///
/// Emitted when the owner adds an address to the vault deposit allowlist.
pub fn event_allowlist_add(env: &Env) -> Symbol {
    Symbol::new(env, "allowlist_add")
}

/// Returns the Symbol for the `"allowlist_clear"` event topic.
///
/// Emitted when the owner clears the entire vault deposit allowlist.
pub fn event_allowlist_clear(env: &Env) -> Symbol {
    Symbol::new(env, "allowlist_clear")
}

/// Returns the Symbol for the `"revenue_pool_proposed"` event topic.
///
/// Emitted when the owner proposes a new revenue pool address.
pub fn event_revenue_pool_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "revenue_pool_proposed")
}

/// Returns the Symbol for the `"revenue_pool_accepted"` event topic.
///
/// Emitted when a proposed revenue pool accepts the role.
pub fn event_revenue_pool_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "revenue_pool_accepted")
}

/// Returns the Symbol for the `"revenue_pool_cancelled"` event topic.
///
/// Emitted when the owner cancels a pending revenue pool proposal.
pub fn event_revenue_pool_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "revenue_pool_cancelled")
}

/// Returns the Symbol for the `"request_id_pruned"` event topic.
///
/// Emitted when an expired idempotency request ID is pruned from storage.
pub fn event_request_id_pruned(env: &Env) -> Symbol {
    Symbol::new(env, "request_id_pruned")
}

/// Returns the Symbol for the `"admin_broadcast"` event topic.
///
/// Emitted when the admin broadcasts an emergency message.
pub fn event_admin_broadcast(env: &Env) -> Symbol {
    Symbol::new(env, "admin_broadcast")
}

// --- Escape-hatch timelock event symbols (Issue #482) ---------------------

/// Returns the Symbol for the `"pause_proposed"` event topic.
///
/// Emitted when an admin stages a pause proposal that must wait for the
/// configured timelock window before it can be executed.
pub fn event_pause_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "pause_proposed")
}

/// Returns the Symbol for the `"pause_executed"` event topic.
///
/// Emitted when an admin executes a pause proposal after the timelock
/// window has elapsed.
pub fn event_pause_executed(env: &Env) -> Symbol {
    Symbol::new(env, "pause_executed")
}

/// Returns the Symbol for the `"pause_cancelled"` event topic.
///
/// Emitted when an admin aborts a pending pause proposal.
pub fn event_pause_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "pause_cancelled")
}

/// Returns the Symbol for the `"upgrade_proposed"` event topic.
///
/// Emitted when an admin stages an upgrade proposal.
pub fn event_upgrade_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "upgrade_proposed")
}

/// Returns the Symbol for the `"upgrade_executed"` event topic.
///
/// Emitted when an admin executes an upgrade proposal after the timelock
/// window has elapsed.
pub fn event_upgrade_executed(env: &Env) -> Symbol {
    Symbol::new(env, "upgrade_executed")
}

/// Returns the Symbol for the `"upgrade_cancelled"` event topic.
///
/// Emitted when an admin aborts a pending upgrade proposal.
pub fn event_upgrade_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "upgrade_cancelled")
}

/// Returns the Symbol for the `"sweep_proposed"` event topic.
///
/// Emitted when an admin stages a sweep (`distribute`) proposal.
pub fn event_sweep_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "sweep_proposed")
}

/// Returns the Symbol for the `"sweep_executed"` event topic.
///
/// Emitted when an admin executes a sweep proposal after the timelock
/// window has elapsed.
pub fn event_sweep_executed(env: &Env) -> Symbol {
    Symbol::new(env, "sweep_executed")
}

/// Returns the Symbol for the `"sweep_cancelled"` event topic.
///
/// Emitted when an admin aborts a pending sweep proposal.
pub fn event_sweep_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "sweep_cancelled")
}

/// Returns the Symbol for the `"timelock_window_changed"` event topic.
///
/// Emitted when the admin updates the configured timelock window length.
pub fn event_timelock_window_changed(env: &Env) -> Symbol {
    Symbol::new(env, "tl_window_changed")
}

/// Returns the Symbol for the `"rescue_funds"` event topic.
///
/// Emitted when the admin rescues funds from the vault.
pub fn event_rescue_funds(env: &Env) -> Symbol {
    Symbol::new(env, "rescue_funds")
}

/// Returns the Symbol for the `"reserve_cap_set"` event topic.
///
/// Emitted when the owner configures a reserve cap for a token.
pub fn event_reserve_cap_set(env: &Env) -> Symbol {
    Symbol::new(env, "reserve_cap_set")
}

/// Returns the Symbol for the `"swept"` event topic.
///
/// Emitted when idle balance is swept from the vault.
pub fn event_swept(env: &Env) -> Symbol {
    Symbol::new(env, "swept")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot test: verifies event topic byte identity preservation.
    /// If this test fails, a topic was accidentally renamed or changed.
    #[test]
    fn test_event_init_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_init(&env);
        assert_eq!(sym, Symbol::new(&env, "init"));
    }

    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_admin_nominated(&env);
        assert_eq!(sym, Symbol::new(&env, "admin_nominated"));
    }

    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_admin_accepted(&env);
        assert_eq!(sym, Symbol::new(&env, "admin_accepted"));
    }

    #[test]
    fn test_event_rescue_funds_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_rescue_funds(&env);
        assert_eq!(sym, Symbol::new(&env, "rescue_funds"));
    }

    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_admin_cancelled(&env);
        assert_eq!(sym, Symbol::new(&env, "admin_cancelled"));
    }

    #[test]
    fn test_event_set_authorized_caller_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_set_authorized_caller(&env);
        assert_eq!(sym, Symbol::new(&env, "set_authorized_caller"));
    }

    #[test]
    fn test_event_set_max_deduct_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_set_max_deduct(&env);
        assert_eq!(sym, Symbol::new(&env, "set_max_deduct"));
    }

    #[test]
    fn test_event_vault_paused_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_vault_paused(&env);
        assert_eq!(sym, Symbol::new(&env, "vault_paused"));
    }

    #[test]
    fn test_event_vault_unpaused_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_vault_unpaused(&env);
        assert_eq!(sym, Symbol::new(&env, "vault_unpaused"));
    }

    #[test]
    fn test_event_deposit_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_deposit(&env);
        assert_eq!(sym, Symbol::new(&env, "deposit"));
    }

    #[test]
    fn test_event_deduct_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_deduct(&env);
        assert_eq!(sym, Symbol::new(&env, "deduct"));
    }

    #[test]
    fn test_event_ownership_nominated_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_ownership_nominated(&env);
        assert_eq!(sym, Symbol::new(&env, "ownership_nominated"));
    }

    #[test]
    fn test_event_ownership_accepted_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_ownership_accepted(&env);
        assert_eq!(sym, Symbol::new(&env, "ownership_accepted"));
    }

    #[test]
    fn test_event_withdraw_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_withdraw(&env);
        assert_eq!(sym, Symbol::new(&env, "withdraw"));
    }

    #[test]
    fn test_event_withdraw_to_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_withdraw_to(&env);
        assert_eq!(sym, Symbol::new(&env, "withdraw_to"));
    }

    #[test]
    fn test_event_distribute_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_distribute(&env);
        assert_eq!(sym, Symbol::new(&env, "distribute"));
    }

    #[test]
    fn test_event_set_revenue_pool_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_set_revenue_pool(&env);
        assert_eq!(sym, Symbol::new(&env, "set_revenue_pool"));
    }

    #[test]
    fn test_event_clear_revenue_pool_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_clear_revenue_pool(&env);
        assert_eq!(sym, Symbol::new(&env, "clear_revenue_pool"));
    }

    #[test]
    fn test_event_set_settlement_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_set_settlement(&env);
        assert_eq!(sym, Symbol::new(&env, "set_settlement"));
    }

    #[test]
    fn test_event_metadata_set_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_metadata_set(&env);
        assert_eq!(sym, Symbol::new(&env, "metadata_set"));
    }

    #[test]
    fn test_event_price_set_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_price_set(&env);
        assert_eq!(sym, Symbol::new(&env, "price_set"));
    }

    #[test]
    fn test_event_price_removed_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_price_removed(&env);
        assert_eq!(sym, Symbol::new(&env, "price_removed"));
    }

    #[test]
    fn test_event_metadata_updated_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_metadata_updated(&env);
        assert_eq!(sym, Symbol::new(&env, "metadata_updated"));
    }

    #[test]
    fn test_event_metadata_removed_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_metadata_removed(&env);
        assert_eq!(sym, Symbol::new(&env, "metadata_removed"));
    }

    #[test]
    fn test_event_upgraded_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_upgraded(&env);
        assert_eq!(sym, Symbol::new(&env, "upgraded"));
    }

    #[test]
    fn test_event_upgrade_started_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_upgrade_started(&env);
        assert_eq!(sym, Symbol::new(&env, "upgrade_started"));
    }

    #[test]
    fn test_event_upgrade_completed_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_upgrade_completed(&env);
        assert_eq!(sym, Symbol::new(&env, "upgrade_completed"));
    }

    #[test]
    fn test_event_allowlist_add_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_allowlist_add(&env);
        assert_eq!(sym, Symbol::new(&env, "allowlist_add"));
    }

    #[test]
    fn test_event_allowlist_clear_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_allowlist_clear(&env);
        assert_eq!(sym, Symbol::new(&env, "allowlist_clear"));
    }

    #[test]
    fn test_event_revenue_pool_proposed_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_revenue_pool_proposed(&env);
        assert_eq!(sym, Symbol::new(&env, "revenue_pool_proposed"));
    }

    #[test]
    fn test_event_revenue_pool_accepted_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_revenue_pool_accepted(&env);
        assert_eq!(sym, Symbol::new(&env, "revenue_pool_accepted"));
    }

    #[test]
    fn test_event_revenue_pool_cancelled_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_revenue_pool_cancelled(&env);
        assert_eq!(sym, Symbol::new(&env, "revenue_pool_cancelled"));
    }

    /// Snapshot: proves event_request_id_pruned still maps to exactly the bytes for "request_id_pruned".
    #[test]
    fn test_event_request_id_pruned_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_request_id_pruned(&env);
        assert_eq!(sym, Symbol::new(&env, "request_id_pruned"));
    }

    /// Snapshot: proves event_admin_broadcast still maps to exactly the bytes for "admin_broadcast".
    #[test]
    fn test_event_admin_broadcast_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_admin_broadcast(&env);
        assert_eq!(sym, Symbol::new(&env, "admin_broadcast"));
    }

    /// Snapshot: proves event_reserve_cap_set still maps to exactly the bytes for "reserve_cap_set".
    #[test]
    fn test_event_reserve_cap_set_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_reserve_cap_set(&env);
        assert_eq!(sym, Symbol::new(&env, "reserve_cap_set"));
    }

    /// Snapshot: proves event_swept still maps to exactly the bytes for "swept".
    #[test]
    fn test_event_swept_bytes() {
        let env = soroban_sdk::Env::default();
        let sym = event_swept(&env);
        assert_eq!(sym, Symbol::new(&env, "swept"));
    }
}
