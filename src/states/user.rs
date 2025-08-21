use pinocchio::{account_info::{AccountInfo, Ref, RefMut}, program_error::ProgramError, pubkey::Pubkey, *};

#[derive(Debug)]
pub struct UserAccount {
    pub owner: Pubkey, // Trader's wallet
    pub margin_balance: u64, // Deposited collateral (USDC)
    pub open_positions: [Pubkey; 10] // References to Position accounts
} 

impl UserAccount {
    pub const SIZE: usize = 32 + 8 + (10 * 32);

    pub fn from_account_info(account: &AccountInfo) -> Result<Ref<Self>, ProgramError> {
        if account.data_len() != Self::SIZE {  
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Ref::map(account.try_borrow_data()?, |data| unsafe {
            *(data.as_ptr() as *const &Self)
        }))
    }

    pub fn from_account_info_mut(account: &AccountInfo) -> Result<RefMut<Self>, ProgramError> {
        if account.data_len() != Self::SIZE {  
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(RefMut::map(account.try_borrow_mut_data()?, |data| unsafe {
            &mut *(data.as_mut_ptr() as *mut Self)
        }))
    }
}