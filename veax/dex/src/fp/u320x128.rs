#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use num_traits::Zero;

use std::iter::Sum;
use std::ops;

use super::{
    try_float_to_ufp::try_float_to_ufp, ufp_to_float::ufp_to_float, Error, U256, U320X64, U448,
    U576,
};
use crate::chain::Float;

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct U320X128(pub U448);

impl U320X128 {
    pub const fn one() -> Self {
        U320X128(U448([0, 0, 1, 0, 0, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        U320X128(U448([self.0 .0[0], self.0 .0[1], 0, 0, 0, 0, 0]))
    }

    pub const fn floor(self) -> Self {
        U320X128(U448([
            0,
            0,
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
        ]))
    }
}

impl Zero for U320X128 {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    fn zero() -> Self {
        Self(U448::zero())
    }
    fn set_zero(&mut self) {
        self.0.set_zero();
    }
}

impl From<u128> for U320X128 {
    fn from(value: u128) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let lower_word = value as u64;
        let upper_word = (value >> 64) as u64;

        U320X128(U448([0, 0, lower_word, upper_word, 0, 0, 0]))
    }
}

impl From<U320X128> for [u64; 7] {
    fn from(value: U320X128) -> Self {
        value.0 .0
    }
}

impl From<[u64; 7]> for U320X128 {
    fn from(array: [u64; 7]) -> Self {
        Self(U448(array))
    }
}

impl ops::Add for U320X128 {
    type Output = Self;

    fn add(self, rhs: U320X128) -> Self {
        U320X128(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U320X128 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U320X128 {
    type Output = Self;

    fn sub(self, rhs: U320X128) -> Self {
        U320X128(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U320X128 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U320X128 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U448 are multiplied exactly, in sufficiently high precision,
        // and converted to U320X128 taking the scale into account and truncating excessive precision.
        // As the product must fit into U320X128, it is sufficient to perfrom
        // the multiplication in 576 (i.e. 320 + 128 + 128) bits:
        // U320X128 x U320X128 = U576/2**128 x U576/2**128 = U576/2**256  -->  U320X128

        let self_u576 = U576([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            0,
            0,
        ]);
        let rhs_u576 = U576([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            0,
            0,
        ]);

        // The product of two U320X128 may not necessarily fit into U320X128,
        // so we need to check for overflow:
        let (result, is_overflow) = self_u576.overflowing_mul(rhs_u576);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U320X128:
        U320X128(U448([
            result.0[2],
            result.0[3],
            result.0[4],
            result.0[5],
            result.0[6],
            result.0[7],
            result.0[8],
        ]))
    }
}

impl ops::Div for U320X128 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        // as we divide 2 fractions with the same denominator (namely 2^128)
        // we are getting a value without a denominator
        // we need to multiply by this denominator to respect the definition
        // doing this is the same as moving the underlying array
        // by 128 bits to the right
        let self_u576_mul_2_128 = U576([
            0,
            0,
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
        ]);

        let rhs_u576 = U576([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            0,
            0,
        ]);

        let result = self_u576_mul_2_128 / rhs_u576;
        // ensure no overflows happen
        assert!(
            result.0[7..9].iter().all(|word| *word == 0),
            "{}",
            Error::Overflow
        );

        U320X128(U448([
            result.0[0],
            result.0[1],
            result.0[2],
            result.0[3],
            result.0[4],
            result.0[5],
            result.0[6],
        ]))
    }
}

impl Sum for U320X128 {
    fn sum<I: Iterator<Item = U320X128>>(iter: I) -> Self {
        let mut s = U320X128::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl<'a> Sum<&'a Self> for U320X128 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        let mut s = U320X128::zero();
        for i in iter {
            s += *i;
        }
        s
    }
}

impl From<U320X128> for Float {
    fn from(value: U320X128) -> Self {
        ufp_to_float::<7, 2>(value.0 .0)
    }
}

impl TryFrom<Float> for U320X128 {
    type Error = Error;

    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<_, 7, 2>(value)
    }
}

impl From<U256> for U320X128 {
    fn from(value: U256) -> Self {
        U320X128(U448([
            0, 0, value.0[0], value.0[1], value.0[2], value.0[3], 0,
        ]))
    }
}
impl From<U320X64> for U320X128 {
    fn from(value: U320X64) -> Self {
        U320X128(U448([
            0,
            value.0 .0[0],
            value.0 .0[1],
            value.0 .0[2],
            value.0 .0[3],
            value.0 .0[4],
            value.0 .0[5],
        ]))
    }
}
