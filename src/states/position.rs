use pinocchio::{pubkey::Pubkey, account_info::{AccountInfo, Ref, RefMut}, program_error::ProgramError,*};

pub struct Position {
    /*The wallet public key (on Solana) that owns this position.
    Every position is tied to a specific user.*/
    pub user: Pubkey,

    /*The market (trading pair) where this position belongs.
    Could be the address of a Market account that stores global data for SOL/USDC perp*/
    pub market: Pubkey,

    /*The position size, i.e. how many contracts this user holds.
    Sign encodes direction:
        +size = Long
        -size = Short
    Example: +10 = long 10 SOL contracts, -5 = short 5 contracts.*/
    pub size: i128,

    /*The average price at which the user entered this position. 
    Used to calculate PnL.
    Example: If user longed 10 SOL at $20.50 → entry_price = 20_500_000.*/
    pub entry_price: u64, 

    /*The collateral the trader locked for this position.
    This protects against liquidation. */
    pub margin: u64,

    /*Unrealized profit or loss (PnL) if the position were closed at current price.*/
    pub unrealized_pnl: i64,

    /*Tracks funding rate adjustments between longs and shorts.
    Tracks funding rate adjustments between longs and shorts.
    In perpetuals, funding payments keep the perpetual price close to spot.
        If perpetual > spot, longs pay shorts.
        If perpetual < spot, shorts pay longs. 
    */
    pub funding_payment: i64,

    /*Timestamp of the last funding settlement for this position.
    Funding is usually settled every 8 hours (depends on protocol). */
    pub last_funding_settlement: i64,

    /*Whether this position is currently open or closed.
        True = open position
        False = position closed
     */
    pub is_active: bool, 
}

#[repr(u8)]
pub enum PositionType {
    Long = 0,
    Short = 1,
    Flat = 2, // No position
}

impl Position {
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
        }

        Ok(RefMut::map(account.try_borrow_mut_data()?, |data| unsafe {
            &mut *(data.as_mut_ptr() as *mut Self)
        }))
    }

    pub fn position_type(&self) -> PositionType {
        if self.size > 0 {
            PositionType::Long
        } else if self.size < 0 {
            PositionType::Short
        } else {
            PositionType::Flat
        }
    }

    pub fn is_long(&self) -> bool {
        self.size > 0
    }

    pub fn is_short(&self) -> bool {
        self.size < 0
    }

    pub fn is_open(&self) -> bool {
        self.is_active
    }
}
