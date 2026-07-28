//! # Tests for vault escape-hatch timelock (Issue #482)
//!
//! These tests cover the full lifecycle of `pause` / `upgrade` / `sweep`
//! proposals: propose, cancel, propose-restarts-timer, execute at/after the
//! boundary, execute rejection before the window, non-admin rejection,
//! window-config setter bounds, parallel proposal isolation, and
//! wall-clock overflow handling.

extern crate std;

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, BytesN, Env, IntoVal, Symbol};

use super::test::CalloraVault;
use super::test::CalloraVaultClient;
use super::{
    timelock, VaultError, DEFAULT_TIMELOCK_SECONDS, MAX_TIMELOCK_SECONDS, MIN_TIMELOCK_SECONDS,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

fn setup(env: &Env) -> (Address, CalloraVaultClient<'_>, Address, Address, Address) {
    env.ledger().set_timestamp(1_700_000_000);
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let (usdc, _) = create_usdc(env, &owner);
    let admin = Address::generate(env);
    let recipient = Address::generate(env);
    let settlement = Address::generate(env);
    env.mock_all_auths();
    // Owner is initial admin by default (lib.rs::init sets Admin = owner).
    client.init(
        &owner,
        &usdc,
        &0i128,       // initial_balance
        &owner,       // authorized_caller (owner)
        &1i128,       // min_deposit
        &None,        // revenue_pool
        &1000i128,    // max_deduct
        &settlement,  // settlement
    );
    // Rotate admin to a distinct address so admin != owner for auth tests.
    client.set_admin(&owner, &admin);
    client.accept_admin();
    (owner, client, admin, recipient, vault_addr)
}

// ---------------------------------------------------------------------------
// Window configuration
// ---------------------------------------------------------------------------

#[test]
fn default_window_is_48_hours() {
    let env = Env::default();
    let (_, client, _, _, _) = setup(&env);
    assert_eq!(client.get_timelock_window(), DEFAULT_TIMELOCK_SECONDS);
    assert_eq!(DEFAULT_TIMELOCK_SECONDS, 172_800);
}

/// `#482` Acceptance: window is configurable per contract (env).
#[test]
fn set_window_updates_storage_and_emits_event() {
    let env = Env::default();
    let (vault_addr, client, admin, _, _) = setup(&env);
    client.set_timelock_window(&admin, &(MIN_TIMELOCK_SECONDS + 60));
    assert_eq!(
        client.get_timelock_window(),
        MIN_TIMELOCK_SECONDS + 60
    );

    // Verify the change event was emitted.
    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "tl_window_changed"));
    assert_eq!(last.0, vault_addr);
}

#[test]
fn set_window_rejects_non_admin() {
    let env = Env::default();
    let (_, client, _, _, _) = setup(&env);
    let intruder = Address::generate(&env);
    let res = client.try_set_timelock_window(&intruder, &(MIN_TIMELOCK_SECONDS + 60));
    assert!(res.is_err(), "non-admin must be rejected");
}

#[test]
fn set_window_rejects_out_of_bounds_low() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let res = client.try_set_timelock_window(&admin, &(MIN_TIMELOCK_SECONDS - 1));
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::InvalidTimelockWindow
    );
}

#[test]
fn set_window_rejects_zero() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let res = client.try_set_timelock_window(&admin, &0u64);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::InvalidTimelockWindow
    );
}

#[test]
fn set_window_rejects_out_of_bounds_high() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let res = client.try_set_timelock_window(&admin, &(MAX_TIMELOCK_SECONDS + 1));
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::InvalidTimelockWindow
    );
}

#[test]
fn set_window_accepts_known_boundaries() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    // Lower bound (inclusive).
    assert!(client.try_set_timelock_window(&admin, &MIN_TIMELOCK_SECONDS).is_ok());
    // Upper bound (inclusive).
    assert!(client.try_set_timelock_window(&admin, &MAX_TIMELOCK_SECONDS).is_ok());
}

// ---------------------------------------------------------------------------
// Pause timelock — propose / execute / cancel
// ---------------------------------------------------------------------------

