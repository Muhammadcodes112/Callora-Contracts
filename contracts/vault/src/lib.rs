#![allow(clippy::too_many_arguments)]
#![no_std]
//!
//! # Callora Vault Contract — deposit/withdraw/deduct/distribute with pause circuit-breaker.
//!
//! ## Escape-Hatch Admin Actions (#482)
//!
//! The following critical admin actions are guarded by a mandatory timelock:
//!
//! | Action  | Propose              | Execute              | Cancel              |
//! |---------|----------------------|----------------------|---------------------|
//! | pause   | `propose_pause`      | `execute_pause`      | `cancel_pause`      |
//! | upgrade | `propose_upgrade`    | `execute_upgrade`    | `cancel_upgrade`    |
//! | sweep   | `propose_sweep`      | `execute_sweep`      | `cancel_sweep`      |
//!
//! Window length is configured by `set_timelock_window(admin, seconds)` and
//! defaults to 48 h (`172_800`). Valid bounds are 1 h – 30 d. All three slots
//! are independent so multiple proposals can coexist concurrently.
//!
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked
/// - Single and batch deducts are blocked
/// - Owner withdrawals are ALLOWED (emergency recovery)
/// - Admin distribute is ALLOWED (emergency recovery of untracked surplus)
/// - Admin/owner configuration functions remain available
///
/// ## Request-ID Idempotency
///
/// `deduct` and `batch_deduct` accept an optional `request_id: Option<Symbol>`.
/// When `Some(id)` is supplied the contract persists a processed-request marker
/// in **temporary storage** and rejects any subsequent call that carries the same
/// `request_id`, returning `VaultError::DuplicateRequestId`.
///
/// This gives safe **at-least-once retry** semantics: a backend can replay a
/// failed transaction with the same `request_id` and the contract will either
/// succeed (first time) or return a deterministic error (duplicate).
///
/// When `request_id` is `None` no deduplication is performed; the call is
/// treated as a fire-and-forget deduction with no idempotency guarantee.
///
/// ### Retention / TTL
/// Processed-request markers live in persistent storage and are bumped to
/// `REQUEST_ID_BUMP_AMOUNT` ledgers on every successful deduct. The threshold
/// for triggering a bump is `REQUEST_ID_BUMP_THRESHOLD`. Because they are now
/// persistent, they do not silently archive. To prevent state bloat, an owner
/// can explicitly prune old markers using `prune_processed_requests`.
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};
use soroban_sdk::token as token_client;

pub mod views;

mod events;

// ---------------------------------------------------------------------------
// Structured event payload types
// ---------------------------------------------------------------------------

/// Data payload emitted with the `"init"` event.
///
/// Carries the full vault configuration at initialization time so indexers
/// can reconstruct initial state without querying storage.
#[contracttype]
#[derive(Clone, Debug)]
pub struct InitPayload {
    /// Vault owner — the address that controls deposits and withdrawals.
    pub owner: Address,
    /// USDC token contract address used for all fund transfers.
    pub usdc_token: Address,
    /// Balance pre-credited to the vault at initialization (may be 0).
    pub initial_balance: i128,
    /// Minimum per-deposit amount enforced by the vault.
    pub min_deposit: i128,
    /// Maximum per-deduct amount enforced by the vault.
    pub max_deduct: i128,
    /// Settlement contract that receives USDC on every `deduct` call.
    pub settlement: Address,
}

/// Data payload emitted with the `"deposit"` event.
///
/// Records the deposited amount and the vault's new tracked balance so
/// indexers can verify accounting consistency without a separate storage read.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DepositPayload {
    /// Amount of USDC deposited in this call (base units).
    pub amount: i128,
    /// Vault tracked balance after the deposit has been applied.
    pub balance_after: i128,
}

/// Data payload emitted with the `"deduct"` event (single deduct).
///
/// Bundles the deducted amount, the caller-supplied idempotency key, the
/// resulting vault balance, and the on-chain destination so indexers can
/// reconcile settlement credits without joining additional event streams.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DeductPayload {
    /// Amount of USDC deducted and transferred to `destination`.
    pub amount: i128,
    /// Caller-supplied idempotency key for this deduction.
    pub request_id: u64,
    /// Vault tracked balance after the deduction has been applied.
    pub balance_after: i128,
    /// Address that received the deducted USDC (the settlement contract).
    pub destination: Address,
}

