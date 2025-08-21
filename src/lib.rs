use pinocchio::{account_info::AccountInfo, pubkey::Pubkey,program_error::ProgramError, *};
use pinocchio_pubkey::declare_id;

use crate::instructions::{initialize_market, initialize_user_account, open_position, process_open_position, PerpetualInstructions};

entrypoint!(process_instruction);

declare_id!("BXacY2xWwx7ogSa1CnvrdXxAigBMwwszoZf4Q98E2YoV");

pub mod instructions;
pub mod states;

pub fn process_instruction(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8]
) -> ProgramResult {

    let (ix_disc, instruction_data) = instruction_data.split_first().ok_or(ProgramError::InvalidInstructionData)?;

    match PerpetualInstructions::try_from(ix_disc)? {
        PerpetualInstructions::InitializeMarket => initialize_market(accounts, instruction_data)?,
        PerpetualInstructions::InitializeUser => initialize_user_account(accounts)?,
        PerpetualInstructions::OpenPosition => process_open_position(accounts, instruction_data)?,
    }
    
    Ok(())
}
