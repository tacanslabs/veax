#![allow(clippy::all, clippy::pedantic)]

#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use num_traits::Zero;
use std::iter::Sum;
use std::ops;

use super::{Error, U256, U320};
use crate::chain::Float;
use crate::fp::try_float_to_ufp::try_float_to_ufp;
use crate::fp::ufp_to_float::ufp_to_float;

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct U192X64(pub U256);

impl Zero for U192X64 {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    fn zero() -> Self {
        Self(U256::zero())
    }
    fn set_zero(&mut self) {
        self.0.set_zero();
    }
}

impl U192X64 {
    pub fn one() -> Self {
        U192X64(U256([0, 1, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        // the fractional part is saved in the first part
        // of the underlying array therefore the underlying
        // array contains zeroth and first values of the
        // array, and the second part is zeroed, as the
        // integer part is zero
        U192X64(U256([self.0 .0[0], 0, 0, 0]))
    }

    pub const fn floor(self) -> Self {
        // the integer part is saved in the second part
        // of the underlying array therefore the underlying
        // array contains second and third values of the
        // array, and the first part is zeroed, as the
        // fractional part is zero
        U192X64(U256([0, self.0 .0[1], self.0 .0[2], self.0 .0[3]]))
    }

    pub fn integer_sqrt(self) -> Self {
        let integer_sqrt = self.0.integer_sqrt();
        // as we taking the sqaure root of a fraction
        // it's denominator, namely 2^64 also gets a square root
        // which is 2^64, therefore to compensate this
        // we need to multiply by 2^64, which is the same
        // as to move the underlying value by 1 to the right
        U192X64(U256([
            0,
            integer_sqrt.0[0],
            integer_sqrt.0[1],
            integer_sqrt.0[2],
        ]))
    }

    pub fn recip(self) -> Self {
        Self::one() / self
    }
}

impl ops::Add for U192X64 {
    type Output = Self;

    fn add(self, rhs: U192X64) -> Self {
        U192X64(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U192X64 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U192X64 {
    type Output = Self;

    fn sub(self, rhs: U192X64) -> Self {
        U192X64(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U192X64 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U192X64 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U256s are multiplied exactly, in sufficiently high precision,
        // and converted to U192X64 taking the scale into account and truncating excessive precision.
        // As the product must fit into U192X64, it is sufficient to perfrom
        // the multiplication in 384 (i.e. 3x128) bits:
        // U192X64 x U192X64 = U256/2**64 x U256/2**64 = U320/2**128 = U128x128  -->  U192X64

        let self_u320 = U320([self.0 .0[0], self.0 .0[1], self.0 .0[2], self.0 .0[3], 0]);
        let rhs_u320 = U320([rhs.0 .0[0], rhs.0 .0[1], rhs.0 .0[2], rhs.0 .0[3], 0]);

        // The product of two U192X64 may not necessarily fit into U192X64,
        // so we need to check for overflow:
        let (rhs_u320, is_overflow) = self_u320.overflowing_mul(rhs_u320);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U192X64:
        U192X64(U256([
            rhs_u320.0[1],
            rhs_u320.0[2],
            rhs_u320.0[3],
            rhs_u320.0[4],
        ]))
    }
}

impl ops::Div for U192X64 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        // as we divide 2 fractions with the same denominator (namely 2^128)
        // we are getting a value without a denominator
        // we need to multiply by this denominator to respect the definition
        // doing this is the same as moving the underlying array
        // by one u64 value to the right
        let self_u320_mul_2_64 = U320([0, self.0 .0[0], self.0 .0[1], self.0 .0[2], self.0 .0[3]]);
        let rhs_u320 = U320([rhs.0 .0[0], rhs.0 .0[1], rhs.0 .0[2], rhs.0 .0[3], 0]);

        let res_u320 = self_u320_mul_2_64 / rhs_u320;
        // assure no overflows happen
        assert!(res_u320.0[4] == 0, "{}", Error::Overflow);

        U192X64(U256([
            res_u320.0[0],
            res_u320.0[1],
            res_u320.0[2],
            res_u320.0[3],
        ]))
    }
}

impl Sum for U192X64 {
    fn sum<I: Iterator<Item = U192X64>>(iter: I) -> Self {
        let mut s = U192X64::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl<'a> Sum<&'a Self> for U192X64 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        let mut s = U192X64::zero();
        for i in iter {
            s += *i;
        }
        s
    }
}

impl From<u128> for U192X64 {
    fn from(value: u128) -> Self {
        U192X64(U256([0, value as u64, (value >> 64) as u64, 0]))
    }
}

impl From<[u64; 4]> for U192X64 {
    fn from(array: [u64; 4]) -> Self {
        Self(U256(array))
    }
}

impl TryFrom<U192X64> for u128 {
    type Error = Error;

    fn try_from(v: U192X64) -> Result<Self, Self::Error> {
        if v.0 .0[3] > 0 {
            return Err(Error::Overflow);
        }
        Ok((u128::from(v.0 .0[2]) << 64) + u128::from(v.0 .0[1]))
    }
}

impl From<U192X64> for [u64; 4] {
    fn from(value: U192X64) -> Self {
        value.0 .0
    }
}

impl TryFrom<Float> for U192X64 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<U192X64, 4, 1>(value)
    }
}

impl From<U192X64> for Float {
    fn from(v: U192X64) -> Self {
        ufp_to_float::<4, 1>(v.0 .0)
    }
}
