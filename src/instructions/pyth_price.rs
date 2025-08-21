use pinocchio::{account_info::AccountInfo, program_error::ProgramError, pubkey::Pubkey, sysvars::clock::Clock, *};
use pinocchio_pubkey::*;
use pythnet_sdk::messages::FeedId;

const SOL_USD_FEED_ID: &str = "ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d";

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum VerificationLevel {
    Partial { num_signatures: u8 },
    Full
}

impl VerificationLevel {
    pub fn gte(&self, other: VerificationLevel) -> bool {
        match self {
            VerificationLevel::Full => true,
            VerificationLevel::Partial { num_signatures } => match other {
                VerificationLevel::Full => false,
                VerificationLevel::Partial { num_signatures: other_number_signature } => *num_signatures >= other_number_signature
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PriceFeedMessage {
    pub feed_id: FeedId,
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
    pub prev_publish_time: i64,
    pub ema_price: i64,
    pub ema_conf: u64,
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PriceUpdateV2 {
    pub write_authority: Pubkey,
    pub verification_level: VerificationLevel,
    pub price_message: PriceFeedMessage,
    pub posted_slot: u64,
}

impl PriceUpdateV2 {
    pub const LEN: usize = 8 + 32 + 2 + 32 + 8 + 8 + 4 + 8 + 8 + 8 + 8 + 8;
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub struct Price {
    pub price: i64,
    pub conf: u64,
    pub exponent: i32,
    pub publish_time: i64,
}

fn decode_hex_char(c: u8) -> Result<u8, ProgramError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

fn decode_hex(input: &str) -> Result<Vec<u8>, ProgramError> {
    let input_bytes = input.as_bytes();
    
    if input_bytes.len() % 2 != 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    
    let mut result = Vec::with_capacity(input_bytes.len() / 2);
    
    for chunk in input_bytes.chunks_exact(2) {
        let high = decode_hex_char(chunk[0])?;
        let low = decode_hex_char(chunk[1])?;
        result.push((high << 4) | low);
    }
    
    Ok(result)
}

impl PriceUpdateV2 {
    pub fn get_price_unchecked(
        &self,
        feed_id: &FeedId
    ) -> Result<Price, ProgramError> {
        if self.price_message.feed_id != *feed_id {
            return Err(ProgramError::InvalidAccountData);
        };

        Ok(Price {
            price: self.price_message.price,
            conf: self.price_message.conf,
            exponent: self.price_message.exponent,
            publish_time: self.price_message.publish_time
        })
    }

    pub fn get_price_no_older_than(
        &self,
        clock: &Clock,
        max_age: u64,
        feed_id: &FeedId
    ) -> Result<Price, ProgramError> {

        let price = self.get_price_unchecked(feed_id)?;

        let age = clock.unix_timestamp.saturating_sub(price.publish_time);
        if age > max_age as i64 {
            return Err(ProgramError::InvalidAccountData);
        };

        Ok(price)
    }

    pub fn get_feed_id_from_hex(input: &str) -> Result<FeedId, ProgramError> {
        let mut feed_id: FeedId = [0; 32];
        
        match input.len() {
            66 => {
                if !input.starts_with("0x") {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let decoded = decode_hex(&input[2..])?;
                if decoded.len() != 32 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                feed_id.copy_from_slice(&decoded);
            },
            64 => {
                let decoded = decode_hex(input)?;
                if decoded.len() != 32 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                feed_id.copy_from_slice(&decoded);
            },
            _ => return Err(ProgramError::InvalidInstructionData),
        }
        
        Ok(feed_id)
    }
}

pub fn fetch_sol_price(accounts: &[AccountInfo]) -> ProgramResult {

    let [signer, price_update_account, clock_sysvar] = accounts else {
        return Err(ProgramError::InvalidAccountData);
    };

    if !signer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    };

    let clock = Clock::from_account_info(clock_sysvar)?;

    let price_update_data = unsafe { price_update_account.borrow_data_unchecked() };

    if price_update_data.len() < PriceUpdateV2::LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    let price_update = unsafe { 
        &*(price_update_data.as_ptr() as *const PriceUpdateV2) 
    };

    let sol_feed_id = PriceUpdateV2::get_feed_id_from_hex(SOL_USD_FEED_ID)?;

    let max_age = 60;

    let sol_price = price_update.get_price_no_older_than(&clock, max_age, &sol_feed_id)?;

    let price_scaled = if sol_price.exponent < 0 {
        let divisor = 10_i64.pow((-sol_price.exponent) as u32);
        sol_price.price as f64 / divisor as f64
    } else {
        let multiplier = 10_i64.pow(sol_price.exponent as u32);
        (sol_price.price * multiplier) as f64
    };

    println!("SOL/USD Price: ${:.2}", price_scaled);
    println!("Price confidence: {}", sol_price.conf);
    println!("Publish time: {}", sol_price.publish_time);
    println!("Exponent: {}", sol_price.exponent);

    Ok(())
}

pub fn get_sol_price_for_trading(
    price_update_account: &AccountInfo,
    clock: &Clock,
    max_age_seconds: u64,
) -> Result<u64, ProgramError> {
    
    let price_update_data = price_update_account.try_borrow_data()?;
    if price_update_data.len() < PriceUpdateV2::LEN {
        return Err(ProgramError::InvalidAccountData);
    }

    let price_update = unsafe { 
        &*(price_update_data.as_ptr() as *const PriceUpdateV2) 
    };

    let sol_feed_id = PriceUpdateV2::get_feed_id_from_hex(SOL_USD_FEED_ID)?;

    let sol_price = price_update.get_price_no_older_than(clock, max_age_seconds, &sol_feed_id)?;

    let price_normalized = normalize_pyth_price(sol_price)?;
    
    Ok(price_normalized)
}

fn normalize_pyth_price(price: Price) -> Result<u64, ProgramError> {
    if price.price <= 0 {
        return Err(ProgramError::InvalidAccountData);
    }

    let normalized_price = if price.exponent < 0 {
        let scale_factor = 10_i64.pow((-price.exponent) as u32);
        let target_scale = 100_000_000i64; 
        
        if scale_factor == target_scale {
            price.price as u64
        } else if scale_factor > target_scale {
            (price.price / (scale_factor / target_scale)) as u64
        } else {
            (price.price * (target_scale / scale_factor)) as u64
        }
    } else {
        let multiplier = 10_i64.pow(price.exponent as u32);
        (price.price * multiplier * 100_000_000) as u64
    };

    Ok(normalized_price)
}

// =============== TESTING fetch_sol_price ===============

// #[cfg(test)]
// mod testing {
//     use super::*;
//     use mollusk_svm::{program, Mollusk, result::Check};
//     use solana_sdk::{
//         account::Account, 
//         clock::Clock, 
//         instruction::{AccountMeta, Instruction}, 
//         pubkey::Pubkey as SolPubkey, 
//         pubkey::Pubkey,
//         pubkey,
//         sysvar::clock,
//     };

//     const PROGRAM_ID: Pubkey = pubkey!("D9aKLRn9MTxmaD1U18uexF59TdYA5Q67us3pE4shsGaQ");
//     const SIGNER: Pubkey =  Pubkey::new_from_array([1u8; 32]);

//     #[test]
//     fn test_fetch_sol_price() {
//         let mollusk = Mollusk::new(&PROGRAM_ID, "target/deploy/pinocchio_pyth");
//         let price_update_pubkey = Pubkey::new_unique();
//         let sol_feed_id = PriceUpdateV2::get_feed_id_from_hex(SOL_USD_FEED_ID).unwrap();

//         let current_time = 1700000000i64;

//         let price_message = PriceFeedMessage {
//             feed_id: sol_feed_id,
//             price: 10500000000i64, // $105.00 with exponent -8
//             conf: 5000000u64,      // Confidence interval
//             exponent: -8i32,       // Price is scaled by 10^-8
//             publish_time: current_time - 30, // 30 seconds ago
//             prev_publish_time: current_time - 90,
//             ema_price: 10450000000i64,
//             ema_conf: 4500000u64,
//         };

//         let write_authority_bytes = SolPubkey::new_unique().to_bytes();

//         let price_update_v2 = PriceUpdateV2 {
//             write_authority: write_authority_bytes,
//             verification_level: VerificationLevel::Full,
//             price_message,
//             posted_slot: 12345u64,
//         };

//         let mut price_update_data = vec![0u8; PriceUpdateV2::LEN];
//         unsafe {
//             let price_update_ptr = price_update_data.as_mut_ptr() as *mut PriceUpdateV2;
//             *price_update_ptr = price_update_v2;
//         };

//         let price_update_account = Account {
//             lamports: 1000000,
//             data: price_update_data,
//             owner: PROGRAM_ID,
//             executable: false,
//             rent_epoch: 0,
//         };

//         let clock = Clock {
//             slot: 12345,
//             epoch_start_timestamp: current_time - 1000,
//             epoch: 100,
//             leader_schedule_epoch: 100,
//             unix_timestamp: current_time,
//         };

//         let mut clock_data = vec![0u8; std::mem::size_of::<Clock>()];
//         unsafe {
//             let clock_ptr = clock_data.as_mut_ptr() as *mut Clock;
//             *clock_ptr = clock;
//         }

//         let clock_account = Account {
//             lamports: 1000000,
//             data: clock_data,
//             owner: solana_sdk::sysvar::ID,
//             executable: false,
//             rent_epoch: 0,
//         };

//         let mut instruction_data = vec![0u8; 32];
//         instruction_data[0] = 0;

//         let instruction = Instruction::new_with_bytes(
//             PROGRAM_ID,
//             &instruction_data,
//             vec![
//                 AccountMeta::new(SIGNER, true),                   
//                 AccountMeta::new_readonly(price_update_pubkey, false),
//                 AccountMeta::new_readonly(solana_sdk::sysvar::clock::id(), false), 
//             ],
//         );
        
//         mollusk.process_and_validate_instruction(
//             &instruction,
//             &[
//                 (SIGNER, Account::default()),
//                 (price_update_pubkey, price_update_account),
//                 (clock::ID, clock_account)
//             ],
//             &[Check::success()],
//         );

//     }
// }