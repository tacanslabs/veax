use super::{signed::Signed, Error, U128X128, U192X192, U192X64, U256X320, U320X64};
use crate::chain::Float;
use crate::fp::{signed, U256};

pub type I256X320 = Signed<U256X320>;

impl From<u128> for I256X320 {
    fn from(value: u128) -> Self {
        I256X320 {
            value: U256X320::from(value),
            non_negative: true,
        }
    }
}

impl From<U128X128> for I256X320 {
    fn from(value: U128X128) -> Self {
        I256X320 {
            value: U256X320::from(value),
            non_negative: true,
        }
    }
}

impl From<U192X64> for I256X320 {
    fn from(value: U192X64) -> Self {
        I256X320 {
            value: U256X320::from(value),
            non_negative: true,
        }
    }
}

impl From<U256> for I256X320 {
    fn from(value: U256) -> Self {
        I256X320 {
            value: U256X320::from(value),
            non_negative: true,
        }
    }
}

impl TryFrom<I256X320> for U256 {
    type Error = Error;

    fn try_from(val: I256X320) -> Result<U256, Self::Error> {
        if val.non_negative {
            U256::try_from(val.value)
        } else {
            Err(Error::NegativeToUnsigned)
        }
    }
}

impl From<U192X192> for I256X320 {
    fn from(value: U192X192) -> Self {
        I256X320 {
            value: U256X320::from(value),
            non_negative: true,
        }
    }
}

impl TryFrom<Float> for I256X320 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        signed::try_from_float::<U256X320, 9, 5>(value)
    }
}

impl From<I256X320> for Float {
    fn from(v: I256X320) -> Self {
        signed::into_float::<U256X320, 9, 5>(v)
    }
}

impl TryFrom<U320X64> for I256X320 {
    type Error = Error;

    fn try_from(value: U320X64) -> Result<I256X320, Self::Error> {
        Ok(I256X320 {
            value: U256X320::try_from(value)?,
            non_negative: true,
        })
    }
}
