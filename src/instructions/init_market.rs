use pinocchio::{
    account_info::AccountInfo, 
    instruction::Signer, 
    program_error::ProgramError, 
    pubkey::Pubkey, 
    sysvars::{rent::Rent, Sysvar}, 
    *
};
use crate::states::Market;
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::InitializeAccount3;

pub fn initialize_market(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {

    let [authority, collateral_mint, market_account, collateral_vault, system_program, token_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !authority.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // if instruction_data.len() < 32 {
    //     return Err(ProgramError::InvalidInstructionData);
    // }

    let market_id = u64::from_le_bytes(
        instruction_data[0..8].try_into().map_err(|_| ProgramError::InvalidInstructionData)?
    );

    let mut market_symbol = [0u8; 16];
    market_symbol.copy_from_slice(&instruction_data[8..24]);

    let max_leverage = u64::from_le_bytes(
        instruction_data[24..32].try_into().map_err(|_| ProgramError::InvalidInstructionData)?
    );

    let (market_account_pda, market_bump) = pubkey::find_program_address(
        &[b"market_account", authority.key().as_ref(), market_id.to_le_bytes().as_ref()],
        &crate::ID
    );

    let (collateral_vault_pda, collateral_bump) = pubkey::find_program_address(
        &[b"collateral_vault", collateral_mint.key().as_ref(), market_id.to_le_bytes().as_ref()],
        &crate::ID
    );

    if *collateral_vault.key() != collateral_vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    if *market_account.key() != market_account_pda {
        return Err(ProgramError::InvalidSeeds);
    }
    
    if market_account.data_is_empty() {
        println!("Initializing Market Account!");

        let lamports = Rent::get()?.minimum_balance(Market::SIZE);

        let market_id_bytes = market_id.to_le_bytes();
        let bump_ref = &[market_bump];
        let seeds = seeds!(
            b"market_account",
            authority.key().as_ref(),
            &market_id_bytes,
            bump_ref
        );
        let signer = Signer::from(&seeds);

        CreateAccount {
            from: authority,
            to: market_account,
            lamports,
            space: Market::SIZE as u64,
            owner: &crate::ID
        }.invoke_signed(&[signer])?;

        // Initialize market data
        let mut market_data = Market::from_account_info_mut(market_account)?;
        market_data.is_initialized = true;
        market_data.market_id = market_id as u8;
        market_data.market_symbol = market_symbol;
        market_data.oracle = Pubkey::default();
        market_data.collateral_mint = *collateral_mint.key(); // FIXED: Set actual mint
        market_data.collateral_vault = *collateral_vault.key();
        market_data.base_oracle = Pubkey::default();
        market_data.initial_margin = 0;
        market_data.maintenance_margin = 0;
        market_data.max_leverage = max_leverage;
        market_data.fee_rate = 0;
        market_data.funding_rate = 0;
        market_data.last_funding_time = 0;
        market_data.funding_interval = 28800;
        market_data.open_interest_long = 0;
        market_data.open_interest_short = 0;
        market_data.total_collateral = 0;
        market_data.unrealized_pnl = 0;
        market_data.authority = *authority.key();
        market_data.bump = market_bump;
        market_data.collateral_bump = collateral_bump;

        println!("Market Account Initialized!");
    } else {
        return Err(ProgramError::AccountAlreadyInitialized);
    }
    
    if collateral_vault.data_is_empty() {
        println!("Initializing Collateral Vault!");

        // Step 1: Create the account with system program
        let token_account_lamports = Rent::get()?.minimum_balance(165); // Token account size

        let collateral_id_bytes = market_id.to_le_bytes();
        let collateral_bump_ref = &[collateral_bump];
        let vault_seeds = seeds!(
            b"collateral_vault",
            collateral_mint.key().as_ref(),
            &collateral_id_bytes,
            collateral_bump_ref
        );
        let vault_signer = Signer::from(&vault_seeds);

        CreateAccount {
            from: authority,
            to: collateral_vault,
            lamports: token_account_lamports,
            space: 165, // Token account size
            owner: token_program.key(), // Owned by token program!
        }.invoke_signed(&[vault_signer])?;

        // Step 2: Initialize as token account owned by market PDA
        InitializeAccount3 {
            account: collateral_vault,
            mint: collateral_mint,
            owner: &market_account_pda, // Market PDA owns the vault!
        }.invoke()?;

        println!("Collateral Vault Initialized!");
    } else {
        return Err(ProgramError::AccountAlreadyInitialized);
    }

    Ok(())
}

// =========================== TESTING initialize_market ===========================

// #[cfg(test)]
// mod testing {
//     use mollusk_svm::{Mollusk, result::Check, program};
//     use solana_sdk::{
//         account::Account,
//         instruction::{AccountMeta, Instruction},
//         pubkey::Pubkey,
//         program_error::ProgramError,
//         pubkey
//     };

//     const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("BXacY2xWwx7ogSa1CnvrdXxAigBMwwszoZf4Q98E2YoV");
//     const AUTHORITY: Pubkey = Pubkey::new_from_array([1u8; 32]);
//     const MARKET_ID: u64 = 66;
//     const COLLATERAL_MINT: Pubkey = Pubkey::new_from_array([3u8; 32]);
//     const MAX_LEVERAGE: u64 = 1000; // 10x leverage

//     #[test]
//     fn test_initialize_market() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (market_account_pda, market_bump) = Pubkey::find_program_address(
//             &[b"market_account", AUTHORITY.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
//             &PROGRAM_ID
//         );

//         let (collateral_vault_pda, collateral_bump) = Pubkey::find_program_address(
//             &[b"collateral_vault", COLLATERAL_MINT.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();
//         let token_program = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

//         let mut instruction_data = vec![0u8; 32];
        
//         // Market ID: bytes 0-8
//         instruction_data[0..8].copy_from_slice(&MARKET_ID.to_le_bytes());
        
//         // Market symbol: bytes 8-24 (16 bytes)
//         let market_symbol = b"SOL-PERP\0\0\0\0\0\0\0\0";
//         instruction_data[8..24].copy_from_slice(market_symbol);
        
//         // Max leverage: bytes 24-32
//         instruction_data[24..32].copy_from_slice(&MAX_LEVERAGE.to_le_bytes());

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(AUTHORITY, true),
//                 AccountMeta::new(COLLATERAL_MINT, false),
//                 AccountMeta::new(market_account_pda, false),
//                 AccountMeta::new(collateral_vault_pda, false),
//                 AccountMeta::new(system_program_id, false), 
//                 AccountMeta::new(token_program, false),
//             ],
//             data: instruction_data,
//         };

//         let authority_account = Account {
//             lamports: 10_000_000, // 0.01 SOL
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let market_account = Account {
//             lamports: 0,
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let mint_account = Account {
//             lamports: 0,
//             data: vec![0; 82], // Standard mint account size
//             owner: token_program,
//             executable: false,
//             rent_epoch: 0,
//         };

//         let collateral_vault_account = Account {
//             lamports: 0,
//             data: vec![0; 82], // Standard mint account size
//             owner: token_program,
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (AUTHORITY, authority_account),
//                 (COLLATERAL_MINT, mint_account),
//                 (market_account_pda, market_account),
//                 (collateral_vault_pda, collateral_vault_account),
//                 (system_program_id, system_account),
//                 (token_program, Account {
//                     lamports: 0,
//                     data: vec![],
//                     owner: token_program,
//                     executable: false,
//                     rent_epoch: 0,
//                 }),
//             ],
//             &[Check::success()],
//         );
//     }

//     #[test]
//     fn test_initialize_market_with_invalid_signer() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (market_account_pda, market_bump) = Pubkey::find_program_address(
//             &[b"market_account", AUTHORITY.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();

//         // FIXED: Create 25-byte instruction data instead of 9
//         let mut instruction_data = vec![0u8; 25];
//         instruction_data[0] = 0;
//         instruction_data[1..9].copy_from_slice(&MARKET_ID.to_le_bytes());
        
//         // FIXED: Add market symbol properly
//         let market_symbol = b"SOL-PERP\0\0\0\0\0\0\0\0";
//         instruction_data[9..25].copy_from_slice(market_symbol);

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(AUTHORITY, false), // Not a signer
//                 AccountMeta::new(market_account_pda, false),
//                 AccountMeta::new_readonly(system_program_id, false),
//             ],
//             data: instruction_data,
//         };

//         let authority_account = Account {
//             lamports: 10_000_000, // 0.01 SOL
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let market_account = Account {
//             lamports: 0,
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (AUTHORITY, authority_account),
//                 (market_account_pda, market_account),
//                 (system_program_id, system_account),
//             ],
//             &[Check::err(ProgramError::InvalidAccountData)], // Expect this specific error
//         );
//     }

//     #[test]
//     fn test_initialize_market_already_initialized() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");

//         let (market_account_pda, market_bump) = Pubkey::find_program_address(
//             &[b"market_account", AUTHORITY.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
//             &PROGRAM_ID
//         );

//         let (system_program_id, system_account) = program::keyed_account_for_system_program();

//         let mut instruction_data = vec![0u8; 25];
//         instruction_data[0] = 0;
//         instruction_data[1..9].copy_from_slice(&MARKET_ID.to_le_bytes());

//         let market_symbol = b"SOL-PERP\0\0\0\0\0\0\0\0";
//         instruction_data[9..25].copy_from_slice(market_symbol);

//         let instruction = Instruction {
//             program_id: PROGRAM_ID,
//             accounts: vec![
//                 AccountMeta::new(AUTHORITY, true),
//                 AccountMeta::new(market_account_pda, false),
//                 AccountMeta::new(system_program_id, false), 
//             ],
//             data: instruction_data,
//         };

//         let authority_account = Account {
//             lamports: 10_000_000, // 0.01 SOL
//             data: vec![],
//             owner: solana_sdk::system_program::id(),
//             executable: false,
//             rent_epoch: 0,
//         };

//         let market_account = Account {
//             lamports: 1_000_000, // Already initialized with some lamports
//             data: vec![], // Simulate initialized state
//             owner: PROGRAM_ID,
//             executable: false,
//             rent_epoch: 0,
//         };

//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &vec![
//                 (AUTHORITY, authority_account),
//                 (market_account_pda, market_account),
//                 (system_program_id, system_account),
//             ],
//             &[Check::err(ProgramError::AccountAlreadyInitialized)], // Expect this specific error
//         );
//     }
// }