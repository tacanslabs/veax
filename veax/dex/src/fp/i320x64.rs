use super::{signed::Signed, u320x64::U320X64, Error};
use crate::chain::Float;
use crate::fp::signed;

pub type I320X64 = Signed<U320X64>;

impl TryFrom<Float> for I320X64 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        signed::try_from_float::<U320X64, 6, 1>(value)
    }
}

impl From<I320X64> for Float {
    fn from(v: I320X64) -> Self {
        signed::into_float::<U320X64, 6, 1>(v)
    }
}
