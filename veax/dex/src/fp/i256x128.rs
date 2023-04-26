use super::{signed::Signed, u256x128::U256X128, Error};
use crate::chain::Float;
use crate::fp::signed;

pub type I256X128 = Signed<U256X128>;

impl TryFrom<Float> for I256X128 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        signed::try_from_float::<U256X128, 6, 2>(value)
    }
}

impl From<I256X128> for Float {
    fn from(v: I256X128) -> Self {
        signed::into_float::<U256X128, 6, 2>(v)
    }
}
