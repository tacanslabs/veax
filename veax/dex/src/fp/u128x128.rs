#![allow(clippy::all, clippy::pedantic)]

#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use super::ufp_to_float::ufp_to_float;
use crate::chain::Float;
use num_traits::Zero;
use std::iter::Sum;
use std::ops;

use super::try_float_to_ufp::try_float_to_ufp;
use super::{Error, U128, U256, U384, U512};

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct U128X128(pub U256);

impl U128X128 {
    pub const fn zero() -> Self {
        U128X128(U256::zero())
    }

    pub fn one() -> Self {
        U128X128(U256::one() << 128)
    }

    pub const fn fract(self) -> Self {
        // the fractional part is saved in the first part
        // of the underlying array therefore the underlying
        // array contains zeroth and first values of the
        // array, and the second part is zeroed, as the
        // integer part is zero
        U128X128(U256([self.0 .0[0], self.0 .0[1], 0, 0]))
    }

    pub const fn floor(self) -> Self {
        // the integer part is saved in the second part
        // of the underlying array therefore the underlying
        // array contains second and third values of the
        // array, and the first part is zeroed, as the
        // fractional part is zero
        U128X128(U256([0, 0, self.0 .0[2], self.0 .0[3]]))
    }

    pub fn integer_sqrt(self) -> Self {
        let integer_sqrt = self.0.integer_sqrt();
        // as we taking the sqaure root of a fraction
        // it's denominator, namely 2^64 also gets a square root
        // which is 2^64, therefore to compensate this
        // we need to multiply by 2^64, which is the same
        // as to move the underlying value by 1 to the right
        U128X128(U256([
            0,
            integer_sqrt.0[0],
            integer_sqrt.0[1],
            integer_sqrt.0[2],
        ]))
    }

    pub fn lower_part(self) -> u128 {
        U128([self.0 .0[0], self.0 .0[1]]).low_u128()
    }

    pub fn upper_part(self) -> u128 {
        U128([self.0 .0[2], self.0 .0[3]]).low_u128()
    }

    pub fn truncate_fract_to_64bits(self) -> Self {
        U128X128(U256([0, self.0 .0[1], self.0 .0[2], self.0 .0[3]]))
    }

    pub fn recip(self) -> Self {
        Self::one() / self
    }
}

impl From<U128X128> for Float {
    fn from(x: U128X128) -> Float {
        ufp_to_float::<4, 2>(x.0 .0)
    }
}

impl TryFrom<Float> for U128X128 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<U128X128, 4, 2>(value)
    }
}

impl From<U128X128> for u128 {
    fn from(val: U128X128) -> u128 {
        val.upper_part()
    }
}

impl From<u128> for U128X128 {
    fn from(value: u128) -> Self {
        U128X128(U256([0, 0, value as u64, (value >> 64) as u64]))
    }
}

impl From<U128X128> for [u64; 4] {
    fn from(value: U128X128) -> Self {
        value.0 .0
    }
}

impl From<[u64; 4]> for U128X128 {
    fn from(array: [u64; 4]) -> Self {
        Self(U256(array))
    }
}

impl ops::Add for U128X128 {
    type Output = Self;

    fn add(self, rhs: U128X128) -> Self {
        U128X128(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U128X128 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U128X128 {
    type Output = Self;

    fn sub(self, rhs: U128X128) -> Self {
        U128X128(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U128X128 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U128X128 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U256s are multiplied exactly, in sufficiently high precision,
        // and converted to U128X128 taking the scale into account and truncating excessive precision.
        // As the product must fit into U128X128, it is sufficient to perfrom
        // the multiplication in 384 (i.e. 3x128) bits:
        // U128X128 x U128X128 = U256/2**128 x U256/2**128 = U384/2**256 = U128x256  -->  U128X128

        let self_u384 = U384([self.0 .0[0], self.0 .0[1], self.0 .0[2], self.0 .0[3], 0, 0]);
        let rhs_u384 = U384([rhs.0 .0[0], rhs.0 .0[1], rhs.0 .0[2], rhs.0 .0[3], 0, 0]);

        // The product of two U128X128 may not necessarily fit into U128X128,
        // so we need to check for overflow:
        let (res_u384, is_overflow) = self_u384.overflowing_mul(rhs_u384);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U128X128:
        U128X128(U256([
            res_u384.0[2],
            res_u384.0[3],
            res_u384.0[4],
            res_u384.0[5],
        ]))
    }
}

impl ops::Div for U128X128 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        let self_u512 = U512([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            0,
            0,
            0,
            0,
        ]);
        let rhs_u512 = U512([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            0,
            0,
            0,
            0,
        ]);

        // as we divide 2 fractions with the same denominator (namely 2^128)
        // we are getting a value without a denominator
        // we need to multiply by this denominator to respect the definition
        // doing this is the same as moving the underlying array
        // by two u64 value to the right
        let self_u512_mul_2_128 = U512([
            0,
            0,
            self_u512.0[0],
            self_u512.0[1],
            self_u512.0[2],
            self_u512.0[3],
            self_u512.0[4],
            self_u512.0[5],
        ]);

        let res_u512 = self_u512_mul_2_128 / rhs_u512;
        // assure no overflows happen
        assert!(
            res_u512.0[4] == 0 && res_u512.0[5] == 0 && res_u512.0[6] == 0 && res_u512.0[7] == 0,
            "{}",
            Error::Overflow
        );

        U128X128(U256([
            res_u512.0[0],
            res_u512.0[1],
            res_u512.0[2],
            res_u512.0[3],
        ]))
    }
}

impl Sum for U128X128 {
    fn sum<I: Iterator<Item = U128X128>>(iter: I) -> Self {
        let mut s = U128X128::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl Zero for U128X128 {
    fn zero() -> Self {
        U128X128::zero()
    }

    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}
