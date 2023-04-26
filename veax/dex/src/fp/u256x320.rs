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
    U192X64, U256, U320, U320X128, U320X64, U384, U576, U896,
};
use crate::chain::Float;

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct U256X320(pub U576);

impl U256X320 {
    pub const fn one() -> Self {
        U256X320(U576([0, 0, 0, 0, 0, 1, 0, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        // the fractional part is saved in the first part
        // of the underlying array therefore the underlying
        // array contains zeroth and first values of the
        // array, and the second part is zeroed, as the
        // integer part is zero
        U256X320(U576([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
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
        U256X320(U576([
            0,
            0,
            0,
            0,
            0,
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
            self.0 .0[8],
        ]))
    }

    pub fn integer_sqrt(self) -> Self {
        // as we taking the sqaure root of a fraction
        // it's denominator, namely 2^320 also gets a square root
        // which is 2^160, therefore to compensate this
        // we need to multiply by 2^160, which is the same
        // as to make 160 left shifts
        U256X320(self.0.integer_sqrt() << 160)
    }

    pub fn lower_part(self) -> U320 {
        U320([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
        ])
    }

    pub fn upper_part(self) -> U256 {
        U256([self.0 .0[5], self.0 .0[6], self.0 .0[7], self.0 .0[8]])
    }

    pub fn ceil(self) -> Self {
        let mut res = self.floor();
        if self.0 .0[0..5].iter().any(|word| *word > 0) {
            res += Self::from(1);
        }
        res
    }
}

impl Zero for U256X320 {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    fn zero() -> Self {
        Self(U576::zero())
    }
    fn set_zero(&mut self) {
        self.0.set_zero();
    }
}

impl From<U128X128> for U256X320 {
    fn from(v: U128X128) -> Self {
        U256X320(U576([
            0, 0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], 0, 0,
        ]))
    }
}

impl From<U192X64> for U256X320 {
    fn from(v: U192X64) -> Self {
        U256X320(U576([
            0, 0, 0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], 0,
        ]))
    }
}

impl From<U192X192> for U256X320 {
    fn from(v: U192X192) -> Self {
        U256X320(U576([
            0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], v.0 .0[4], v.0 .0[5], 0,
        ]))
    }
}

impl TryFrom<U256X320> for U192X192 {
    type Error = Error;

    fn try_from(value: U256X320) -> Result<U192X192, Self::Error> {
        if value.0 .0[7] != 0 {
            return Err(Error::Overflow);
        };

        Ok(U192X192(U384([
            value.0 .0[2],
            value.0 .0[3],
            value.0 .0[4],
            value.0 .0[5],
            value.0 .0[6],
            value.0 .0[7],
        ])))
    }
}

impl TryFrom<U320X64> for U256X320 {
    type Error = Error;

    fn try_from(value: U320X64) -> Result<U256X320, Self::Error> {
        if value.0 .0[5] != 0 {
            return Err(Error::Overflow);
        };

        Ok(U256X320(U576([
            0,
            0,
            0,
            0,
            value.0 .0[0],
            value.0 .0[1],
            value.0 .0[2],
            value.0 .0[3],
            value.0 .0[4],
        ])))
    }
}

impl From<u128> for U256X320 {
    fn from(value: u128) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let lower_word = value as u64;
        let upper_word = (value >> 64) as u64;
        U256X320(U576([0, 0, 0, 0, 0, lower_word, upper_word, 0, 0]))
    }
}

impl TryFrom<U256X320> for u128 {
    type Error = Error;

    fn try_from(value: U256X320) -> Result<u128, Self::Error> {
        if value.0 .0[7..9].iter().any(|word| !word.is_zero()) {
            return Err(Error::Overflow);
        };
        let res = (u128::from(value.0 .0[6]) << 64) + u128::from(value.0 .0[5]);

        Ok(res)
    }
}

impl TryFrom<U256X320> for U256 {
    type Error = Error;
    fn try_from(value: U256X320) -> Result<Self, Self::Error> {
        Ok(value.upper_part())
    }
}

impl From<U256> for U256X320 {
    fn from(value: U256) -> Self {
        U256X320(U576([
            0, 0, 0, 0, 0, value.0[0], value.0[1], value.0[2], value.0[3],
        ]))
    }
}

impl From<U256X320> for [u64; 9] {
    fn from(value: U256X320) -> Self {
        value.0 .0
    }
}

impl From<[u64; 9]> for U256X320 {
    fn from(array: [u64; 9]) -> Self {
        Self(U576(array))
    }
}

impl ops::Add for U256X320 {
    type Output = Self;

    fn add(self, rhs: U256X320) -> Self {
        U256X320(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U256X320 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U256X320 {
    type Output = Self;

    fn sub(self, rhs: U256X320) -> Self {
        U256X320(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U256X320 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Mul for U256X320 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U576s are multiplied exactly, in sufficiently high precision,
        // and converted to U256X320 taking the scale into account and truncating excessive precision.
        // As the product must fit into U256X320, it is sufficient to perfrom
        // the multiplication in 896 bits (256 + 320 + 320):
        // U256X320 x U256X320 = U576/2^320 x U576/2^320 = U1152/2^640 = U512x640 --truncate--> U256X320

        let self_u896 = U896([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            self.0 .0[6],
            self.0 .0[7],
            self.0 .0[8],
            0,
            0,
            0,
            0,
            0,
        ]);
        let rhs_u896 = U896([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            rhs.0 .0[7],
            rhs.0 .0[8],
            0,
            0,
            0,
            0,
            0,
        ]);

        // The product of two U256X320 may not necessarily fit into U256X320,
        // so we need to check for overflow:
        let (res_u896, is_overflow) = self_u896.overflowing_mul(rhs_u896);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U256X320:
        U256X320(U576([
            // skip 5 lowest words
            res_u896.0[5],
            res_u896.0[6],
            res_u896.0[7],
            res_u896.0[8],
            res_u896.0[9],
            res_u896.0[10],
            res_u896.0[11],
            res_u896.0[12],
            res_u896.0[13],
        ]))
    }
}

impl ops::Div for U256X320 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        let self_u896 = U896([
            0,
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
            self.0 .0[8],
        ]);
        let rhs_u896 = U896([
            rhs.0 .0[0],
            rhs.0 .0[1],
            rhs.0 .0[2],
            rhs.0 .0[3],
            rhs.0 .0[4],
            rhs.0 .0[5],
            rhs.0 .0[6],
            rhs.0 .0[7],
            rhs.0 .0[8],
            0,
            0,
            0,
            0,
            0,
        ]);

        let res_u896 = self_u896 / rhs_u896;
        // ensure no overflows happen
        assert!(
            res_u896.0[8..14].iter().all(|word| *word == 0),
            "{}",
            Error::Overflow
        );

        U256X320(U576([
            res_u896.0[0],
            res_u896.0[1],
            res_u896.0[2],
            res_u896.0[3],
            res_u896.0[4],
            res_u896.0[5],
            res_u896.0[6],
            res_u896.0[7],
            res_u896.0[8],
        ]))
    }
}

impl Sum for U256X320 {
    fn sum<I: Iterator<Item = U256X320>>(iter: I) -> Self {
        let mut s = U256X320::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl<'a> Sum<&'a Self> for U256X320 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        let mut s = U256X320::zero();
        for i in iter {
            s += *i;
        }
        s
    }
}

impl From<U256X320> for Float {
    fn from(v: U256X320) -> Self {
        ufp_to_float::<9, 5>(v.0 .0)
    }
}

impl TryFrom<Float> for U256X320 {
    type Error = Error;

    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<_, 9, 5>(value)
    }
}

impl TryFrom<U320X128> for U256X320 {
    type Error = Error;
    fn try_from(value: U320X128) -> Result<Self, Self::Error> {
        if value.0 .0[6] != 0 {
            return Err(Error::Overflow);
        };

        Ok(U256X320(U576([
            0,
            0,
            0,
            value.0 .0[0],
            value.0 .0[1],
            value.0 .0[2],
            value.0 .0[3],
            value.0 .0[4],
            value.0 .0[5],
        ])))
    }
}