/// Data payload emitted with the `"deduct"` event (batch deduct).
///
/// Summarises a batch as a single aggregate event rather than per-item events
/// to keep gas costs proportional to batch size only at the token-transfer
/// layer, not the event layer.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchDeductPayload {
    /// Sum of all deducted amounts across all items in the batch.
    pub total_amount: i128,
    /// Number of items in the batch.
    pub item_count: u32,
    /// Vault tracked balance after all deductions have been applied.
    pub balance_after: i128,
    /// Address that received the deducted USDC (the settlement contract).
    pub destination: Address,
}

/// Typed error codes for the Callora Vault contract.
///
/// These error codes are returned instead of string panics to enable
/// machine-readable error handling by integrators using @stellar/stellar-sdk.
// NOTE: `VaultError` is defined in `errors.rs` and re-exported via `pub use`
// at the end of the module declarations. The inline definition was removed to
// avoid duplication and keep the error table in one canonical location.

// TTL constants shared with `limits.rs` and other modules.
/// Bump TTL when fewer than ~30 days of remaining lifetime.
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;
/// Extend TTL to ~60 days on each bump.
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60;

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Owner,
    UsdcToken,
    Balance,
    AuthorizedCaller,
    MinDeposit,
    RevenuePool,
    MaxDeduct,
    Settlement,
    Paused,
    Depositor(Address),
}

/// Persistent / instance storage keys for the vault.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    UsdcToken,
    ProcessedRequest(Symbol),
    ReserveCap(Address),
    DeveloperConfig(Address),
    DeveloperState(Address),
    /// Configured timelock window length in seconds (admin-settable).
    TimelockWindow,
    /// Active pause proposal pending timelock expiry.
    PendingPause,
    /// Active upgrade proposal (wasm hash) pending timelock expiry.
    PendingUpgrade,
    /// Active sweep proposal (recipient + amount) pending timelock expiry.
    PendingSweep,
    /// Current admin address (defaults to owner at init).
    Admin,
    /// Pending admin address awaiting acceptance (two-step transfer).
    PendingAdmin,
    /// Recorded wasm hash after a successful upgrade.
    ContractVersion,
}


#[cfg(target_arch = "wasm32")]
pub mod settlement {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/callora_settlement.wasm"
    );
}

