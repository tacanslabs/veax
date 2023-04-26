pub use crate::chain::Float;

/// Generates floating-point type wrapper with interface compliant with `dex` module requirements
///
/// Generates all necessary trait implementations, derives etc. etc. using simple expressions syntax
#[macro_export]
#[allow(clippy::crate_in_macro_def)]
macro_rules! wrap_float {
    (
        $(#[$attr:meta])* $pub:vis $inner:ty {
            MANTISSA_BITS: $mantissa_bits_expr:expr,
            MAX: $max_value_expr:expr,
            zero: $zero_expr:expr,
            one: $one_expr:expr,
            cmp: |$cmp_l:ident, $cmp_r:ident| $cmp_expr:expr,
            classify: |$classify_arg:ident| $classify_expr:expr,
            add: |$add_l:ident, $add_r:ident| $add_expr:expr,
            sub: |$sub_l:ident, $sub_r:ident| $sub_expr:expr,
            mul: |$mul_l:ident, $mul_r:ident| $mul_expr:expr,
            div: |$div_l:ident, $div_r:ident| $div_expr:expr,
            rem: |$rem_l:ident, $rem_r:ident| $rem_expr:expr,
            sqrt: |$sqrt_arg:ident| $sqrt_expr:expr,
            round: |$round_arg:ident| $round_expr:expr,
            floor: |$floor_arg:ident| $floor_expr:expr,
            ceil: |$ceil_arg:ident| $ceil_expr:expr,
            from u64: |$from_u64_arg:ident| $from_u64_expr:expr,
            from BasisPoints: |$from_basispoints_in:ident| $from_basispoints_expr:expr,
            from UInt: |$from_uint_in:ident| $from_uint_expr:expr,
            try_into UInt: |$try_to_uint_in:ident| $try_to_uint_expr:expr,
            integer_decode: |$integer_decode_arg:ident| $integer_decode_expr:expr,
            try_into_lossy FixedPoint: |$try_into_lossy_fixedpoint_in:ident| $try_into_lossy_fixedpoint_expr:expr,
        }
    ) => {
        $(#[$attr])*
        #[derive(Copy, Clone)]
        #[repr(transparent)]
        $pub struct Float($inner);

        impl Float {
            pub const MANTISSA_BITS: usize = ($mantissa_bits_expr) as usize;
            pub const MAX: Self = Self($max_value_expr);

            pub fn zero() -> Self {
                Self($zero_expr)
            }

            pub fn one() -> Self {
                Self($one_expr)
            }

            pub const fn from_bits(bits: u64) -> Self {
                // Actually safe - emulates `f64::from_bits`, which isn't
                // `const`-stable yet, unlike `transmute`
                Self(unsafe { std::mem::transmute(bits) })
            }

            pub const fn to_bits(&self) -> u64 {
                // Actually safe - emulates `f64::to_bits`, which isn't
                // `const`-stable yet, unlike `transmute`
                unsafe { std::mem::transmute(self.0) }
            }

            pub fn sqrt(self) -> Self {
                let $sqrt_arg = self.0;
                Self($sqrt_expr)
            }

            pub fn classify(&self) -> std::num::FpCategory {
                let $classify_arg = &self.0;
                $classify_expr
            }

            pub fn is_nan(&self) -> bool {
                self.classify() == std::num::FpCategory::Nan
            }

            pub fn is_infinity(&self) -> bool {
                self.classify() == std::num::FpCategory::Infinite
            }

            pub fn is_zero(&self) -> bool {
                self.classify() == std::num::FpCategory::Zero
            }

            pub fn is_normal(&self) -> bool {
                self.classify() == std::num::FpCategory::Normal
            }

            pub fn is_subnormal(&self) -> bool {
                self.classify() == std::num::FpCategory::Subnormal
            }

            pub fn round(self) -> Self {
                let $round_arg = self.0;
                Self($round_expr)
            }
            pub fn floor(self) -> Self {
                let $floor_arg = self.0;
                Self($floor_expr)
            }

            pub fn ceil(self) -> Self {
                let $ceil_arg = self.0;
                Self($ceil_expr)
            }

            pub fn min(self, rhs: Self) -> Self {
                match self.partial_cmp(&rhs) {
                    Some(std::cmp::Ordering::Less) => self,
                    Some(_) => rhs,
                    None => if self.is_nan() { rhs } else { self },
                }
            }

            pub fn max(self, rhs: Self) -> Self {
                match self.partial_cmp(&rhs) {
                    Some(std::cmp::Ordering::Greater) => self,
                    Some(_) => rhs,
                    None => if self.is_nan() { rhs } else { self },
                }
            }

            pub fn try_into_lossy(self) -> std::result::Result<crate::chain::FixedPoint, crate::fp::Error> {
                let $try_into_lossy_fixedpoint_in = self.0;
                $try_into_lossy_fixedpoint_expr
            }

            pub fn integer_decode(self) -> (u64, i16, i8) {
                let $integer_decode_arg = self.0;
                $integer_decode_expr
            }

            // TODO: consider to optmize
            pub fn powi(&self, p: i32) -> Self {
                let mut res = Self::one();
                for _ in 0..p.abs() {
                    res *= *self;
                }
                if p < 0 {
                    res = Self::one() / res;
                }
                res
            }

            pub fn abs(&self) -> Self {
                Self(self.0.abs())
            }

            pub fn recip(&self) -> Self {
                Self::one() / *self
            }
        }

        impl std::cmp::PartialEq<Self> for Float {
            fn eq(&self, rhs: &Self) -> bool {
                self.partial_cmp(rhs) == Some(std::cmp::Ordering::Equal)
            }
        }

        impl std::cmp::PartialOrd<Self> for Float {
            fn partial_cmp(&self, rhs: &Self) -> Option<std::cmp::Ordering> {
                let $cmp_l = &self.0;
                let $cmp_r = &rhs.0;
                ($cmp_expr)
            }
        }

        impl std::default::Default for Float {
            fn default() -> Self {
                Float::zero()
            }
        }

        impl std::convert::From<$inner> for Float {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl std::convert::From<Float> for $inner {
            fn from(value: Float) -> Self {
                value.0
            }
        }

        impl std::convert::From<u64> for Float {
            fn from($from_u64_arg: u64) -> Self {
                Self($from_u64_expr)
            }
        }

        impl std::convert::From<i32> for Float {
            fn from(value: i32) -> Float {
                let float_abs = Self::from(value.abs() as u64);
                if value >= 0 {
                    float_abs
                } else {
                    -float_abs
                }
            }
        }

        impl std::convert::From<$crate::dex::BasisPoints> for Float {
            fn from($from_basispoints_in: $crate::dex::BasisPoints) -> Self {
                Self($from_basispoints_expr)
            }
        }

        impl std::convert::From<crate::chain::UInt> for Float {
            fn from($from_uint_in: crate::chain::UInt) -> Self {
                Self($from_uint_expr)
            }
        }

        impl std::convert::TryFrom<Float> for crate::chain::UInt {
            type Error = crate::fp::Error;

            fn try_from(value: Float) -> std::result::Result<crate::chain::UInt, Self::Error> {
                let $try_to_uint_in = value.0;
                $try_to_uint_expr
            }
        }

        impl std::convert::TryFrom<Float> for i32 {
            type Error = crate::fp::Error;
            fn try_from(value: Float) -> std::result::Result<i32, Self::Error> {
                let value_abs_uint = crate::chain::UInt::try_from(value.abs())?;
                let Ok(abs_max) = crate::chain::UInt::try_from(i32::MAX) else {
                    // i32::MAX should always fit into our UInt
                    unreachable!()
                };
                if value_abs_uint <= abs_max {
                    let Ok(value_abs_i32) = i32::try_from(value_abs_uint) else {
                        // value_abs_uint is always within `[0..=i32::MAX]` due to prev checks
                        unreachable!()
                    };
                    if value >= Float::zero() {
                        Ok(value_abs_i32)
                    } else {
                        Ok(-value_abs_i32)
                    }
                } else {
                    Err($crate::fp::Error::Overflow)
                }
            }
        }

        impl std::ops::Add<Self> for Float {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                let $add_l = self.0;
                let $add_r = rhs.0;
                Self($add_expr)
            }
        }

        impl std::ops::AddAssign<Self> for Float {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl std::ops::Sub<Self> for Float {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                let $sub_l = self.0;
                let $sub_r = rhs.0;
                Self($sub_expr)
            }
        }

        impl std::ops::SubAssign<Self> for Float {
            fn sub_assign(&mut self, rhs: Self) {
                *self = *self - rhs;
            }
        }

        impl std::ops::Mul<Self> for Float {
            type Output = Self;

            fn mul(self, rhs: Self) -> Self::Output {
                let $mul_l = self.0;
                let $mul_r = rhs.0;
                Self($mul_expr)
            }
        }

        impl std::ops::MulAssign<Self> for Float {
            fn mul_assign(&mut self, rhs: Self) {
                *self = *self * rhs;
            }
        }

        impl std::ops::Div<Self> for Float {
            type Output = Self;

            fn div(self, rhs: Self) -> Self::Output {
                let $div_l = self.0;
                let $div_r = rhs.0;
                Self($div_expr)
            }
        }

        impl std::ops::DivAssign<Self> for Float {
            fn div_assign(&mut self, rhs: Self) {
                *self = *self / rhs;
            }
        }

        impl std::ops::Rem<Self> for Float {
            type Output = Self;

            fn rem(self, rhs: Self) -> Self::Output {
                let $rem_l = self.0;
                let $rem_r = rhs.0;
                Self($rem_expr)
            }
        }

        impl std::ops::RemAssign<Self> for Float {
            fn rem_assign(&mut self, rhs: Self) {
                *self = *self % rhs;
            }
        }

        impl std::iter::Product for Float {
            fn product<I: Iterator<Item = Self>>(iter: I) -> Self {
                iter.fold(Float::one(), |product, el| product * el)
            }
        }

        impl std::ops::Neg for Float {
            type Output = Self;

            fn neg(self) -> Self::Output {
                Float(self.0.neg())
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        impl std::fmt::Display for Float {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(&self, f)
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        impl std::fmt::Debug for Float {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Debug::fmt(&f64::from_bits(self.to_bits()), f)
            }
        }
    };
}
