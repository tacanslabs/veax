#![allow(clippy::all, clippy::pedantic)]

use std::{marker::PhantomData, ops};

use super::traits::{IntegerSqrt, OverflowMul};
use super::Error;
use num_traits::Zero;

#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct UnsignedSymmetric<T, M, const S: usize, const S_AND_A_HALF: usize>
where
    T: From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]>,
{
    pub(super) underlying: T,
    _marker_m: PhantomData<M>,
}
#[allow(dead_code)]
impl<T, M, const S: usize, const S_AND_A_HALF: usize> UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    fn new(value: T) -> Self {
        UnsignedSymmetric {
            underlying: value,
            _marker_m: PhantomData,
        }
    }

    fn zero() -> Self {
        UnsignedSymmetric {
            underlying: T::zero(),
            _marker_m: PhantomData,
        }
    }

    pub fn fract(self) -> Self {
        // the fractional part is saved in the first part
        // of the underlying array therefore the underlying
        // array contains zeroth and first values of the
        // array, and the second part is zeroed, as the
        // integer part is zero
        let mut res: [u64; S] = self.underlying.as_ref().try_into().unwrap();
        let half = S / 2;
        for i in half..S {
            res[i] = 0;
        }
        UnsignedSymmetric::new(T::from(res))
    }

    pub fn floor(self) -> Self {
        // the integer part is saved in the second part
        // of the underlying array therefore the underlying
        // array contains second and third values of the
        // array, and the first part is zeroed, as the
        // fractional part is zero
        let mut res: [u64; S] = self.underlying.as_ref().try_into().unwrap();
        let half = S / 2;
        for i in 0..half {
            res[i] = 0;
        }
        UnsignedSymmetric::new(T::from(res))
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>
        + IntegerSqrt
        + ops::Shl<usize, Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    #[allow(dead_code)]
    pub fn integer_sqrt(self) -> Self {
        let mut integer_sqrt = self.underlying.integer_sqrt();

        let mut res: [u64; S] = [0; S];

        if S % 4 == 0 {
            let as_ref = integer_sqrt.as_ref();
            let quarter = S / 4;
            for i in 0..S - quarter {
                res[i + quarter] = as_ref[i];
            }
        } else {
            let quarter = (S * 64) / 4;
            integer_sqrt = integer_sqrt << quarter;
            res = integer_sqrt.as_ref()[0..S].try_into().unwrap();
        }
        UnsignedSymmetric::new(T::from(res))
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> ops::Add
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self {
        UnsignedSymmetric::new(self.underlying + rhs.underlying)
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> ops::Sub
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        UnsignedSymmetric::new(self.underlying - rhs.underlying)
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> ops::SubAssign
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Clone
        + Copy
        + Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: Clone
        + Copy
        + OverflowMul
        + ops::Div
        + ops::Div<Output = M>
        + From<[u64; S_AND_A_HALF]>
        + AsRef<[u64]>,
{
    fn sub_assign(&mut self, other: Self) {
        let value = *self;
        *self = value - other;
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> ops::Mul
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // The underlying S*64-bit integers are multiplied in sufficiently high precision,
        // and converted to UnsignedSymmetric<T, M, S> taking the scale into account.
        // The excessive precision bits are truncated. As the product must fit into S*64-bit integer,
        // it is sufficient to perform the multiplication in 1.5*S*64-bit integers.
        // For example, for S=4 (i.e. U128X128 type with underlying U256):
        // UnsignedSymmetric<T, M, S> x UnsignedSymmetric<T, M, S>
        //    = U256/2**128 x U256/2**128 = U384/2**256
        //    = U128x256  --conv->  UnsignedSymmetric<T, M, S>

        let mut self_s_and_a_half_array: [u64; S_AND_A_HALF] = [0; S_AND_A_HALF];

        for i in 0..S {
            self_s_and_a_half_array[i] = self.underlying.as_ref()[i];
        }

        let self_mul_value = M::from(self_s_and_a_half_array);

        let mut rhs_s_and_a_half_array: [u64; S_AND_A_HALF] = [0; S_AND_A_HALF];

        for i in 0..S {
            rhs_s_and_a_half_array[i] = rhs.underlying.as_ref()[i];
        }

        let rhs_mul_value = M::from(rhs_s_and_a_half_array);

        // The product of two UnsignedSymmetric<T, M, D, S> may not necessarily fit into UnsignedSymmetric<T, M, D, S>,
        // so we need to check for overflow:
        let (res_mul_value, is_overflow) = self_mul_value.overflowing_mul(rhs_mul_value);
        assert!(!is_overflow, "{}", Error::Overflow);

        let inner = res_mul_value.as_ref();

        let starting_point = S / 2;

        let res: [u64; S] = inner[starting_point..S_AND_A_HALF].try_into().unwrap();

        UnsignedSymmetric::new(T::from(res))
    }
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> ops::Div
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    fn div(self, rhs: Self) -> Self {
        let mut self_double_s_array: [u64; S_AND_A_HALF] = [0; S_AND_A_HALF];

        let half_s = S / 2;

        // as we divide 2 fractions with the same denominator (namely 2^128)
        // we are getting a value without a denominator
        // we need to multiply by this denominator to respect the definition
        // doing this is the same as moving the underlying array
        // by two u64 value to the right
        for i in half_s..S + half_s {
            self_double_s_array[i] = self.underlying.as_ref()[i - half_s];
        }

        let self_div_value = M::from(self_double_s_array);

        let mut rhs_double_s_array: [u64; S_AND_A_HALF] = [0; S_AND_A_HALF];

        for i in 0..S {
            rhs_double_s_array[i] = rhs.underlying.as_ref()[i];
        }

        let rhs_div_value = M::from(rhs_double_s_array);

        let res_div_value = self_div_value / rhs_div_value;

        let inner = res_div_value.as_ref();
        // assure no overflows happen
        for i in S..S_AND_A_HALF {
            assert!(inner[i] == 0, "{}", Error::Overflow);
        }

        let res: [u64; S] = inner[0..S].try_into().unwrap();

        UnsignedSymmetric::new(T::from(res))
    }

    type Output = Self;
}

impl<T, M, const S: usize, const S_AND_A_HALF: usize> std::iter::Sum
    for UnsignedSymmetric<T, M, S, S_AND_A_HALF>
where
    T: Zero
        + From<[u64; S]>
        + AsRef<[u64]>
        + ops::Add
        + ops::Add<Output = T>
        + ops::Sub
        + ops::Sub<Output = T>
        + ops::Mul
        + ops::Mul<Output = T>
        + ops::Div
        + ops::Div<Output = T>,
    M: OverflowMul + ops::Div + ops::Div<Output = M> + From<[u64; S_AND_A_HALF]> + AsRef<[u64]>,
{
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut s = Self::zero();
        for i in iter {
            s = s + i;
        }
        s
    }
}
