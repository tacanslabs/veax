use super::signed::{self, Signed};
use super::u192x64::U192X64;
use super::Error;
use crate::chain::Float;

pub type I192X64 = Signed<U192X64>;

impl TryFrom<Float> for I192X64 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        signed::try_from_float::<U192X64, 4, 1>(value)
    }
}

impl From<I192X64> for Float {
    fn from(v: I192X64) -> Self {
        signed::into_float::<U192X64, 4, 1>(v)
    }
}
