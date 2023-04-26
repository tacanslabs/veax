pub trait OverflowMul {
    fn overflowing_mul(&self, rhs: Self) -> (Self, bool)
    where
        Self: Sized;
}

pub trait IntegerSqrt {
    fn integer_sqrt(&self) -> Self;
}