#[test]
fn propose_pause_stores_snapshot_and_deadline() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    let proposal = client.get_pending_pause().expect("expected pending pause");
    assert_eq!(proposal.proposed_at, 1_700_000_000);
    assert_eq!(
        proposal.execute_after,
        1_700_000_000 + DEFAULT_TIMELOCK_SECONDS
    );
    assert!(!client.is_paused(), "vault must remain unpaused");
}

#[test]
fn execute_pause_fails_before_window() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    // 1 second before the boundary → must fail with TimelockNotExpired.
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS - 1);
    let res = client.try_execute_pause(&admin);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::TimelockNotExpired
    );
    assert!(!client.is_paused());
    // Pause proposal should still be live.
    assert!(client.get_pending_pause().is_some());
}

#[test]
fn execute_pause_succeeds_at_boundary() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    client.execute_pause(&admin);
    assert!(client.is_paused(), "vault must be paused at boundary");
    assert!(
        client.get_pending_pause().is_none(),
        "proposal must be consumed on success"
    );
}

#[test]
fn execute_pause_succeeds_far_after_window() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    env.ledger().set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS * 10);
    client.execute_pause(&admin);
    assert!(client.is_paused());
    assert!(client.get_pending_pause().is_none());
}

#[test]
fn execute_pause_without_proposal_fails() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    env.ledger().set_timestamp(2_000_000_000);
    let res = client.try_execute_pause(&admin);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::ProposalNotFound);
}

#[test]
fn cancel_pause_removes_proposal() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    assert!(client.get_pending_pause().is_some());
    client.cancel_pause(&admin);
    assert!(client.get_pending_pause().is_none());
}

#[test]
fn pause_propose_replaces_and_restarts_timer() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    env.ledger().set_timestamp(1_700_000_100);
    client.propose_pause(&admin);
    let p = client.get_pending_pause().unwrap();
    assert_eq!(p.proposed_at, 1_700_000_100);
    assert_eq!(p.execute_after, 1_700_000_100 + DEFAULT_TIMELOCK_SECONDS);
}

// ---------------------------------------------------------------------------
// Upgrade timelock — propose / execute / cancel
// ---------------------------------------------------------------------------

#[test]
fn propose_upgrade_stores_wasm_and_deadline() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[7u8; 32]);
    client.propose_upgrade(&admin, &hash);
    let p = client.get_pending_upgrade().unwrap();
    assert_eq!(p.wasm_hash, hash);
    assert_eq!(p.proposed_at, 1_700_000_000);
    assert_eq!(p.execute_after, 1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
}

#[test]
fn execute_upgrade_fails_before_window() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[9u8; 32]);
    client.propose_upgrade(&admin, &hash);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS - 1);
    let res = client.try_execute_upgrade(&admin);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::TimelockNotExpired
    );
    assert!(client.get_pending_upgrade().is_some());
}

#[test]
fn execute_upgrade_succeeds_at_boundary_and_records_version() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[11u8; 32]);
    client.propose_upgrade(&admin, &hash);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    client.execute_upgrade(&admin);
    assert_eq!(client.get_version(), Some(hash));
    assert!(client.get_pending_upgrade().is_none());
}

#[test]
fn cancel_upgrade_removes_proposal() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[13u8; 32]);
    client.propose_upgrade(&admin, &hash);
    client.cancel_upgrade(&admin);
    assert!(client.get_pending_upgrade().is_none());
    // Version should still be None since we cancelled before execution.
    assert!(client.get_version().is_none());
}

// ---------------------------------------------------------------------------
// Sweep timelock — propose / execute / cancel
// ---------------------------------------------------------------------------

