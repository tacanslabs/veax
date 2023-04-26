#![allow(clippy::all, clippy::pedantic)]

#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Serialize, Serializer},
};

use super::Error;
use num_traits::Zero;
use std::cmp::Ordering;
use std::{cmp, iter, mem, ops};

#[cfg(feature = "near")]
pub mod near {
    pub trait ConditionalSerialisation {}
    impl<T> ConditionalSerialisation for T {}
}

#[cfg(feature = "near")]
pub use near::ConditionalSerialisation;

use crate::chain::Float;
use crate::ensure;
use crate::fp::try_float_to_ufp::try_mantissa_exponent_to_ufp;

#[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize))]
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    pub value: T,
    pub non_negative: bool,
}

impl<T> Default for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation
        + Zero,
{
    fn default() -> Self {
        Self::zero()
    }
}
impl<T> PartialOrd for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation
        + Zero,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation
        + Zero,
{
    fn cmp(&self, other: &Self) -> Ordering {
        if self.value.is_zero() && other.value.is_zero() {
            return Ordering::Equal;
        }
        match (self.non_negative, other.non_negative) {
            (true, true) => self.value.cmp(&other.value),
            (false, false) => self.value.cmp(&other.value).reverse(),
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
        }
    }
}

#[cfg(feature = "near")]
impl<T> Serialize for Signed<T>
where
    T: std::fmt::Display
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let num: String = if self.non_negative {
            "".to_string()
        } else {
            "-".to_string()
        } + &self.value.to_string();
        Serialize::serialize(&num, serializer)
    }
}

impl<T> ops::Add for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        if self.non_negative == rhs.non_negative {
            Self {
                value: self.value + rhs.value,
                non_negative: self.non_negative,
            }
        } else if self.value > rhs.value {
            Self {
                value: self.value - rhs.value,
                non_negative: self.non_negative,
            }
        } else {
            Self {
                value: rhs.value - self.value,
                non_negative: rhs.non_negative,
            }
        }
    }
}

impl<T> ops::AddAssign for Signed<T>
where
    T: ops::Add
        + ops::AddAssign
        + ops::SubAssign
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    fn add_assign(&mut self, mut rhs: Self) {
        if self.non_negative == rhs.non_negative {
            self.value += rhs.value;
        } else if self.value > rhs.value {
            self.value -= rhs.value;
        } else {
            mem::swap(&mut self.value, &mut rhs.value);
            self.value -= rhs.value;
            self.non_negative = rhs.non_negative;
        }
    }
}

impl<T> ops::Neg for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation
        + Zero,
{
    type Output = Self;

    fn neg(mut self) -> Self::Output {
        self.non_negative = self.value.is_zero() || !self.non_negative;
        self
    }
}

impl<T> Zero for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
    T: Zero,
{
    fn is_zero(&self) -> bool {
        self.value == T::zero()
    }

    fn zero() -> Self {
        Self {
            value: T::zero(),
            non_negative: true,
        }
    }

    fn set_zero(&mut self) {
        self.value.set_zero();
        self.non_negative = true;
    }
}

impl<T> ops::Sub for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        let rhs_negated = Self {
            value: rhs.value,
            non_negative: !rhs.non_negative,
        };
        ops::Add::add(self, rhs_negated)
    }
}

impl<T> ops::Mul for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            value: self.value * rhs.value,
            non_negative: self.non_negative == rhs.non_negative,
        }
    }
}

impl<T> ops::Div for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            value: self.value / rhs.value,
            non_negative: self.non_negative == rhs.non_negative,
        }
    }
}

impl<T> iter::Sum for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
    T: Zero,
{
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::zero(), |sum, item| sum + item)
    }
}

impl<T> From<T> for Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    fn from(value: T) -> Self {
        Self {
            value,
            non_negative: true,
        }
    }
}

impl<T> Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    pub fn try_from_unsigned<I: TryInto<T>>(value: I) -> Result<Self, I::Error> {
        Ok(Self {
            value: value.try_into()?,
            non_negative: true,
        })
    }
}

impl<T> Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
    T: Zero,
{
    pub fn try_into_unsigned(self) -> Result<T, Error> {
        ensure!(
            self.non_negative || self.value.is_zero(),
            Error::NegativeToUnsigned
        );
        Ok(self.value)
    }
}

impl<T> Signed<T>
where
    T: ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    #[allow(dead_code)]
    pub fn neg_from(self: Self) -> Self {
        if !self.non_negative {
            self
        } else {
            Self {
                value: self.value,
                non_negative: !self.non_negative,
            }
        }
    }

    #[allow(dead_code)]
    pub fn pos_from(self: Self) -> Self {
        if self.non_negative {
            self
        } else {
            Self {
                value: self.value,
                non_negative: self.non_negative,
            }
        }
    }
}

pub(crate) fn try_from_float<UNDERLYING, const TOT_WORDS: usize, const FRACT_WORDS: usize>(
    v: crate::chain::Float,
) -> Result<Signed<UNDERLYING>, Error>
where
    UNDERLYING: From<[u64; TOT_WORDS]>
        + ops::Add
        + ops::Add<Output = UNDERLYING>
        + ops::Sub
        + ops::Sub<Output = UNDERLYING>
        + ops::Mul
        + ops::Mul<Output = UNDERLYING>
        + ops::Div
        + ops::Div<Output = UNDERLYING>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
{
    let (mantissa, exponent, sign) = v.integer_decode();
    Ok(Signed {
        value: try_mantissa_exponent_to_ufp::<UNDERLYING, TOT_WORDS, FRACT_WORDS>(
            mantissa, exponent,
        )?,
        non_negative: sign >= 0,
    })
}

pub(crate) fn into_float<UNDERLYING, const TOT_SIZE: usize, const FRACT_SIZE: usize>(
    v: Signed<UNDERLYING>,
) -> Float
where
    UNDERLYING: ops::Add
        + ops::Add<Output = UNDERLYING>
        + ops::Sub
        + ops::Sub<Output = UNDERLYING>
        + ops::Mul
        + ops::Mul<Output = UNDERLYING>
        + ops::Div
        + ops::Div<Output = UNDERLYING>
        + cmp::Ord
        + cmp::Eq
        + ConditionalSerialisation,
    Float: From<UNDERLYING>,
{
    let abs_value = Float::from(v.value);
    if v.non_negative {
        abs_value
    } else {
        -abs_value
    }
}
