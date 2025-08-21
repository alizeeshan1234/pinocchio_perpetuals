use pinocchio::{account_info::{AccountInfo, Ref, RefMut}, program_error::ProgramError, pubkey::Pubkey, ProgramResult};

#[derive(Debug, Clone, Copy)]
pub struct Market {
    pub is_initialized: bool,
    pub market_id: u8,
    pub market_symbol: [u8; 16], // Human-readable market name SOL-PERP
    pub oracle: Pubkey, // Price oracle account
    pub collateral_mint: Pubkey, //The SPL Token used for collateral/margin
    pub collateral_vault: Pubkey, // Vault holding collateral for this market
    pub base_oracle: Pubkey, //Public key of an oracle account (e.g., Pyth price feed).

    // Risk parameters
    // Minimum % margin required to open a position (e.g., 10%).
    pub initial_margin: u64,     // % margin needed to open a position
    // Minimum % margin to keep it alive (e.g., 5%).
    pub maintenance_margin: u64, // % margin required to avoid liquidation

    pub max_leverage: u64, // Maximum leverage allowed (e.g., 10x = 1000 bps)
    // How much is charged per trade (e.g., 10 bps = 0.1%).
    pub fee_rate: u64,           // trading fees

    // Funding mechanics
    // Periodic payment rate between longs & shorts.
    pub funding_rate: i64,       // current funding rate (basis points)
    // Last time funding was settled.
    pub last_funding_time: i64,  // last timestamp when funding was settled
    // How often settlement happens
    pub funding_interval: i64,   // e.g. every 8 hours

    // Open interest tracking
    // Aggregate size of all long positions.
    pub open_interest_long: u64, // total size of all long positions
    //Aggregate size of all short positions.
    pub open_interest_short: u64,// total size of all short positions

    // Collateral stats
    // Total margin locked in the market.
    pub total_collateral: u64,   // total margin collateral in this market
    // Snapshot of system-wide unrealized profit/loss.
    pub unrealized_pnl: i128,    // system-wide PnL snapshot

    // Admin/governance authority
    // Governance/admin that can adjust parameters
    pub authority: Pubkey,

    /// PDA bump for address derivation
    pub bump: u8,

    pub collateral_bump: u8, // PDA bump for collateral vault
}

impl Market {
    // pub const SIZE: usize = 1 + 1 + 16 + (3 * 32) + (6 * 8) + (3 * 8) + 16 + 1;
    pub const SIZE: usize = core::mem::size_of::<Self>();

    pub fn from_account_info(account: &AccountInfo) -> Result<Ref<Self>, ProgramError> {
        if account.data_len() < Self::SIZE {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Ref::map(account.try_borrow_data()?, |data| unsafe {
            *(data.as_ptr() as *const &Self)
        }))
    }

    pub fn from_account_info_mut(account: &AccountInfo) -> Result<RefMut<Self>, ProgramError> {
        if account.data_len() < Self::SIZE {
            return Err(ProgramError::InvalidAccountData);
        };

        Ok(RefMut::map(account.try_borrow_mut_data()?, |data| unsafe {
            &mut *(data.as_mut_ptr() as *mut Self)
        }))
    }
}