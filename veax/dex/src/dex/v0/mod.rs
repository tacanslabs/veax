use super::{FeeLevel, PoolsNumber};

mod account_state_ex;
mod pool_state_ex;
mod position_state_ex;
mod util_types;

use super::super::dex;

pub use account_state_ex::*;
pub use pool_state_ex::*;
pub use util_types::*;

pub const NUM_FEE_LEVELS: FeeLevel = 8;

pub const NUM_TOP_POOLS: PoolsNumber = 8;

pub const NUM_PATHS: PoolsNumber = 16;
