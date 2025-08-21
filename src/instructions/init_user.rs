use pinocchio::{account_info::AccountInfo, instruction::Signer, program_error::ProgramError, pubkey::Pubkey, sysvars::{rent::Rent, Sysvar}, *};
use pinocchio_system::instructions::CreateAccount;
use crate::states::UserAccount;

pub fn initialize_user_account(accounts: &[AccountInfo]) -> ProgramResult {

    let [user, user_account, system_program] = accounts else {
        return Err(ProgramError::InvalidAccountData);
    };

    if !user.is_signer() {
        return Err(ProgramError::InvalidAccountData);
    };

    let (user_account_pda, bump) = pubkey::find_program_address(
        &[b"user_account", user.key().as_ref()],
        &crate::ID
    );

    if *user_account.key() != user_account_pda {
        return Err(ProgramError::InvalidAccountData);
    };

    let pda_ref = &[bump];
    let seeds = seeds!(
        b"user_account",
        user.key().as_ref(),
        pda_ref
    );

    let signer_seeds = Signer::from(&seeds);

    if user_account.data_is_empty() {
        println!("Initializing User Account!");

        let lamports = Rent::get()?.minimum_balance(UserAccount::SIZE);

        CreateAccount {
            from: user,
            to: user_account,
            lamports,
            space: UserAccount::SIZE as u64,
            owner: &crate::ID
        }.invoke_signed(&[signer_seeds])?;

        let mut user_account_info_mut = UserAccount::from_account_info_mut(user_account)?;

        user_account_info_mut.owner = *user.key();
        user_account_info_mut.margin_balance = 0;
        user_account_info_mut.open_positions = [Pubkey::default(); 10];

        msg!("User account initialized");
    } else {
        msg!("User account already initialized");
    }

    Ok(())
}

// =========================== TESTING initialize_user_account ===========================

// #[cfg(test)]
// mod testing {
//     use mollusk_svm::{Mollusk, result::Check, program};
//     use solana_sdk::{
//         account::Account,
//         instruction::{AccountMeta, Instruction},
//         pubkey::Pubkey,
//         program_error::ProgramError,
//     };

//     const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("BXacY2xWwx7ogSa1CnvrdXxAigBMwwszoZf4Q98E2YoV");
//     const USER: Pubkey = Pubkey::new_from_array([2u8; 32]);

//     #[test]
//     fn test_initialize_user_account() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (user_account_pda, bump) = Pubkey::find_program_address(
//             &[b"user_account", USER.as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();

//         let instruction_data = vec![1u8]; // Remove the mut and assignment

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(USER, true),
//                 AccountMeta::new(user_account_pda, false),
//                 AccountMeta::new_readonly(system_program_id, false),
//             ],
//             data: instruction_data,
//         };

//         let user_account_lamports = solana_sdk::native_token::LAMPORTS_PER_SOL; // Give more lamports
//         let user = Account {
//             lamports: user_account_lamports,
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let user_account = Account {
//             lamports: 0,
//             data: vec![], // Empty data for uninitialized account
//             owner: solana_sdk::system_program::id(), // Starts as system owned
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (USER, user),
//                 (user_account_pda, user_account),
//                 (system_program_id, system_account),
//             ],
//             &[Check::success()],
//         );
//     }

//     #[test]
//     fn test_initialize_user_account_already_initialized() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (user_account_pda, bump) = Pubkey::find_program_address(
//             &[b"user_account", USER.as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();

//         let instruction_data = vec![1u8];

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(USER, true),
//                 AccountMeta::new(user_account_pda, false),
//                 AccountMeta::new_readonly(system_program_id, false),
//             ],
//             data: instruction_data,
//         };

//         let user_account_lamports = solana_sdk::native_token::LAMPORTS_PER_SOL; // Give more lamports
//         let user = Account {
//             lamports: user_account_lamports,
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let user_account = Account {
//             lamports: 0,
//             data: vec![], // Pre-populate with valid size
//             owner: PROGRAM_ID, // Set to the program ID to indicate initialized
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (USER, user),
//                 (user_account_pda, user_account),
//                 (system_program_id, system_account),
//             ],
//             &[Check::success()],
//         );
//     }

//     #[test]
//     fn test_initialize_user_account_invalid_signer() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (user_account_pda, bump) = Pubkey::find_program_address(
//             &[b"user_account", USER.as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();

//         let instruction_data = vec![1u8];

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(USER, false), // Not a signer
//                 AccountMeta::new(user_account_pda, false),
//                 AccountMeta::new_readonly(system_program_id, false),
//             ],
//             data: instruction_data,
//         };

//         let user_account_lamports = solana_sdk::native_token::LAMPORTS_PER_SOL; // Give more lamports
//         let user = Account {
//             lamports: user_account_lamports,
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let user_account = Account {
//             lamports: 0,
//             data: vec![], // Empty data for uninitialized account
//             owner: solana_sdk::system_program::id(), // Starts as system owned
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (USER, user),
//                 (user_account_pda, user_account),
//                 (system_program_id, system_account),
//             ],
//             &[Check::success()],
//         );
//     }
// }