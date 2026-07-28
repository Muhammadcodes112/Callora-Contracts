//! # Hot/Cold Balance Split
//!
//! This module implements a configurable hot/cold balance split for the
//! Callora Vault contract. The vault's total tracked balance
//! (`VaultMeta.balance`) is logically partitioned into two pools:
//!
//! - **Hot pool**: serves `deduct` calls directly. Kept small and liquid.
//! - **Cold pool**: sweep-only. Requires N-of-M multisig approval to move
//!   funds out, providing defense-in-depth against a compromised
//!   `authorized_caller` or single-key admin draining the vault in one
//!   transaction.
//!
//! This module is implemented but not yet wired into the public contract API.
//! Items are marked `#[allow(dead_code)]` to prevent warnings during the
//! incremental integration phase.
#![allow(dead_code)]
//!    between the last approval and execution.
//!
//! Only one cold sweep may be pending at a time per vault.

use soroban_sdk::{contracttype, Address, Vec};

use crate::VaultError;

/// Default rebalance drift tolerance, in basis points of total balance.
/// If the hot pool's actual share of total balance drifts more than this
/// many basis points from the configured target (`hot_bps`), a deposit
/// triggers an automatic rebalance.
pub const DEFAULT_REBALANCE_THRESHOLD_BPS: u32 = 500; // 5%

/// Basis-point denominator (10_000 bps = 100%).
pub const BPS_DENOMINATOR: i128 = 10_000;

/// Configuration for the hot/cold split. Stored once at `init_cold_storage`
/// and updatable via `set_hot_cold_ratio` (ratio only) or
/// `set_cold_signers` (signer set only) — never both in one call, to keep
/// each authorization check narrowly scoped.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ColdConfig {
    /// Target share of total balance kept in the hot pool, in basis points
    /// (out of 10_000). E.g. `2000` = 20% hot / 80% cold.
    pub hot_bps: u32,
    /// Drift tolerance, in basis points of total balance, before a deposit
    /// triggers an automatic rebalance back toward `hot_bps`.
    pub rebalance_threshold_bps: u32,
    /// Addresses authorized to propose and approve cold sweeps.
    pub cold_signers: Vec<Address>,
    /// Number of distinct `cold_signers` approvals required to execute a
    /// cold sweep (N-of-M, where M = `cold_signers.len()`).
    pub cold_threshold: u32,
}

/// The current hot/cold balance split. `hot + cold` MUST always equal the
/// vault's total tracked balance (`VaultMeta.balance`).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ColdBalances {
    pub hot: i128,
    pub cold: i128,
}

/// A pending multisig-gated request to move funds out of the cold pool.
/// Only one may be pending at a time.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingColdSweep {
    pub amount: i128,
    pub destination: Address,
    pub approvals: Vec<Address>,
    pub proposed_at: u64,
}

impl ColdConfig {
    /// Validates a `ColdConfig` is internally consistent. Does not touch
    /// storage or balances.
    pub fn validate(&self) -> Result<(), VaultError> {
        if self.hot_bps == 0 || self.hot_bps > BPS_DENOMINATOR as u32 {
            return Err(VaultError::InvalidHotBps);
        }
        if self.rebalance_threshold_bps == 0
            || self.rebalance_threshold_bps > BPS_DENOMINATOR as u32
        {
            return Err(VaultError::InvalidRebalanceThreshold);
        }
        if self.cold_signers.is_empty() {
            return Err(VaultError::ColdSignersEmpty);
        }
        if self.cold_threshold == 0 || self.cold_threshold > self.cold_signers.len() {
            return Err(VaultError::InvalidColdThreshold);
        }
        // Reject duplicate signers — a duplicate would let one key count
        // twice toward the threshold, silently weakening the multisig.
        for i in 0..self.cold_signers.len() {
            for j in (i + 1)..self.cold_signers.len() {
                if self.cold_signers.get(i) == self.cold_signers.get(j) {
                    return Err(VaultError::DuplicateColdSigner);
                }
            }
        }
        Ok(())
    }