#[test]
fn propose_sweep_stores_recipient_amount_and_deadline() {
    let env = Env::default();
    let (_, client, admin, recipient, _) = setup(&env);
    client.propose_sweep(&admin, &recipient, &500i128);
    let p = client.get_pending_sweep().unwrap();
    assert_eq!(p.to, recipient);
    assert_eq!(p.amount, 500);
    assert_eq!(p.proposed_at, 1_700_000_000);
    assert_eq!(p.execute_after, 1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
}

#[test]
fn execute_sweep_fails_before_window() {
    let env = Env::default();
    let (vault_addr, client, admin, recipient, _) = setup(&env);
    // Fund vault so the on-ledger transfer would succeed.
    let usdc = client.get_usdc_token();
    let stellar_admin = token::StellarAssetClient::new(&env, &usdc);
    stellar_admin.mint(&vault_addr, &1000);

    client.propose_sweep(&admin, &recipient, &500i128);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS - 1);
    let res = client.try_execute_sweep(&admin);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::TimelockNotExpired
    );
    assert!(client.get_pending_sweep().is_some());
}

#[test]
fn execute_sweep_succeeds_at_boundary_and_transfers() {
    let env = Env::default();
    let (vault_addr, client, admin, recipient, _) = setup(&env);
    let usdc = client.get_usdc_token();
    let stellar_admin = token::StellarAssetClient::new(&env, &usdc);
    stellar_admin.mint(&vault_addr, &1000);

    client.propose_sweep(&admin, &recipient, &500i128);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    client.execute_sweep(&admin);
    assert!(client.get_pending_sweep().is_none());
    let bal = token::Client::new(&env, &usdc).balance(&recipient);
    assert_eq!(bal, 500);
}

#[test]
fn execute_sweep_without_proposal_fails() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    env.ledger().set_timestamp(2_000_000_000);
    let res = client.try_execute_sweep(&admin);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::ProposalNotFound);
}

#[test]
fn propose_sweep_rejects_non_positive_amount() {
    let env = Env::default();
    let (_, client, admin, recipient, _) = setup(&env);
    let zero = client.try_propose_sweep(&admin, &recipient, &0i128);
    assert_eq!(zero.unwrap_err().unwrap(), VaultError::AmountNotPositive);
    let neg = client.try_propose_sweep(&admin, &recipient, &-1i128);
    assert_eq!(neg.unwrap_err().unwrap(), VaultError::AmountNotPositive);
}

#[test]
fn cancel_sweep_removes_proposal() {
    let env = Env::default();
    let (_, client, admin, recipient, _) = setup(&env);
    client.propose_sweep(&admin, &recipient, &500i128);
    client.cancel_sweep(&admin);
    assert!(client.get_pending_sweep().is_none());
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[test]
fn non_admin_cannot_propose_pause() {
    let env = Env::default();
    let (_, client, _, _, _) = setup(&env);
    let intruder = Address::generate(&env);
    let res = client.try_propose_pause(&intruder);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::Unauthorized);
}

#[test]
fn non_admin_cannot_execute_pause() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let intruder = Address::generate(&env);
    client.propose_pause(&admin);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    let res = client.try_execute_pause(&intruder);
    // With `mock_all_auths` the require_auth succeeds; the admin-check
    // inside `require_admin` is what must reject the intruder.
    assert_eq!(res.unwrap_err().unwrap(), VaultError::Unauthorized);
}

#[test]
fn non_admin_cannot_cancel_pause() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let intruder = Address::generate(&env);
    client.propose_pause(&admin);
    let res = client.try_cancel_pause(&intruder);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::Unauthorized);
}

#[test]
fn owner_alone_cannot_timelock_pause() {
    let env = Env::default();
    let (owner, client, _, _, _) = setup(&env);
    let res = client.try_propose_pause(&owner);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::Unauthorized);
}

// ---------------------------------------------------------------------------
// Concurrency: independent proposal slots
// ---------------------------------------------------------------------------

#[test]
fn pause_and_upgrade_can_be_proposed_concurrently() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[42u8; 32]);
    client.propose_pause(&admin);
    client.propose_upgrade(&admin, &hash);
    assert!(client.get_pending_pause().is_some());
    assert!(client.get_pending_upgrade().is_some());
}

#[test]
fn executing_pause_does_not_affect_pending_upgrade() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    let hash = BytesN::from_array(&env, &[43u8; 32]);
    client.propose_pause(&admin);
    client.propose_upgrade(&admin, &hash);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    client.execute_pause(&admin);
    assert!(client.is_paused());
    // Upgrade proposal is still queued.
    assert!(client.get_pending_upgrade().is_some());
}

