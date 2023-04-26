#![allow(clippy::all, clippy::pedantic)]

#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};
use num_traits::Zero;

use std::iter::Sum;
use std::ops;

use super::{
    try_float_to_ufp::try_float_to_ufp, ufp_to_float::ufp_to_float, Error, U128X128, U192X192,
    U192X64, U256, U384, U512, U768,
};
use crate::chain::Float;

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct U256X256(pub U512);

impl U256X256 {
    pub const fn one() -> Self {
        U256X256(U512([0, 0, 0, 0, 1, 0, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        // the fractional part is saved in the first part
        // of the underlying array therefore the underlying
        // array contains zeroth and first values of the
        // array, and the second part is zeroed, as the
        // integer part is zero
        U256X256(U512([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            0,
            0,
            0,
            0,
        ]))
    }

    pub const fn floor(self) -> Self {
        // the integer part is saved in the second part
        // of the underlying array therefore the underlying
        // array contains second and third values of the
        // array, and the first part is zeroed, as the
        // fractional part is zero
        U256X256(U512([
            0,
            0,
            0,
            0,
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
        ]))
    }

    pub fn integer_sqrt(self) -> Self {
        let integer_sqrt = self.0.integer_sqrt();
        // as we taking the sqaure root of a fraction
        // it's denominator, namely 2^4*64 also gets a square root
        // which is 2^2*64, therefore to compensate this
        // we need to multiply by 2^2*64, which is the same
        // as to move the underlying value by 2 to the right
        U256X256(U512([
            0,
            0,
            integer_sqrt.0[0],
            integer_sqrt.0[1],
            integer_sqrt.0[2],
            integer_sqrt.0[3],
            integer_sqrt.0[4],
            integer_sqrt.0[5],
        ]))
    }

    pub fn lower_part(self) -> U256 {
        U256([self.0 .0[0], self.0 .0[1], self.0 .0[2], self.0 .0[3]])
    }

    pub fn upper_part(self) -> U256 {
        U256([self.0 .0[4], self.0 .0[5], self.0 .0[6], self.0 .0[7]])
    }

    pub fn truncate_fract_to_64bits(self) -> Self {
        U256X256(U512([
            0,
            0,
            0,
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
        ]))
    }

    pub fn ceil(self) -> Self {
        let mut res = self.floor();
        if self.0 .0[0..4].iter().any(|word| *word > 0) {
            res += Self::from(1);
        }
        res
    }
}

impl Zero for U256X256 {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    fn zero() -> Self {
        Self(U512::zero())
    }
    fn set_zero(&mut self) {
        self.0.set_zero();
    }
}

impl TryFrom<U256X256> for u128 {
    type Error = Error;

    fn try_from(val: U256X256) -> Result<u128, Self::Error> {
        let val = val.upper_part();
        if val > u128::MAX.into() {
            Err(Error::Overflow)
        } else {
            Ok(val.low_u128())
        }
    }
}

impl From<U128X128> for U256X256 {
    fn from(v: U128X128) -> Self {
        U256X256(U512([
            0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], 0, 0,
        ]))
    }
}

impl From<U192X64> for U256X256 {
    fn from(v: U192X64) -> Self {
        U256X256(U512([
            0, 0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], 0,
        ]))
    }
}

impl From<U192X192> for U256X256 {
    fn from(v: U192X192) -> Self {
        U256X256(U512([
            0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], v.0 .0[4], v.0 .0[5], 0,
        ]))
    }
}

impl TryFrom<U256X256> for U192X192 {
    type Error = Error;

    fn try_from(value: U256X256) -> Result<U192X192, Self::Error> {
        if value.0 .0[7] != 0 {
            return Err(Error::Overflow);
        };

        Ok(U192X192(U384([
            value.0 .0[1],
            value.0 .0[2],
            value.0 .0[3],
            value.0 .0[4],
            value.0 .0[5],
            value.0 .0[6],
        ])))
    }
}

impl From<u128> for U256X256 {
    fn from(value: u128) -> Self {
        U256X256(U512([0, 0, 0, 0, value as u64, (value >> 64) as u64, 0, 0]))
    }
}

impl From<U256X256> for U256 {
    fn from(val: U256X256) -> U256 {
        val.upper_part()
    }
}

impl From<U256> for U256X256 {
    fn from(value: U256) -> Self {
        U256X256(U512([
            0, 0, 0, 0, value.0[0], value.0[1], value.0[2], value.0[3],
        ]))
    }
}

impl From<U256X256> for [u64; 8] {
    fn from(value: U256X256) -> Self {
        value.0 .0
    }
}

impl From<[u64; 8]> for U256X256 {
    fn from(array: [u64; 8]) -> Self {
        Self(U512(array))
    }
}

impl ops::Add for U256X256 {
    type Output = Self;

    fn add(self, rhs: U256X256) -> Self {
        U256X256(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U256X256 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U256X256 {
    type Output = Self;

    fn sub(self, rhs: U256X256) -> Self {
        U256X256(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U256X256 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U256X256 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U512s are multiplied exactly, in sufficiently high precision,
        // and converted to U256X256 taking the scale into account and truncating excessive precision.
        // As the product must fit into U256X256, it is sufficient to perfrom
        // the multiplication in 768 (i.e. 6x128) bits:
        // U256X256 x U256X256 = U512/2**256 x U512/2**256 = U768/2**512 = U256x512  -->  U128X128

        let self_u768 = U768([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
            0,
            0,
            0,
            0,
        ]);
        let rhs_u768 = U768([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            rhs.0 .0[7],
            0,
            0,
            0,
            0,
        ]);

        // The product of two U128X128 may not necessarily fit into U128X128,
        // so we need to check for overflow:
        let (res_u768, is_overflow) = self_u768.overflowing_mul(rhs_u768);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U128X128:
        U256X256(U512([
            res_u768.0[4],
            res_u768.0[5],
            res_u768.0[6],
            res_u768.0[7],
            res_u768.0[8],
            res_u768.0[9],
            res_u768.0[10],
            res_u768.0[11],
        ]))
    }
}

impl ops::Div for U256X256 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        let self_u768 = U768([
            0,
            0,
            0,
            0,
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
        ]);
        let rhs_u768 = U768([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            rhs.0 .0[7],
            0,
            0,
            0,
            0,
        ]);

        let res_u768 = self_u768 / rhs_u768;
        // ensure no overflows happen
        assert!(
            res_u768.0[8] == 0 && res_u768.0[9] == 0 && res_u768.0[10] == 0 && res_u768.0[11] == 0,
            "{}",
            Error::Overflow
        );

        U256X256(U512([
            res_u768.0[0],
            res_u768.0[1],
            res_u768.0[2],
            res_u768.0[3],
            res_u768.0[4],
            res_u768.0[5],
            res_u768.0[6],
            res_u768.0[7],
        ]))
    }
}

impl Sum for U256X256 {
    fn sum<I: Iterator<Item = U256X256>>(iter: I) -> Self {
        let mut s = U256X256::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl<'a> Sum<&'a Self> for U256X256 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        let mut s = U256X256::zero();
        for i in iter {
            s += *i;
        }
        s
    }
}

impl From<U256X256> for Float {
    fn from(v: U256X256) -> Self {
        ufp_to_float::<8, 4>(v.0 .0)
    }
}

impl TryFrom<Float> for U256X256 {
    type Error = Error;

    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<_, 8, 4>(value)
    }
}
