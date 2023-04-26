#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use super::{Error, U384, U576, U768};
use crate::chain::Float;
use crate::fp::try_float_to_ufp::try_float_to_ufp;
use crate::fp::ufp_to_float::ufp_to_float;
use crate::fp::{U128X128, U192X64, U256};
use num_traits::Zero;
use std::{iter::Sum, ops};

#[cfg_attr(
    feature = "near",
    derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize)
)]
#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct U192X192(pub U384);

#[allow(unused)]
impl U192X192 {
    pub const fn one() -> Self {
        U192X192(U384([0, 0, 0, 1, 0, 0]))
    }

    pub const fn fract(self) -> Self {
        // the fractional part is saved in the zeroth value
        // of the underlying array
        U192X192(U384([self.0 .0[0], self.0 .0[1], self.0 .0[2], 0, 0, 0]))
    }

    pub const fn floor(self) -> Self {
        // the integer part is saved in the first value
        // of the underlying array
        U192X192(U384([0, 0, 0, self.0 .0[3], self.0 .0[4], self.0 .0[5]]))
    }

    pub fn ceil(self) -> Self {
        let mut res = self.floor();
        if self.0 .0[0..3].iter().any(|word| *word > 0) {
            res += Self::from(1);
        }
        res
    }

    pub fn integer_sqrt(self) -> Self {
        // as we taking the sqaure root of a fraction
        // it's denominator, namely 2^192 also gets a square root
        // which is 2^96, therefore to compensate this
        // we need to multiply by 2^96, which is the same
        // as to make 32 left shifts
        U192X192(self.0.integer_sqrt() << 96)
    }

    #[allow(unused_assignments)]
    pub fn integer_cbrt(self) -> Self {
        let mut inner = self.0;

        let mut s = 255;
        let mut y = U384::zero();
        let mut b = U384::zero();
        let one = U384::one();
        while s >= 0 {
            y += y;
            b = U384::from(3) * y * (y + one) + one;
            if (inner >> s) >= b {
                inner -= b << s;
                y += one;
            }
            s -= 3;
        }
        U192X192(U384::from([0, 0, y.0[0], y.0[1], y.0[2], y.0[3]]))
    }
}

impl From<u128> for U192X192 {
    fn from(value: u128) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let lower_word = value as u64;
        let upper_word = (value >> 64) as u64;
        U192X192(U384([0, 0, 0, lower_word, upper_word, 0]))
    }
}

impl From<U128X128> for U192X192 {
    fn from(v: U128X128) -> Self {
        U192X192(U384([0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3], 0]))
    }
}

impl From<[u64; 6]> for U192X192 {
    fn from(array: [u64; 6]) -> Self {
        Self(U384(array))
    }
}

impl TryFrom<U192X192> for u128 {
    type Error = Error;

    fn try_from(v: U192X192) -> Result<Self, Self::Error> {
        if v.0 .0[5] > 0 {
            return Err(Error::Overflow);
        }
        Ok((u128::from(v.0 .0[4]) << 64) + u128::from(v.0 .0[3]))
    }
}

impl From<U192X192> for Float {
    fn from(v: U192X192) -> Self {
        ufp_to_float::<6, 3>(v.0 .0)
    }
}

impl From<U192X64> for U192X192 {
    fn from(v: U192X64) -> Self {
        U192X192(U384([0, 0, v.0 .0[0], v.0 .0[1], v.0 .0[2], v.0 .0[3]]))
    }
}

impl TryFrom<U192X192> for U192X64 {
    type Error = Error;
    fn try_from(v: U192X192) -> Result<Self, Self::Error> {
        if v.0 .0[0] > 0 || v.0 .0[1] > 0 {
            return Err(Error::PrecisionLoss);
        }
        Ok(U192X64(U256([v.0 .0[2], v.0 .0[3], v.0 .0[4], v.0 .0[5]])))
    }
}

impl TryFrom<Float> for U192X192 {
    type Error = Error;
    fn try_from(value: Float) -> Result<Self, Self::Error> {
        try_float_to_ufp::<U192X192, 6, 3>(value)
    }
}

impl ops::Add for U192X192 {
    type Output = Self;

    fn add(self, rhs: U192X192) -> Self {
        U192X192(self.0 + rhs.0)
    }
}

impl ops::AddAssign for U192X192 {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::Sub for U192X192 {
    type Output = Self;

    fn sub(self, rhs: U192X192) -> Self {
        U192X192(self.0 - rhs.0)
    }
}

impl ops::SubAssign for U192X192 {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl Sum for U192X192 {
    fn sum<I: Iterator<Item = U192X192>>(iter: I) -> Self {
        let mut s = U192X192::zero();
        for i in iter {
            s += i;
        }
        s
    }
}

impl<'a> Sum<&'a Self> for U192X192 {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        let mut s = U192X192::zero();
        for i in iter {
            s += *i;
        }
        s
    }
}

impl ops::Mul for U192X192 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying U384s are multiplied exactly, in sufficiently high precision,
        // and converted to U192X192 taking the scale into account and truncating excessive precision.
        // As the product must fit into U192X192, it is sufficient to perfrom
        // the multiplication in 576 (i.e. 3x192) bits:
        // U192X192 x U192X192 = U384/2**192 x U384/2**192 = U576/2**384 = U192x384  -->  U192X192

        let self_u576 = U576([
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
            0,
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
            0,
            0,
            0,
        ]);

        // The product of two U192X192 may not necessarily fit into U192X192,
        // so we need to check for overflow:
        let (res_u576, is_overflow) = self_u576.overflowing_mul(rhs_u576);
        assert!(!is_overflow, "{}", Error::Overflow);

        // Scale the product back to U192X192:
        U192X192(U384([
            res_u576.0[3],
            res_u576.0[4],
            res_u576.0[5],
            res_u576.0[6],
            res_u576.0[7],
            res_u576.0[8],
        ]))
    }
}

impl ops::Div for U192X192 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self {
        // as we divide 2 fractions with the same denominator (namely 2^192)
        // we are getting a value without a denominator
        // we need to multiply by this denominator to respect the definition
        // doing this is the same as moving the underlying array
        // by three u64 value to the right
        let self_u768_mul_2_196 = U768([
            0,
            0,
            0,
            self.0 .0[0],
            self.0 .0[1],
            self.0 .0[2],
            self.0 .0[3],
            self.0 .0[4],
            self.0 .0[5],
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
            0,
            0,
            0,
            0,
            0,
            0,
        ]);

        let res_u768 = self_u768_mul_2_196 / rhs_u768;
        // assure no overflows happen
        assert!(
            res_u768.0[6] == 0
                && res_u768.0[7] == 0
                && res_u768.0[8] == 0
                && res_u768.0[9] == 0
                && res_u768.0[10] == 0
                && res_u768.0[11] == 0,
            "{}",
            Error::Overflow
        );

        U192X192(U384([
            res_u768.0[0],
            res_u768.0[1],
            res_u768.0[2],
            res_u768.0[3],
            res_u768.0[4],
            res_u768.0[5],
        ]))
    }
}

impl Zero for U192X192 {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    fn zero() -> Self {
        Self(U384::zero())
    }
    fn set_zero(&mut self) {
        self.0.set_zero();
    }
}
