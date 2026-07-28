//! Read-only views that report what a hypothetical state-changing call *would*
//! do, without mutating any storage or moving any USDC.
//!
//! The vault occasionally accumulates **untracked on-ledger surplus**: USDC
//! that lives at the vault address but is not reflected in `meta.balance`. The
//! canonical cause is a third party transferring USDC directly to the vault
//! contract address bypassing `deposit`, but it can also arise from rounding,
//! airdrops, or manually-credited recovery funds. The admin recovers this
//! surplus by calling [`crate::CalloraVault::distribute`], which transfers an
//! admin-specified amount.
//!
//! A "sweep" is the convention of distributing the entire surplus at once.
//! There is no dedicated `sweep_idle_balance` mutator on this contract; the
//! sweep is performed by calling `distribute` with `amount` equal to the
//! amount this dry-run reports.
//!
//! The view in this module lets indexers, dashboards, and admins inspect what
//! a sweep would move **before** they sign the transfer. It performs no auth,
//! does not bump TTL, and reads only `meta.balance` plus the on-ledger USDC
//! balance.

use soroban_sdk::{contracttype, token, Address, Env};

use crate::VaultError;

/// Structured result of [`CalloraVault::dry_run_sweep_idle_balance`].
///
/// Fields are emitted in a stable order so off-chain decoders can rely on
/// positional access in addition to `#[contracttype]` decoding.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweepPreview {
    /// USDC token balance currently held by the vault contract on-ledger.
    /// Read fresh from the token contract at the moment the view is called.
    pub on_ledger_balance: i128,
    /// USDC balance recorded by the vault's internal accounting (`meta.balance`).
    /// Mutated by `deposit`, `deduct`, `withdraw`, and `withdraw_to`.
    pub tracked_balance: i128,
    /// `max(on_ledger_balance - tracked_balance, 0)`.
    ///
    /// Saturates at zero when the tracked balance exceeds what the vault holds
    /// on-ledger (a state that should never arise during normal operation but
    /// is reported defensively rather than panicking on overflow).
    pub idle_balance: i128,
    /// `true` iff `idle_balance > 0` — i.e. a sweep would transfer a non-zero
    /// amount. Provided so off-chain callers can branch on a single boolean
    /// without re-deriving the comparison.
    pub has_idle: bool,
}

/// Compute the idle (untracked) on-ledger USDC balance for the vault.
///
/// Called by [`crate::CalloraVault::dry_run_sweep_idle_balance`]; extracted
/// here so it can be tested in isolation without the full contract impl.
pub fn compute_sweep_preview(env: &Env) -> Result<SweepPreview, VaultError> {
    let tracked_balance: i128 = env
        .storage()
        .instance()
        .get(&crate::DataKey::Balance)
        .unwrap_or(0);
    let usdc_addr: Address = env
        .storage()
        .instance()
        .get(&crate::DataKey::UsdcToken)
        .ok_or(VaultError::NotInitialized)?;
    let usdc = token::Client::new(env, &usdc_addr);
    let on_ledger = usdc.balance(&env.current_contract_address());

    // Defensive: saturate at zero rather than reporting a negative idle balance.
    let idle = if on_ledger > tracked_balance {
        on_ledger - tracked_balance
    } else {
        0
    };

    Ok(SweepPreview {
        on_ledger_balance: on_ledger,
        tracked_balance,
        idle_balance: idle,
        has_idle: idle > 0,
    })
}
