use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Vault contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers may branch on these `u32` codes instead
/// of parsing panic strings.
///
/// | Code | Variant                        | Meaning                                                  |
/// |------|--------------------------------|----------------------------------------------------------|
/// | 1    | NotInitialized                 | Vault has not been initialized                           |
/// | 2    | AlreadyInitialized             | `init` was called more than once                         |
/// | 3    | Unauthorized                   | Caller is not authorized for the operation               |
/// | 4    | Paused                         | State-changing action is blocked while paused            |
/// | 5    | InsufficientBalance            | Vault balance is too low for the requested operation     |
/// | 6    | AmountNotPositive              | Amount must be greater than zero                         |
/// | 7    | ExceedsMaxDeduct               | Deduct amount exceeds the configured cap                 |
/// | 8    | BelowMinDeposit                | Deposit amount is below the configured minimum           |
/// | 9    | Overflow                       | Arithmetic overflow was detected                         |
/// | 10   | InitialBalanceNegative         | Initial balance must be non-negative                     |
/// | 11   | MinDepositNotPositive          | Minimum deposit must be greater than zero                |
/// | 12   | MaxDeductNotPositive           | Maximum deduct must be greater than zero                 |
/// | 13   | MinDepositExceedsMaxDeduct     | Minimum deposit cannot exceed maximum deduct             |
/// | 14   | UsdcTokenCannotBeVault         | USDC token address cannot be the vault contract          |
/// | 15   | RevenuePoolCannotBeVault       | Revenue pool address cannot be the vault contract        |
/// | 16   | AuthorizedCallerCannotBeVault  | Authorized caller cannot be the vault contract           |
/// | 17   | InitialBalanceExceedsOnLedger  | Initial tracked balance exceeds on-ledger USDC           |
/// | 18   | AlreadyPaused                  | Contract is already paused                               |
/// | 19   | NotPaused                      | Contract is not paused                                   |
/// | 20   | SettlementNotSet               | Settlement address has not been configured               |
/// | 21   | BatchEmpty                     | Batch deduct received no items                           |
/// | 22   | BatchTooLarge                  | Batch deduct exceeds the maximum allowed size            |
/// | 23   | NewOwnerSameAsCurrent          | Proposed owner matches the current owner                 |
/// | 24   | NoOwnershipTransferPending     | No ownership transfer is pending                         |
/// | 25   | NoAdminTransferPending         | No admin transfer is pending                             |
/// | 26   | OfferingIdTooLong              | Offering ID exceeds the maximum length                   |
/// | 27   | MetadataTooLong                | Metadata exceeds the maximum length                      |
/// | 28   | PriceParseError                | Price is invalid or non-positive                         |
/// | 29   | DuplicateRequestId             | Request ID has already been processed                    |
/// | 30   | OfferingIdInvalid              | Offering ID is empty or contains invalid characters      |
/// | 31   | MetadataInvalid                | Metadata is empty or contains invalid characters         |
/// | 32   | StaleNonce                     | Rotation nonce does not match the stored current nonce   |
/// | 33   | NewRevenuePoolSameAsCurrent    | Proposed revenue pool matches the current revenue pool   |
/// | 34   | NoRevenuePoolTransferPending   | No revenue-pool transfer is pending                      |
/// | 35   | Slippage                       | Fee basis points exceeds caller limit                    |
/// | 36   | RateLimited                    | Developer rate limit has been exceeded                   |
/// | 37   | PausedState                    | Operation is rejected because the vault is paused        |
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum VaultError {
    /// Vault has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Vault has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// Vault is currently paused (code 4).
    Paused = 4,
    /// Insufficient balance for the requested operation (code 5).
    InsufficientBalance = 5,
    /// Amount must be positive (code 6).
    AmountNotPositive = 6,
    /// Deduct amount exceeds the configured maximum (code 7).
    ExceedsMaxDeduct = 7,
    /// Deposit amount is below the configured minimum (code 8).
    BelowMinDeposit = 8,
    /// Arithmetic overflow detected (code 9).
    Overflow = 9,
    /// Initial balance must be non-negative (code 10).
    InitialBalanceNegative = 10,
    /// Min deposit must be positive (code 11).
    MinDepositNotPositive = 11,
    /// Max deduct must be positive (code 12).
    MaxDeductNotPositive = 12,
    /// Min deposit cannot exceed max deduct (code 13).
    MinDepositExceedsMaxDeduct = 13,
    /// USDC token address cannot be the vault address (code 14).
    UsdcTokenCannotBeVault = 14,
    /// Revenue pool address cannot be the vault address (code 15).
    RevenuePoolCannotBeVault = 15,
    /// Authorized caller address cannot be the vault address (code 16).
    AuthorizedCallerCannotBeVault = 16,
    /// Initial balance exceeds on-ledger USDC balance (code 17).
    InitialBalanceExceedsOnLedger = 17,
    /// Vault is already paused (code 18).
    AlreadyPaused = 18,
    /// Vault is not paused (code 19).
    NotPaused = 19,
    /// Settlement address has not been configured (code 20).
    SettlementNotSet = 20,
    /// Batch deduct requires at least one item (code 21).
    BatchEmpty = 21,
    /// Batch size exceeds maximum allowed (code 22).
    BatchTooLarge = 22,
    /// New owner must be different from current owner (code 23).
    NewOwnerSameAsCurrent = 23,
    /// No ownership transfer is pending (code 24).
    NoOwnershipTransferPending = 24,
    /// No admin transfer is pending (code 25).
    NoAdminTransferPending = 25,
    /// Offering ID exceeds maximum length (code 26).
    OfferingIdTooLong = 26,
    /// Metadata exceeds maximum length (code 27).
    MetadataTooLong = 27,
    /// Price parsing error or non-positive price (code 28).
    PriceParseError = 28,
    /// Duplicate request ID detected (code 29).
    DuplicateRequestId = 29,
    /// Offering ID is empty or contains invalid characters (code 30).
    OfferingIdInvalid = 30,
    /// Metadata string is empty or contains invalid characters (code 31).
    MetadataInvalid = 31,
    /// Supplied nonce does not match the stored authorized-caller rotation nonce (code 32).
    StaleNonce = 32,
    /// New revenue pool must be different from current revenue pool (code 33).
    NewRevenuePoolSameAsCurrent = 33,
    /// No revenue pool transfer is pending (code 34).
    NoRevenuePoolTransferPending = 34,
    /// Calculated fee in basis points exceeds the caller-supplied `max_fee_bps` limit (code 35).
    Slippage = 35,
    /// Rate limit exceeded for the developer (code 36).
    RateLimited = 36,
    /// Operation is rejected because the vault is paused (code 37).
    PausedState = 37,
    /// Hot BPS must be between 1 and 10000 (code 38).
    InvalidHotBps = 38,
    /// Rebalance threshold must be between 1 and 10000 (code 39).
    InvalidRebalanceThreshold = 39,
    /// Cold signer set cannot be empty (code 40).
    ColdSignersEmpty = 40,
    /// Cold threshold must be between 1 and signer count (code 41).
    InvalidColdThreshold = 41,
    /// Duplicate address found in cold signer set (code 42).
    DuplicateColdSigner = 42,
    /// Deposit would exceed the configured reserve cap (code 43).
    ExceedsReserveCap = 43,
    /// No pending timelock proposal for the requested action (code 44).
    ProposalNotFound = 44,
    /// Action attempted before the timelock window has elapsed (code 45).
    TimelockNotExpired = 45,
    /// `proposed_at + window` overflowed `u64` (code 46).
    TimelockOverflow = 46,
    /// Proposed timelock window is outside the allowed `MIN..=MAX` bounds (code 47).
    InvalidTimelockWindow = 47,
}