    /// Returns true if `addr` is a configured cold signer.
    pub fn is_cold_signer(&self, addr: &Address) -> bool {
        self.cold_signers.iter().any(|s| s == *addr)
    }
}

impl ColdBalances {
    /// Total balance represented by this hot/cold split.
    pub fn total(&self) -> Result<i128, VaultError> {
        self.hot.checked_add(self.cold).ok_or(VaultError::Overflow)
    }
}

/// Computes the hot pool's current share of `total`, in basis points.
/// Returns `0` if `total` is `0` (no funds, no drift).
fn hot_share_bps(hot: i128, total: i128) -> Result<i128, VaultError> {
    if total == 0 {
        return Ok(0);
    }
    hot.checked_mul(BPS_DENOMINATOR)
        .ok_or(VaultError::Overflow)?
        .checked_div(total)
        .ok_or(VaultError::Overflow)
}

/// Public wrapper around `target_hot`, for use from `lib.rs` during
/// `init_cold_storage` to compute the initial split.
pub fn target_hot_pub(total: i128, hot_bps: u32) -> Result<i128, VaultError> {
    target_hot(total, hot_bps)
}

/// Computes the target hot amount for a given `total`, per `hot_bps`.
/// Uses integer division (floor); any remainder stays in cold, which is
/// the conservative direction (keeps slightly more in cold, never less).
fn target_hot(total: i128, hot_bps: u32) -> Result<i128, VaultError> {
    total
        .checked_mul(hot_bps as i128)
        .ok_or(VaultError::Overflow)?
        .checked_div(BPS_DENOMINATOR)
        .ok_or(VaultError::Overflow)
}

