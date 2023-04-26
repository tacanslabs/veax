#![allow(clippy::all, clippy::pedantic)]

#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use std::iter::Sum;
use std::ops;

use super::{U256, U384};
use crate::{chain::Float, dex::Error};

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct U256X128(pub U384);

impl U256X128 {
    pub const fn zero() -> Self {
        U256X128(U384::zero())
    }

    pub const fn one() -> Self {
        U256X128(U384([0, 0, 1, 0, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        U256X128(U384([self.0 .0[0], self.0 .0[1], 0, 0, 0, 0]))
    }

    pub const fn floor(self) -> Self {
        U256X128(U384([
            0,
            0,
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
        ]))
    }

    pub fn integer_sqrt(self) -> Self {
        todo!()
    }
}

impl TryFrom<U256X128> for u128 {
    type Error = Error;

    fn try_from(_val: U256X128) -> Result<u128, Self::Error> {
        todo!()
    }
}

impl From<u128> for U256X128 {
    fn from(_value: u128) -> Self {
        todo!()
    }
}

impl From<U256X128> for U256 {
    fn from(_val: U256X128) -> U256 {
        todo!()
    }
}

impl From<U256> for U256X128 {
    fn from(_value: U256) -> Self {
        todo!()
    }
}

impl From<U256X128> for [u64; 6] {
    fn from(_value: U256X128) -> Self {
        todo!()
    }
}

impl From<[u64; 6]> for U256X128 {
    fn from(_array: [u64; 6]) -> Self {
        todo!()
    }
}

impl ops::Add for U256X128 {
    type Output = Self;

    fn add(self, rhs: U256X128) -> Self {
        U256X128(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U256X128 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U256X128 {
    type Output = Self;

    fn sub(self, rhs: U256X128) -> Self {
        U256X128(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U256X128 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U256X128 {
    type Output = Self;

    fn mul(self, _rhs: Self) -> Self {
        todo!()
    }
}

impl ops::Div for U256X128 {
    type Output = Self;

    fn div(self, _rhs: Self) -> Self {
        todo!()
    }
}

impl Sum for U256X128 {
    fn sum<I: Iterator<Item = U256X128>>(_iter: I) -> Self {
        todo!()
    }
}

impl From<U256X128> for Float {
    fn from(_v: U256X128) -> Self {
        todo!()
    }
}

impl TryFrom<Float> for U256X128 {
    type Error = crate::dex::Error;

    fn try_from(_value: Float) -> Result<Self, Self::Error> {
        todo!()
    }
}
