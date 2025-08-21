use pinocchio::{account_info::AccountInfo, instruction::Signer, program_error::ProgramError, pubkey::Pubkey, sysvars::{clock::Clock, rent::Rent, Sysvar}, *};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::TransferChecked;
use pinocchio_token::state::TokenAccount;

use crate::{instructions::get_sol_price_for_trading, states::{Market, UserAccount, Position}};

pub fn process_open_position(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {

    let [
        user,  // The trader (must sign transaction)
        market_authority, // Authority that controls the market
        collateral_mint, // Token mint for collateral (e.g., USDC)
        user_mint, // User's token mint (must match collateral mint)
        market_account, // Stores market configuration
        user_account, // User's trading account
        collateral_vault, // Vault holding all collateral
        user_token_account, // User's token account to debit
        user_position_account, // Account storing position data
        pyth_price_account, // Pyth oracle for price feeds
        system_program, 
        token_program,
        clock_sysvar // Solana clock for timestamps
        ] = accounts else {
        return Err(ProgramError::InvalidAccountData);
    };

    // ---- Basic checks ----
    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if *system_program.key() != pinocchio_system::ID {
        return Err(ProgramError::InvalidAccountData);
    }
    if *token_program.key() != pinocchio_token::ID {
        return Err(ProgramError::InvalidAccountData);
    }
    if instruction_data.len() < 25 {
        return Err(ProgramError::InvalidInstructionData);
    }
    if user_mint.key() != collateral_mint.key() {
        return Err(ProgramError::InvalidAccountData);
    }

    // ---- Parse instruction ----
    let market_id = instruction_data[0];
    let size = i128::from_le_bytes(
        instruction_data[1..17].try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    );
    let margin_amount = u64::from_le_bytes(
        instruction_data[17..25].try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?
    );
    if size == 0 {
        return Err(ProgramError::InvalidInstructionData);
    };

    // ---- Derive & check PDAs ----
    let (market_account_pda, _market_bump) = pubkey::find_program_address(
        &[b"market_account", market_authority.key().as_ref(), market_id.to_le_bytes().as_ref()],
        &crate::ID
    );
    if *market_account.key() != market_account_pda {
        return Err(ProgramError::InvalidAccountData);
    }

    let (user_account_pda, user_bump) = pubkey::find_program_address(
        &[b"user_account", user.key().as_ref()],
        &crate::ID
    );
    if *user_account.key() != user_account_pda {
        return Err(ProgramError::InvalidAccountData);
    }

    let (user_position_account_pda, position_bump) = pubkey::find_program_address(
        &[b"position", user.key().as_ref(), market_id.to_le_bytes().as_ref()],
        &crate::ID
    );
    if *user_position_account.key() != user_position_account_pda {
        return Err(ProgramError::InvalidAccountData);
    }

    let (collateral_vault_pda, _collateral_bump) = pubkey::find_program_address(
        &[b"collateral_vault", collateral_mint.key().as_ref(), market_id.to_le_bytes().as_ref()],
        &crate::ID
    );
    if *collateral_vault.key() != collateral_vault_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // ---- Load market ----
    let mut market = Market::from_account_info_mut(market_account)?;
    if !market.is_initialized {
        return Err(ProgramError::UninitializedAccount);
    }
    if market.authority != *market_authority.key() {
        return Err(ProgramError::InvalidAccountData);
    }
    if market.collateral_vault != *collateral_vault.key() {
        return Err(ProgramError::InvalidAccountData);
    }
    if market.collateral_mint != *collateral_mint.key() {
        return Err(ProgramError::InvalidAccountData);
    }

    // ---- Token account validations ----
    let user_ta = TokenAccount::from_account_info(user_token_account)?;
    if *user_ta.owner() != *user.key() || *user_ta.mint() != *collateral_mint.key() {
        return Err(ProgramError::InvalidAccountData);
    }

    let vault_ta = TokenAccount::from_account_info(collateral_vault)?;
    if *vault_ta.mint() != *collateral_mint.key() {
        return Err(ProgramError::InvalidAccountData);
    }

    // ---- Sysvars / Oracle ----
    let clock = Clock::from_account_info(clock_sysvar)?;
    let current_time = clock.unix_timestamp;

    let current_price = get_sol_price_for_trading(
        pyth_price_account,
        &clock,
        60
    )?;

    // ---- Notional & margin checks (u128) ----
    let position_value = calculate_position_value(size, current_price)?;
    let required_margin = calculate_required_margin(position_value, market.initial_margin)?;

    if margin_amount < required_margin {
        return Err(ProgramError::InsufficientFunds);
    }

    let leverage = calculate_leverage(position_value, margin_amount)?;

    if leverage > market.max_leverage {
        return Err(ProgramError::InvalidInstructionData);
    }

    // ---- Fee calculation (u128) ----
    let trading_fee = calculate_trading_fee(position_value, market.fee_rate)?;
    let total_required = margin_amount
        .checked_add(trading_fee)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // ---- Ensure user account exists ----
    let mut user_account_data = if user_account.data_is_empty() {
        let lamports = Rent::get()?.minimum_balance(UserAccount::SIZE);

        let seeds = seeds!(
            b"user_account",
            user.key().as_ref()
        );

        let signer_seeds = Signer::from(&seeds);

        CreateAccount {
            from: user,
            to: user_account,
            lamports,
            space: UserAccount::SIZE as u64,
            owner: &crate::ID
        }.invoke_signed(&[signer_seeds])?;

        let mut user_data = UserAccount::from_account_info_mut(user_account)?;
        user_data.owner = *user.key();
        user_data.margin_balance = 0;
        user_data.open_positions = [Pubkey::default(); 10];
        
        user_data
    } else {
        UserAccount::from_account_info_mut(user_account)?
    };

    if user_account_data.owner != *user.key() {
        return Err(ProgramError::InvalidAccountData);
    }

    // ---- Transfer margin from user -> vault ----
    TransferChecked {
        from: user_token_account,
        to: collateral_vault,
        authority: user,
        mint: collateral_mint,
        amount: margin_amount,
        decimals: 6, 
    }.invoke()?;

    user_account_data.margin_balance = user_account_data.margin_balance.checked_add(margin_amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    if user_account_data.margin_balance < trading_fee as u64 {
        return Err(ProgramError::InsufficientFunds);
    };

    user_account_data.margin_balance = user_account_data.margin_balance.checked_sub(trading_fee as u64)
        .ok_or(ProgramError::InsufficientFunds)?;

    // ---- Create or update position ----
    let position_data = if user_position_account.data_is_empty() {
        println!("Creating new position account");

        let lamports = Rent::get()?.minimum_balance(Position::SIZE);

        let market_id_bytes = market_id.to_le_bytes();
        let seeds = seeds!(
            b"position",
            user.key().as_ref(),
            market_id_bytes.as_ref()
        );

        let signer_seeds = Signer::from(&seeds);

        CreateAccount {
            from: user,
            to: user_position_account,
            lamports,
            space: Position::SIZE as u64,
            owner: &crate::ID
        }.invoke_signed(&[signer_seeds])?;

        let mut position = Position::from_account_info_mut(user_position_account)?;
        position.user = *user.key();
        position.market = *market_account.key();
        position.size = size;
        position.entry_price = current_price;
        position.margin = margin_amount;
        position.unrealized_pnl = 0;
        position.funding_payment = 0;
        position.last_funding_settlement = current_time;
        position.is_active = true;

        add_position_to_user(&mut user_account_data, user_position_account.key())?;
        
        position
    } else {
        println!("Updating existing position");
        let mut position = Position::from_account_info_mut(user_position_account)?;
        
        if position.user != *user.key() {
            return Err(ProgramError::InvalidAccountData);
        }

        update_existing_position(&mut position, size, current_price, margin_amount, current_time)?;
        
        position
    };

    // ---- Update market accounting (new collateral only) ----
    market.total_collateral = market
        .total_collateral
        .checked_add(margin_amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Update market open interest
    update_market_open_interest(&mut market, size, margin_amount)?;

    println!("Position opened successfully");
    println!("Size: {}", size);
    println!("Entry Price: {}", current_price);
    println!("Margin: {}", margin_amount);
    println!("Trading Fee: {}", trading_fee);
    println!("Total Deducted: {}", total_required);

    Ok(())
}

fn calculate_position_value(size: i128, price: u64) -> Result<u64, ProgramError> {
    let abs_size = size.abs() as u64;
    abs_size.checked_mul(price)
        .ok_or(ProgramError::ArithmeticOverflow)
}

fn calculate_required_margin(position_value: u64, initial_margin_bps: u64) -> Result<u64, ProgramError> {
    position_value.checked_mul(initial_margin_bps)
        .and_then(|v| v.checked_div(10000)) 
        .ok_or(ProgramError::ArithmeticOverflow)
}

fn calculate_leverage(position_value: u64, margin: u64) -> Result<u64, ProgramError> {
    if margin == 0 {
        return Err(ProgramError::InvalidArgument);
    }
    position_value.checked_div(margin)
        .ok_or(ProgramError::ArithmeticOverflow)
}

fn calculate_trading_fee(position_value: u64, fee_rate_bps: u64) -> Result<u64, ProgramError> {
    position_value.checked_mul(fee_rate_bps)
        .and_then(|v| v.checked_div(10000))
        .ok_or(ProgramError::ArithmeticOverflow)
}

fn update_existing_position(
    position: &mut Position,
    additional_size: i128,
    current_price: u64,
    additional_margin: u64,
    current_time: i64
) -> Result<(), ProgramError> {
    if !position.is_active {

        position.size = additional_size;
        position.entry_price = current_price;
        position.margin = additional_margin;
        position.is_active = true;
        position.last_funding_settlement = current_time;
        return Ok(());
    }

    let current_size = position.size;
    let new_total_size = current_size
        .checked_add(additional_size)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    if (current_size > 0 && additional_size > 0) || (current_size < 0 && additional_size < 0) {

        let current_notional = (current_size.abs() as u64)
            .checked_mul(position.entry_price)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        
        let additional_notional = (additional_size.abs() as u64)
            .checked_mul(current_price)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        
        let total_notional = current_notional
            .checked_add(additional_notional)
            .ok_or(ProgramError::ArithmeticOverflow)?;
        
        if new_total_size != 0 {
            position.entry_price = total_notional
                .checked_div(new_total_size.abs() as u64)
                .ok_or(ProgramError::ArithmeticOverflow)?;
        }
        
        position.size = new_total_size;

    } else if (current_size > 0 && additional_size < 0) || (current_size < 0 && additional_size > 0) {

        position.size = new_total_size;
        
        if new_total_size == 0 {
            position.is_active = false;
        } else if (current_size > 0 && new_total_size < 0) || (current_size < 0 && new_total_size > 0) {
            position.entry_price = current_price;
        }
    }

    position.margin = position.margin
        .checked_add(additional_margin)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    Ok(())
}

fn add_position_to_user(
    user_account: &mut UserAccount,
    position_key: &Pubkey
) -> Result<(), ProgramError> {
    for existing_position in &user_account.open_positions {
        if existing_position == position_key {
            return Ok(()); 
        }
    }

    for i in 0..user_account.open_positions.len() {
        if user_account.open_positions[i] == Pubkey::default() {
            user_account.open_positions[i] = *position_key;
            return Ok(());
        }
    }

    Err(ProgramError::AccountAlreadyInitialized)
}

fn update_market_open_interest(
    market: &mut Market,
    size: i128,
    margin: u64
) -> Result<(), ProgramError> {
    let abs_size = size.abs() as u64;
    
    if size > 0 {
        market.open_interest_long = market.open_interest_long
            .checked_add(abs_size)
            .ok_or(ProgramError::ArithmeticOverflow)?;
    } else {
        market.open_interest_short = market.open_interest_short
            .checked_add(abs_size)
            .ok_or(ProgramError::ArithmeticOverflow)?;
    }

    market.total_collateral = market.total_collateral
        .checked_add(margin)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    Ok(())
}

// =========================== TESTING process_open_position ===========================

#[cfg(test)]
mod tests {
    use mollusk_svm::{Mollusk, result::Check, program};
    use solana_sdk::{
        account::Account, instruction::{AccountMeta, Instruction}, pubkey::Pubkey, pubkey
    };

    const PROGRAM_ID: Pubkey = solana_sdk::pubkey!("BXacY2xWwx7ogSa1CnvrdXxAigBMwwszoZf4Q98E2YoV");
    const AUTHORITY: Pubkey = Pubkey::new_from_array([1u8; 32]);
    const USER: Pubkey = Pubkey::new_from_array([2u8; 32]);
    const MARKET_ID: u64 = 66;
    const COLLATERAL_MINT: Pubkey = Pubkey::new_from_array([3u8; 32]);
    const USER_MINT: Pubkey = Pubkey::new_from_array([3u8; 32]);

    #[test]
    fn test_process_open_position() {
        let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_perp");
        let price_update_pubkey = Pubkey::new_unique();
        let user_token_account_pubkey = Pubkey::new_unique();
        
        let (market_account_pda, _market_bump) = Pubkey::find_program_address(
            &[b"market_account", AUTHORITY.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
            &PROGRAM_ID
        );

        let (user_account_pda, _user_bump) = Pubkey::find_program_address(
            &[b"user_account", USER.as_ref()],
            &PROGRAM_ID
        );

        let (user_position_account_pda, _position_bump) = Pubkey::find_program_address(
            &[b"position", USER.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
            &PROGRAM_ID
        );

        let (collateral_vault_pda, _collateral_bump) = Pubkey::find_program_address(
            &[b"collateral_vault", COLLATERAL_MINT.as_ref(), MARKET_ID.to_le_bytes().as_ref()],
            &PROGRAM_ID
        );

        let (system_program_id, system_account) = program::keyed_account_for_system_program();
        let token_program = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        
        // Create clock sysvar account
        let clock_id = solana_sdk::sysvar::clock::id();
        let clock_account = Account {
            lamports: 0,
            data: vec![0; 40],
            owner: solana_sdk::sysvar::id(),
            executable: false,
            rent_epoch: 0,
        };

        // Create instruction data with proper size (25 bytes total)
        let mut instruction_data = vec![0u8; 25];
        instruction_data[0] = MARKET_ID as u8;
        instruction_data[1..17].copy_from_slice(&i128::from(10).to_le_bytes());
        instruction_data[17..25].copy_from_slice(&1000u64.to_le_bytes());

        let instruction = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(USER, true),                              // 1. user
                AccountMeta::new(AUTHORITY, false),                        // 2. market_authority
                AccountMeta::new(COLLATERAL_MINT, false),                 // 3. collateral_mint
                AccountMeta::new(USER_MINT, false),                       // 4. user_mint
                AccountMeta::new(market_account_pda, false),              // 5. market_account
                AccountMeta::new(user_account_pda, false),                // 6. user_account
                AccountMeta::new(collateral_vault_pda, false),            // 7. collateral_vault
                AccountMeta::new(user_token_account_pubkey, false),       // 8. user_token_account
                AccountMeta::new(user_position_account_pda, false),       // 9. user_position_account
                AccountMeta::new_readonly(price_update_pubkey, false),    // 10. pyth_price_account
                AccountMeta::new_readonly(system_program_id, false),      // 11. system_program
                AccountMeta::new_readonly(token_program, false),          // 12. token_program
                AccountMeta::new_readonly(clock_id, false)                // 13. clock_sysvar
            ],
            data: instruction_data,
        };

        // Create all the accounts
        let user = Account {
            lamports: solana_sdk::native_token::LAMPORTS_PER_SOL,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };

        let user_account = Account {
            lamports: solana_sdk::native_token::LAMPORTS_PER_SOL,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };

        let authority_account = Account {
            lamports: solana_sdk::native_token::LAMPORTS_PER_SOL,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };

        let collateral_mint_account = Account {
            lamports: 0,
            data: vec![0; 82],
            owner: token_program,
            executable: false,
            rent_epoch: 0,
        };

        let user_mint_account = Account {
            lamports: 0,
            data: vec![0; 82],
            owner: token_program,
            executable: false,
            rent_epoch: 0,
        };

        // Market account needs to be initialized with proper Market struct data
        let market_data_size = 200; // Adjust based on your Market struct size
        let mut market_data = vec![0u8; market_data_size];
        
        // Set basic market data - adjust offsets based on your Market struct
        market_data[0] = 1; // is_initialized = true
        // You may need to set other fields like:
        // - collateral_vault pubkey
        // - collateral_mint pubkey  
        // - initial_margin, max_leverage, fee_rate values
        
        let market_account = Account {
            lamports: 1000000,
            data: market_data,
            owner: PROGRAM_ID, // Market account should be owned by your program
            executable: false,
            rent_epoch: 0,
        };

        let collateral_vault_account = Account {
            lamports: 0,
            data: vec![0; 165], // SPL token account size
            owner: token_program,
            executable: false,
            rent_epoch: 0,
        };

        let user_token_account = Account {
            lamports: 0,
            data: vec![0; 165], // SPL token account size
            owner: token_program,
            executable: false,
            rent_epoch: 0,
        };

        let user_position_account = Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::id(), // Will be created by the program
            executable: false,
            rent_epoch: 0,
        };

        let price_update_account = Account {
            lamports: 0,
            data: vec![0; 200], // Pyth price account size
            owner: PROGRAM_ID, // Or actual Pyth program ID
            executable: false,
            rent_epoch: 0,
        };

        let token_program_account = Account {
            lamports: 0,
            data: vec![],
            owner: Pubkey::default(),
            executable: true,
            rent_epoch: 0,
        };

        mollusk.process_and_validate_instruction(
            &instruction,
            &vec![
                (USER, user),
                (AUTHORITY, authority_account),
                (COLLATERAL_MINT, collateral_mint_account),
                (USER_MINT, user_mint_account),
                (market_account_pda, market_account),
                (user_account_pda, user_account),
                (collateral_vault_pda, collateral_vault_account),
                (user_token_account_pubkey, user_token_account), // This was missing!
                (user_position_account_pda, user_position_account),
                (price_update_pubkey, price_update_account),
                (system_program_id, system_account),
                (token_program, token_program_account),
                (clock_id, clock_account),
            ],
            &[Check::success()],
        );
    }
}