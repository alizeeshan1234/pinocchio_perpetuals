use pinocchio::{program_error::ProgramError, *};

pub mod init_market;
pub use init_market::*;

pub mod init_user;
pub use init_user::*;

pub mod pyth_price;
pub use pyth_price::*;

pub mod open_position;
pub use open_position::*;

#[repr(u8)]
pub enum PerpetualInstructions {
    InitializeMarket,
    InitializeUser,
    OpenPosition
}

impl TryFrom<&u8> for PerpetualInstructions {
    type Error = ProgramError;

    fn try_from(value: &u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PerpetualInstructions::InitializeMarket),
            1 => Ok(PerpetualInstructions::InitializeUser),
            2 => Ok(PerpetualInstructions::OpenPosition),
            _ => Err(ProgramError::InvalidInstructionData)

        }
    }
}