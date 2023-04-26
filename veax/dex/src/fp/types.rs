#![allow(clippy::all, clippy::pedantic)]
#[cfg(feature = "near")]
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

use uint::construct_uint;

use super::traits::{IntegerSqrt, OverflowMul};
use num_traits::Zero;

construct_uint! {
    /// 128-bit unsigned integer, constructed out of 2 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U128(2);
}

construct_uint! {
    /// 256-bit unsigned integer, constructed out of 4 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U256(4);
}

construct_uint! {
    /// 384-bit unsigned integer, constructed out of 6 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U320(5);
}

construct_uint! {
    /// 384-bit unsigned integer, constructed out of 6 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U384(6);
}

construct_uint! {
    /// 448-bit unsigned integer, constructed out of 7 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U448(7);
}

construct_uint! {
    /// 512-bit unsigned integer, constructed out of 8 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U512(8);
}

construct_uint! {
    /// 576-bit unsigned integer, constructed out of 9 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U576(9);
}

construct_uint! {
    /// 768-bit unsigned integer, constructed out of 12 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U768(12);
}

construct_uint! {
    /// 896-bit unsigned integer, constructed out of 14 words x 64 bits.
    #[cfg_attr(feature = "near", derive(BorshDeserialize, BorshSerialize, Deserialize, Serialize))]
    pub struct U896(14);
}

#[allow(unused)]
macro_rules! impl_uint {
    ($name:ident, $size_words:literal) => {
        impl Zero for $name {
            fn is_zero(&self) -> bool {
                self.0.iter().all(|word| *word == 0)
            }

            fn zero() -> Self {
                Self::zero()
            }

            fn set_zero(&mut self) {
                for word in self.0.iter_mut() {
                    *word = 0;
                }
            }
        }

        impl IntegerSqrt for $name {
            fn integer_sqrt(&self) -> Self {
                self.integer_sqrt()
            }
        }

        impl OverflowMul for $name {
            fn overflowing_mul(&self, rhs: Self) -> (Self, bool) {
                <Self>::overflowing_mul(*self, rhs)
            }
        }

        impl From<[u64; $size_words]> for $name {
            fn from(inner_value: [u64; $size_words]) -> Self {
                Self(inner_value)
            }
        }
    };
}

impl_uint!(U256, 4);
impl_uint!(U384, 6);
impl_uint!(U448, 7);
impl_uint!(U512, 8);
impl_uint!(U576, 9);
impl_uint!(U768, 12);
impl_uint!(U896, 14);