/// Given the current hot/cold split and config, returns the rebalanced
/// split if the hot pool's drift from `hot_bps` exceeds
/// `rebalance_threshold_bps`. Returns the unchanged split if within
/// tolerance. Never pulls cold funds into hot — only ever moves hot
/// surplus into cold. Pulling cold back into hot requires the explicit,
/// multisig-gated cold-sweep-to-hot path, never an automatic side effect
/// of a deposit.
pub fn maybe_rebalance(
    balances: &ColdBalances,
    config: &ColdConfig,
) -> Result<ColdBalances, VaultError> {
    let total = balances.total()?;
    if total == 0 {
        return Ok(balances.clone());
    }

    let current_share = hot_share_bps(balances.hot, total)?;
    let target_share = config.hot_bps as i128;
    let drift = (current_share - target_share).abs();

    if drift <= config.rebalance_threshold_bps as i128 {
        // Within tolerance — no rebalance.
        return Ok(balances.clone());
    }

    if current_share <= target_share {
        // Hot is already at or below target share; only excess-hot drift
        // triggers an automatic move (hot -> cold). A hot deficit is left
        // for the multisig-gated sweep-to-hot path to correct deliberately.
        return Ok(balances.clone());
    }

    let new_hot = target_hot(total, config.hot_bps)?;
    let moved = balances
        .hot
        .checked_sub(new_hot)
        .ok_or(VaultError::Overflow)?;
    let new_cold = balances
        .cold
        .checked_add(moved)
        .ok_or(VaultError::Overflow)?;

    Ok(ColdBalances {
        hot: new_hot,
        cold: new_cold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;
    use soroban_sdk::testutils::Address as _;

    fn addr(env: &Env, seed: u8) -> Address {
        // Deterministic-ish distinct addresses for unit tests that don't
        // need full Soroban auth simulation.
        let _ = seed;
        Address::generate(env)
    }

    #[test]
    fn hot_share_bps_zero_total_is_zero() {
        assert_eq!(hot_share_bps(0, 0).unwrap(), 0);
    }

    #[test]
    fn hot_share_bps_basic() {
        // 2000/10000 = 20%
        assert_eq!(hot_share_bps(2000, 10_000).unwrap(), 2000);
    }

    #[test]
    fn target_hot_basic() {
        assert_eq!(target_hot(10_000, 2000).unwrap(), 2000);
        // floor behavior: remainder stays in cold
        assert_eq!(target_hot(9_999, 2000).unwrap(), 1999);
    }

    #[test]
    fn maybe_rebalance_within_tolerance_is_noop() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        // hot share = 2200/10000 = 22%, target 20%, drift 2% <= 5% tolerance
        let balances = ColdBalances {
            hot: 2200,
            cold: 7800,
        };
        let result = maybe_rebalance(&balances, &config).unwrap();
        assert_eq!(result, balances);
    }

    #[test]
    fn maybe_rebalance_excess_hot_moves_to_cold() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        // hot share = 5000/10000 = 50%, target 20%, drift 30% > 5% tolerance
        let balances = ColdBalances {
            hot: 5000,
            cold: 5000,
        };
        let result = maybe_rebalance(&balances, &config).unwrap();
        assert_eq!(result.hot, 2000);
        assert_eq!(result.cold, 8000);
        assert_eq!(result.total().unwrap(), balances.total().unwrap());
    }

    #[test]
    fn maybe_rebalance_hot_deficit_does_not_auto_pull_from_cold() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 5000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        // hot share = 1000/10000 = 10%, target 50%, drift 40% > 5% tolerance,
        // but hot is BELOW target, so no automatic pull from cold.
        let balances = ColdBalances {
            hot: 1000,
            cold: 9000,
        };
        let result = maybe_rebalance(&balances, &config).unwrap();
        assert_eq!(result, balances);
    }

    #[test]
    fn maybe_rebalance_zero_total_is_noop() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        let balances = ColdBalances { hot: 0, cold: 0 };
        let result = maybe_rebalance(&balances, &config).unwrap();
        assert_eq!(result, balances);
    }

    #[test]
    fn cold_config_validate_rejects_zero_hot_bps() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 0,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        assert_eq!(config.validate(), Err(VaultError::InvalidHotBps));
    }

    #[test]
    fn cold_config_validate_rejects_hot_bps_over_max() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 10_001,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 1,
        };
        assert_eq!(config.validate(), Err(VaultError::InvalidHotBps));
    }

    #[test]
    fn cold_config_validate_rejects_empty_signers() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: Vec::new(&env),
            cold_threshold: 1,
        };
        assert_eq!(config.validate(), Err(VaultError::ColdSignersEmpty));
    }

    #[test]
    fn cold_config_validate_rejects_threshold_over_signer_count() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 2,
        };
        assert_eq!(config.validate(), Err(VaultError::InvalidColdThreshold));
    }

    #[test]
    fn cold_config_validate_rejects_zero_threshold() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v
            },
            cold_threshold: 0,
        };
        assert_eq!(config.validate(), Err(VaultError::InvalidColdThreshold));
    }

    #[test]
    fn cold_config_validate_rejects_duplicate_signers() {
        let env = Env::default();
        let shared = addr(&env, 1);
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(shared.clone());
                v.push_back(shared);
                v
            },
            cold_threshold: 1,
        };
        assert_eq!(config.validate(), Err(VaultError::DuplicateColdSigner));
    }

    #[test]
    fn cold_config_validate_accepts_valid_config() {
        let env = Env::default();
        let config = ColdConfig {
            hot_bps: 2000,
            rebalance_threshold_bps: 500,
            cold_signers: {
                let mut v = Vec::new(&env);
                v.push_back(addr(&env, 1));
                v.push_back(addr(&env, 2));
                v.push_back(addr(&env, 3));
                v
            },
            cold_threshold: 2,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn cold_balances_total_overflow_is_caught() {
        let balances = ColdBalances {
            hot: i128::MAX,
            cold: 1,
        };
        assert_eq!(balances.total(), Err(VaultError::Overflow));
    }
}