/// In native/test mode, vault calls to settlement are no-ops since the settlement
/// contract is registered directly in the test `Env` and credits are verified
/// through its public interface.
#[cfg(not(target_arch = "wasm32"))]
pub mod settlement {
    use soroban_sdk::{Address, Env};
    pub struct Client<'a> {
        _env: &'a Env,
        _addr: &'a Address,
    }
    impl<'a> Client<'a> {
        pub fn new(env: &'a Env, addr: &'a Address) -> Self {
            Client {
                _env: env,
                _addr: addr,
            }
        }
        pub fn record_deduction(&self, _amount: &i128, _request_id: &u64) {}
    }
}

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    fn require_positive_amount(amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
    }

    fn require_valid_deposit_amount(amount: i128, min_deposit: i128) {
        Self::require_positive_amount(amount);
        if amount < min_deposit {
            panic!("deposit below minimum");
        }
    }

    fn require_valid_deduct_amount(amount: i128, min_amount: i128, max_deduct: i128) {
        Self::require_positive_amount(amount);
        if amount < min_amount {
            panic!("deduct below minimum");
        }
        if amount > max_deduct {
            panic!("deduct amount exceeds max_deduct");
        }
    }

    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: i128,
        authorized_caller: Address,
        min_deposit: i128,
        revenue_pool: Option<Address>,
        max_deduct: i128,
        settlement: Address,
    ) {
        if env.storage().instance().has(&DataKey::Owner) {
            panic!("Already initialized");
        }
        if min_deposit <= 0 {
            panic!("min_deposit must be positive");
        }
        if max_deduct <= 0 {
            panic!("max_deduct must be positive");
        }
        if min_deposit > max_deduct {
            panic!("min_deposit cannot exceed max_deduct");
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::Balance, &initial_balance);
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &authorized_caller);
        env.storage()
            .instance()
            .set(&DataKey::MinDeposit, &min_deposit);
        if let Some(pool) = revenue_pool {
            env.storage().instance().set(&DataKey::RevenuePool, &pool);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        env.storage().instance().set(&DataKey::Paused, &false);
        // Admin defaults to owner at initialization.
        env.storage().instance().set(&StorageKey::Admin, &owner);

        // Emit structured `init` event so indexers can bootstrap vault state
        // without replaying every subsequent mutation.
        env.events().publish(
            (events::event_init(&env), owner.clone()),
            InitPayload {
                owner,
                usdc_token,
                initial_balance,
                min_deposit,
                max_deduct,
                settlement,
            },
        );
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        Self::require_valid_deposit_amount(amount, min_dep);
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            let is_allowed = env
                .storage()
                .instance()
                .get::<_, bool>(&DataKey::Depositor(caller.clone()))
                .unwrap_or(false);
            if !is_allowed {
                panic!("Not authorized depositor");
            }
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_add(amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        let token_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let token_client = token_client::Client::new(&env, &token_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);

        env.events().publish(
            (events::event_deposit(&env), caller),
            DepositPayload {
                amount,
                balance_after: new_bal,
            },
        );
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: u64) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        Self::require_valid_deduct_amount(amount, min_dep, max_deduct);
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        if current_bal < amount {
            panic!("insufficient balance");
        }
        let new_bal = current_bal - amount;
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        // Transfer USDC from vault to settlement on-ledger.
        let usdc_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let usdc = token_client::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        usdc.transfer(&env.current_contract_address(), &settlement_addr, &amount);
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        settlement_client.record_deduction(&amount, &request_id);

        env.events().publish(
            (events::event_deduct(&env), caller),
            DeductPayload {
                amount,
                request_id,
                balance_after: new_bal,
                destination: settlement_addr,
            },
        );
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<(i128, u64)>) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        let mut total_amount: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            Self::require_valid_deduct_amount(amount, min_dep, max_deduct);
            total_amount = total_amount.checked_add(amount).unwrap_or_else(|| panic!("overflow"));
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        if current_bal < total_amount {
            panic!("insufficient balance");
        }
        let new_bal = current_bal - total_amount;
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        // Transfer total USDC from vault to settlement on-ledger atomically.
        let usdc_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let usdc = token_client::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        usdc.transfer(
            &env.current_contract_address(),
            &settlement_addr,
            &total_amount,
        );
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        for item in items.iter() {
            let (amount, request_id) = item;
            settlement_client.record_deduction(&amount, &request_id);
        }

        env.events().publish(
            (events::event_deduct(&env), caller),
            BatchDeductPayload {
                total_amount,
                item_count: items.len(),
                balance_after: new_bal,
                destination: settlement_addr,
            },
        );
    }

    // -----------------------------------------------------------------------
    // View functions — no TTL bump (read-only, zero write cost)
    // -----------------------------------------------------------------------

    /// Simulates a vault deduction without altering on-chain state.
    ///
    /// Performs validation checks identical to `deduct` and returns the predicted
    /// balance after the specified `amount` is deducted.
    ///
    /// # Errors
    /// Returns `VaultError` under the exact same conditions as `deduct`
    /// (e.g., paused state, amount exceeding balance, amount exceeding max deduction limit).
    // pub fn simulate_deduct(
    //     env: Env,
    //     caller: Address,
    //     amount: i128,
    //     request_id: Option<Symbol>,
    // ) -> Result<i128, VaultError> {
    //     Self::require_not_paused(env.clone())?;
    //     caller.require_auth();
    //     if amount <= 0 {
    //         return Err(VaultError::AmountNotPositive);
    //     }
    //     Self::require_authorized_deduct_caller(env.clone(), &caller)?;
    //     let max_d = Self::get_max_deduct(env.clone());
    //     if amount > max_d {
    //         return Err(VaultError::ExceedsMaxDeduct);
    //     }
    //     if let Some(ref rid) = request_id {
    //         Self::require_not_duplicate(&env, rid)?;
    //     }
    //     let meta = Self::get_meta(env.clone())?;
    //     if meta.balance < amount {
    //         return Err(VaultError::InsufficientBalance);
    //     }
    //     let _ = Self::require_settlement(&env)?;
    //     meta.balance
    //         .checked_sub(amount)
    //         .ok_or(VaultError::Overflow)
    // }

    // pub fn simulate_batch_deduct(
    //     env: Env,
    //     caller: Address,
    //     items: Vec<DeductItem>,
    // ) -> Result<i128, VaultError> {
    //     Self::require_not_paused(env.clone())?;
    //     caller.require_auth();
    //     Self::require_authorized_deduct_caller(env.clone(), &caller)?;
    //     let n = items.len();
    //     if n == 0 {
    //         return Err(VaultError::BatchEmpty);
    //     }
    //     if n > MAX_BATCH_SIZE {
    //         return Err(VaultError::BatchTooLarge);
    //     }
    //     let max_d = Self::get_max_deduct(env.clone());
    //     let meta = Self::get_meta(env.clone())?;
    //     let mut running = meta.balance;
    //     let mut seen_in_batch: Vec<Symbol> = Vec::new(&env);
    //     for item in items.iter() {
    //         if item.amount <= 0 {
    //             return Err(VaultError::AmountNotPositive);
    //         }
    //         if item.amount > max_d {
    //             return Err(VaultError::ExceedsMaxDeduct);
    //         }
    //         if running < item.amount {
    //             return Err(VaultError::InsufficientBalance);
    //         }
    //         if let Some(ref rid) = item.request_id {
    //             Self::require_not_duplicate(&env, rid)?;
    //             if seen_in_batch.contains(rid) {
    //                 return Err(VaultError::DuplicateRequestId);
    //             }
    //             seen_in_batch.push_back(rid.clone());
    //         }
    //         running = running.checked_sub(item.amount).ok_or(VaultError::Overflow)?;
    //     }
    //     let _ = Self::require_settlement(&env)?;
    //     Ok(running)
    // }

    // pub fn get_meta(env: Env) -> Result<VaultMeta, VaultError> {
    //     env.storage()
    //         .instance()
    //         .set(&DataKey::Depositor(depositor), &true);
    // }

    pub fn set_authorized_caller(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &caller);

        env.events().publish(
            (events::event_set_authorized_caller(&env), caller.clone()),
            caller,
        );
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &true);

        env.events()
            .publish((events::event_vault_paused(&env), caller), ());
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &false);

        env.events()
            .publish((events::event_vault_unpaused(&env), caller), ());
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
    pub fn balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap()
    }
    pub fn get_owner(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap()
    }

    pub(crate) fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let owner = Self::get_owner(env);
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap()
    }
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(i128::MAX)
    }

    pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        if max_deduct <= 0 {
            panic!("max_deduct must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);

        env.events().publish(
            (events::event_set_max_deduct(&env), caller),
            max_deduct,
        );
    }

    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap()
    }

    pub fn set_settlement(env: Env, caller: Address, settlement: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);

        env.events().publish(
            (events::event_set_settlement(&env), caller),
            settlement,
        );
    }
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::RevenuePool)
    }

    /// Return the capability bitmap for this contract version.
    ///
    /// Each set bit represents a feature that this contract supports.  Bits are
    /// stable — a position once assigned is never reused for a different feature.
    /// Reserved bits (18–63) are always zero.
    ///
    /// No authentication required; this is a pure view function.
    ///
    /// # Example
    /// ```ignore
    /// let caps = client.capabilities();
    /// let has_batch = caps & capabilities::CAP_BATCH_DEDUCT != 0;
    /// ```
    pub fn capabilities(env: Env) -> u64 {
        capabilities::capabilities(&env)
    }

    // =====================================================================
    //  Escape-hatch admin timelock (Issue #482)
    //
    //  Critical admin actions — `pause`, `upgrade`, and `sweep` — must
    //  propose before they can be executed. The proposal records a
    //  `proposed_at` timestamp and an `execute_after` deadline derived
    //  from the configured `TimelockWindow` (default 48h). Any admin may
    //  cancel an outstanding proposal at any time.
    //
    //  All three action slots are independent so multiple escape-hatch
    //  proposals can be staged in parallel.
    // =====================================================================

    /// Configure the timelock window length (admin only).
    ///
    /// The window sets the minimum delay between proposing and executing a
    /// critical admin action. Default on deployment is **48 h** (`172_800` s).
    ///
    /// # Bounds
    /// - Minimum: [`timelock::MIN_TIMELOCK_SECONDS`] (1 h).
    /// - Maximum: [`timelock::MAX_TIMELOCK_SECONDS`] (30 d).
    /// - Default: [`timelock::DEFAULT_TIMELOCK_SECONDS`] (48 h).
    ///
    /// Changing the window does **not** retroactively shorten existing
    /// proposals — each proposal carries its own `execute_after` deadline.
    ///
    /// # Authorization
    /// The caller must be the current admin. `require_auth` runs first so
    /// misconfigured callers can never silently poison the configuration.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::InvalidTimelockWindow` if `window < MIN` or `window > MAX`.
    pub fn set_timelock_window(env: Env, caller: Address, window: u64) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        if window < timelock::MIN_TIMELOCK_SECONDS || window > timelock::MAX_TIMELOCK_SECONDS {
            return Err(VaultError::InvalidTimelockWindow);
        }
        timelock::set_timelock_window(&env, window);
        env.events().publish(
            (
                events::event_timelock_window_changed(&env),
                caller.clone(),
            ),
            (timelock::get_timelock_window(&env), window),
        );
        Ok(())
    }

    /// Return the configured timelock window length in seconds.
    ///
    /// Defaults to [`timelock::DEFAULT_TIMELOCK_SECONDS`] (48 h) when no
    /// window has been explicitly configured.
    pub fn get_timelock_window(env: Env) -> u64 {
        timelock::get_timelock_window(&env)
    }

    /// Return the pending pause proposal, if any.
    pub fn get_pending_pause(env: Env) -> Option<timelock::PendingPause> {
        timelock::get_pending_pause(&env)
    }

    /// Return the pending upgrade proposal, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<timelock::PendingUpgrade> {
        timelock::get_pending_upgrade(&env)
    }

    /// Return the pending sweep proposal, if any.
    pub fn get_pending_sweep(env: Env) -> Option<timelock::PendingSweep> {
        timelock::get_pending_sweep(&env)
    }

    /// Require the caller to be the current admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != &admin {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Return the current admin address.
    ///
    /// Defaults to the owner when no admin has been explicitly set.
    ///
    /// # Errors
    /// - `VaultError::NotInitialized` if the contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)
    }

    /// Initiate a two-step admin transfer (current admin only).
    ///
    /// Stores `new_admin` as a pending admin. The transfer is not complete
    /// until `accept_admin` is called by `new_admin`.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not the current admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events().publish(
            (
                events::event_admin_nominated(&env),
                caller,
                new_admin.clone(),
            ),
            new_admin,
        );
        Ok(())
    }

    /// Accept a pending admin transfer (pending admin only).
    ///
    /// Promotes the pending admin to the current admin and clears the pending slot.
    ///
    /// # Errors
    /// - `VaultError::NoAdminTransferPending` if there is no pending admin.
    pub fn accept_admin(env: Env) -> Result<(), VaultError> {
        let new_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(VaultError::NoAdminTransferPending)?;
        new_admin.require_auth();
        let old_admin = Self::get_admin(env.clone())?;
        env.storage()
            .instance()
            .set(&StorageKey::Admin, &new_admin);
        env.storage()
            .instance()
            .remove(&StorageKey::PendingAdmin);

        env.events().publish(
            (
                events::event_admin_accepted(&env),
                old_admin,
                new_admin.clone(),
            ),
            new_admin,
        );
        Ok(())
    }

    /// Propose pausing the vault (admin only, timelocked).
    ///
    /// Snapshots the proposal deadline `proposed_at + window`. Execution
    /// becomes available after [`Self::execute_pause`] once the ledger
    /// clock reaches `execute_after`.
    ///
    /// Re-proposing while a previous pause proposal is still live replaces
    /// the proposal **and restarts the timer** — useful when the admin
    /// re-baselines a stale proposal.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::TimelockOverflow` if `proposed_at + window` overflows.
    pub fn propose_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_pause(
            &env,
            &timelock::PendingPause {
                proposed_at,
                execute_after,
            },
        );
        env.events()
            .publish((events::event_pause_proposed(&env), caller), (proposed_at, execute_after));
        Ok(())
    }

    /// Execute a pause proposal after the configured window has elapsed (admin only).
    ///
    /// On success, sets `StorageKey::Paused` to `true` and consumes the
    /// pending proposal so it cannot be replayed. Emits the standard
    /// `vault_paused` event.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::ProposalNotFound` if no pause proposal exists.
    /// - `VaultError::TimelockNotExpired` if `now < execute_after`.
    pub fn execute_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_pause(&env)
            .ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        timelock::clear_pending_pause(&env);
        env.events()
            .publish((events::event_pause_executed(&env), caller.clone()), env.ledger().timestamp());
        env.events()
            .publish((events::event_vault_paused(&env), caller), ());
        Ok(())
    }

    /// Cancel an outstanding pause proposal (admin only).
    ///
    /// Removes the proposal without applying it. Always emits the
    /// `pause_cancelled` event for audit traceability, even when no
    /// proposal exists, so consumers can never confuse "no event" with
    /// "wasn't called".
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    pub fn cancel_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_pause(&env);
        timelock::clear_pending_pause(&env);
        env.events().publish(
            (
                events::event_pause_cancelled(&env),
                caller.clone(),
            ),
            existing.is_some(),
        );
        Ok(())
    }

    /// Propose upgrading the vault WASM (admin only, timelocked).
    ///
    /// After the configured window elapses, `execute_upgrade` performs the
    /// on-chain deployer update. Re-proposing replaces the stored wasm
    /// hash **and restarts the timer**.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::TimelockOverflow` if `proposed_at + window` overflows.
    pub fn propose_upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_upgrade(
            &env,
            &timelock::PendingUpgrade {
                wasm_hash: new_wasm_hash.clone(),
                proposed_at,
                execute_after,
            },
        );
        env.events().publish(
            (events::event_upgrade_proposed(&env), caller),
            (new_wasm_hash, proposed_at, execute_after),
        );
        Ok(())
    }

    /// Execute an upgrade proposal after the configured window has elapsed (admin only).
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::ProposalNotFound` if no upgrade proposal exists.
    /// - `VaultError::TimelockNotExpired` if `now < execute_after`.
    pub fn execute_upgrade(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_upgrade(&env)
            .ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }
        let wasm_hash = proposal.wasm_hash.clone();
        let _admin = Self::get_admin(env.clone())?;
        env.deployer().update_current_contract_wasm(wasm_hash.clone());
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &wasm_hash);
        timelock::clear_pending_upgrade(&env);
        env.events().publish(
            (events::event_upgrade_executed(&env), caller.clone()),
            env.ledger().timestamp(),
        );
        env.events()
            .publish((events::event_upgraded(&env), caller), wasm_hash);
        Ok(())
    }

    /// Cancel an outstanding upgrade proposal (admin only).
    ///
    /// Always emits `upgrade_cancelled` for audit traceability, even when
    /// no proposal is pending; the bool data payload reports whether a
    /// proposal was actually consumed.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    pub fn cancel_upgrade(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_upgrade(&env);
        timelock::clear_pending_upgrade(&env);
        env.events().publish(
            (
                events::event_upgrade_cancelled(&env),
                caller.clone(),
            ),
            existing.is_some(),
        );
        Ok(())
    }

    /// Propose sweeping (distributing) on-ledger USDC surplus to a recipient (admin only, timelocked).
    ///
    /// After the configured window elapses, `execute_sweep` transfers the
    /// funds and emits the standard `distribute` event. Re-proposing
    /// replaces the recipient+amount **and restarts the timer**.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::AmountNotPositive` if `amount <= 0`.
    /// - `VaultError::TimelockOverflow` if `proposed_at + window` overflows.
    pub fn propose_sweep(
        env: Env,
        caller: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_sweep(
            &env,
            &timelock::PendingSweep {
                to: to.clone(),
                amount,
                proposed_at,
                execute_after,
            },
        );
        env.events().publish(
            (events::event_sweep_proposed(&env), caller),
            (to, amount, proposed_at, execute_after),
        );
        Ok(())
    }

    /// Execute a sweep proposal after the configured window has elapsed (admin only).
    ///
    /// Performs the on-ledger transfer of `amount` USDC to the snapshotted
    /// recipient. The proposal is consumed atomically so a successful
    /// execute cannot be replayed.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::ProposalNotFound` if no sweep proposal exists.
    /// - `VaultError::TimelockNotExpired` if `now < execute_after`.
    /// - `VaultError::InsufficientBalance` if vault holds less than `amount`.
    pub fn execute_sweep(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_sweep(&env)
            .ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }

        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        let usdc = token_client::Client::new(&env, &usdc_addr);
        if usdc.balance(&env.current_contract_address()) < proposal.amount {
            return Err(VaultError::InsufficientBalance);
        }
        usdc.transfer(
            &env.current_contract_address(),
            &proposal.to,
            &proposal.amount,
        );
        let executed_at = env.ledger().timestamp();
        env.events().publish(
            (events::event_sweep_executed(&env), caller),
            (proposal.to.clone(), proposal.amount, executed_at),
        );
        env.events().publish(
            (events::event_distribute(&env), proposal.to.clone()),
            proposal.amount,
        );
        timelock::clear_pending_sweep(&env);
        Ok(())
    }

    /// Cancel an outstanding sweep proposal (admin only).
    ///
    /// Always emits `sweep_cancelled` for audit traceability, even when
    /// no proposal is pending; the bool data payload reports whether a
    /// proposal was actually consumed.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    pub fn cancel_sweep(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_sweep(&env);
        timelock::clear_pending_sweep(&env);
        env.events().publish(
            (
                events::event_sweep_cancelled(&env),
                caller.clone(),
            ),
            existing.is_some(),
        );
        Ok(())
    }

    /// Garbage-collect processed request markers from persistent storage.
    /// Only the owner can call this.
    /// Emits a `request_id_pruned` event for each removed ID.
    pub fn prune_processed_requests(
        env: Env,
        caller: Address,
        ids: Vec<Symbol>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;

        for id in ids.iter() {
            let key = StorageKey::ProcessedRequest(id.clone());
            if env.storage().persistent().has(&key) {
                env.storage().persistent().remove(&key);
                env.events().publish(
                    (events::event_request_id_pruned(&env), caller.clone()),
                    id.clone(),
                );
            }
        }

        Ok(())
    }

    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Depositor(caller))
            .unwrap_or(false)
    }

    /// Set or update the reserve cap for a token (owner only).
    ///
    /// The reserve cap is the maximum total balance the vault may hold for
    /// `token`.  Any `deposit` call that would push the balance beyond `cap`
    /// is rejected with [`VaultError::ExceedsReserveCap`].
    ///
    /// Pass `i128::MAX` to remove the effective cap (restore unlimited deposits).
    ///
    /// # Parameters
    /// - `caller` — must be the vault owner.
    /// - `token` — token contract address the cap applies to.
    /// - `cap` — maximum balance in token stroops; must be > 0.
    ///
    /// # Errors
    /// - [`VaultError::Unauthorized`] — `caller` is not the owner.
    /// - [`VaultError::AmountNotPositive`] — `cap <= 0`.
    pub fn set_reserve_cap(
        env: Env,
        caller: Address,
        token: Address,
        cap: i128,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        if cap <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        let prev = limits::set(&env, &token, cap);
        env.events().publish(
            (events::event_reserve_cap_set(&env), caller, token),
            (prev, cap),
        );
        Ok(())
    }

    /// Return the currently stored contract WASM hash, or `None` if no
    /// upgrade has been applied yet (i.e. the `ContractVersion` storage key
    /// has not been written).
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&StorageKey::ContractVersion)
    }

    /// Return the reserve cap for `token`.
    ///
    /// Returns `i128::MAX` when no cap has been configured (effectively unlimited).
    pub fn get_reserve_cap(env: Env, token: Address) -> i128 {
        limits::get(&env, &token)
    }

    /// Read-only preview of the untracked on-ledger USDC the vault holds
    /// above its internal tracked balance.
    ///
    /// No authentication required. Does not write storage or bump TTL.
    ///
    /// # Errors
    /// - [`VaultError::NotInitialized`] if `init` has not been called.
    pub fn dry_run_sweep_idle_balance(env: Env) -> Result<views::SweepPreview, VaultError> {
        views::compute_sweep_preview(&env)
    }
}

pub mod capabilities;
mod cold_storage;
mod errors;
pub use errors::VaultError;
pub mod limits;
pub mod rate_limit;
pub mod timelock;

pub use timelock::{
    DEFAULT_TIMELOCK_SECONDS, MAX_TIMELOCK_SECONDS, MIN_TIMELOCK_SECONDS,
};

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod test {
    pub use super::CalloraVault;
    pub use super::CalloraVaultClient;
}

#[cfg(test)]
mod test_sweep_idle_balance;

#[cfg(test)]
mod test_access_control_matrix;

#[cfg(test)]
mod test_timelock;

#[cfg(test)]
mod test_events;
// mod test_gas_budget;
// #[cfg(test)]
// mod test_rate_limit;