// ---------------------------------------------------------------------------
// Wall-clock overflow
// ---------------------------------------------------------------------------

#[test]
fn propose_pause_rejects_timestamp_overflow() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    env.ledger().set_timestamp(u64::MAX);
    let res = client.try_propose_pause(&admin);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::TimelockOverflow);
    assert!(client.get_pending_pause().is_none());
}

#[test]
fn propose_upgrade_rejects_timestamp_overflow() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    env.ledger().set_timestamp(u64::MAX);
    let hash = BytesN::from_array(&env, &[15u8; 32]);
    let res = client.try_propose_upgrade(&admin, &hash);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::TimelockOverflow);
    assert!(client.get_pending_upgrade().is_none());
}

#[test]
fn propose_sweep_rejects_timestamp_overflow() {
    let env = Env::default();
    let (_, client, admin, recipient, _) = setup(&env);
    env.ledger().set_timestamp(u64::MAX);
    let res = client.try_propose_sweep(&admin, &recipient, &100i128);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::TimelockOverflow);
    assert!(client.get_pending_sweep().is_none());
}

// ---------------------------------------------------------------------------
// Re-execution after cancel/consume → must fail with ProposalNotFound
// ---------------------------------------------------------------------------

#[test]
fn execute_pause_after_cancel_fails() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    client.cancel_pause(&admin);
    env.ledger().set_timestamp(2_000_000_000);
    let res = client.try_execute_pause(&admin);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::ProposalNotFound);
}

#[test]
fn execute_pause_twice_fails_with_proposal_not_found() {
    let env = Env::default();
    let (_, client, admin, _, _) = setup(&env);
    client.propose_pause(&admin);
    env.ledger()
        .set_timestamp(1_700_000_000 + DEFAULT_TIMELOCK_SECONDS);
    client.execute_pause(&admin);
    // Second execute should fail because proposal was consumed.
    env.ledger().set_timestamp(2_000_000_000);
    let res = client.try_execute_pause(&admin);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::ProposalNotFound);
}

#[test]
fn cancel_pause_event_reports_whether_a_proposal_was_consumed() {
    let env = Env::default();
    let (vault_addr, client, admin, _, _) = setup(&env);
    // Cancel WITHOUT a proposal — event payload should be (no_proposal = true -> re-read:false).
    let topic: Symbol = Symbol::new(&env, "pause_cancelled");
    client.cancel_pause(&admin);
    let events = env.events().all();
    let last = events.last().expect("expected cancel event");
    assert_eq!(last.0, vault_addr);
    let t0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(t0, topic);
    let had_proposal: bool = last.2.into_val(&env);
    assert!(!had_proposal);

    // Cancel AFTER proposing — payload must now be true.
    client.propose_pause(&admin);
    client.cancel_pause(&admin);
    let events = env.events().all();
    let last = events.last().expect("expected second cancel event");
    assert_eq!(last.0, vault_addr);
    let t0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(t0, Symbol::new(&env, "pause_cancelled"));
    let had_proposal: bool = last.2.into_val(&env);
    assert!(had_proposal);
}

// ---------------------------------------------------------------------------
// Module-level helper coverage
// ---------------------------------------------------------------------------

#[test]
fn saturating_deadline_handles_boundary() {
    let env = Env::default();
    // zero window -> returns proposed_at unchanged
    assert_eq!(
        timelock::saturating_deadline(1_000, 0),
        Some(1_000)
    );
    // fits
    assert_eq!(
        timelock::saturating_deadline(100, 50),
        Some(150)
    );
    // exact max
    assert_eq!(
        timelock::saturating_deadline(u64::MAX - 5, 5),
        Some(u64::MAX)
    );
    // overflow -> None
    assert_eq!(
        timelock::saturating_deadline(u64::MAX, 1),
        None
    );
    assert_eq!(
        timelock::saturating_deadline(u64::MAX - 1, 5),
        None
    );
}
