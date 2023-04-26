use super::U320X64;
use super::{signed::Signed, Error, U128X128, U192X192, U192X64, U256X256};
use crate::chain::Float;
use crate::fp::{signed, U256};

pub type I256X256 = Signed<U256X256>;

impl From<u128> for I256X256 {
    fn from(value: u128) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl TryFrom<I256X256> for u128 {
    type Error = Error;

    fn try_from(val: I256X256) -> Result<u128, Self::Error> {
        if val.non_negative {
            u128::try_from(val.value)
        } else {
            Err(Error::NegativeToUnsigned)
        }
    }
}

impl From<U128X128> for I256X256 {
    fn from(value: U128X128) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl From<U192X64> for I256X256 {
    fn from(value: U192X64) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl From<U320X64> for I256X256 {
    fn from(value: U320X64) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl From<U256> for I256X256 {
    fn from(value: U256) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl From<U192X192> for I256X256 {
    fn from(value: U192X192) -> Self {
        I256X256 {
            value: U256X256::from(value),
            non_negative: true,
        }
    }
}

impl TryFrom<Float> for I256X256 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        signed::try_from_float::<U256X256, 8, 4>(value)
    }
}

impl From<I256X256> for Float {
    fn from(v: I256X256) -> Self {
        signed::into_float::<U256X256, 8, 4>(v)
    }
}
