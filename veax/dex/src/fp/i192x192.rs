use super::signed::{into_float, try_from_float, Signed};
use super::{Error, U128X128, U192X192, U192X64};
use crate::chain::Float;

pub type I192X192 = Signed<U192X192>;

impl From<U192X64> for I192X192 {
    fn from(value: U192X64) -> Self {
        I192X192 {
            value: value.into(),
            non_negative: true,
        }
    }
}

impl From<u128> for I192X192 {
    fn from(value: u128) -> Self {
        I192X192 {
            value: U192X192::from(value),
            non_negative: true,
        }
    }
}

impl From<U128X128> for I192X192 {
    fn from(value: U128X128) -> Self {
        I192X192 {
            value: U192X192::from(value),
            non_negative: true,
        }
    }
}

impl TryFrom<Float> for I192X192 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_from_float::<U192X192, 6, 3>(value)
    }
}

impl From<I192X192> for Float {
    fn from(v: I192X192) -> Self {
        into_float::<U192X192, 6, 3>(v)
    }
}
