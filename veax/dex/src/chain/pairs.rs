use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};

#[derive(
    PartialEq,
    Eq,
    Default,
    BorshDeserialize,
    BorshSerialize,
    Deserialize,
    Debug,
    Serialize,
    Clone,
    Copy,
)]
#[serde(crate = "near_sdk::serde")]
pub struct Pair<T> {
    pub left: T,
    pub right: T,
}

impl<T> Pair<T> {
    pub fn new(left: T, right: T) -> Self {
        Self { left, right }
    }
    /// Maps Pair<T> into Pair<U>
    ///
    /// Needed because we cannot write `impl<T, U: From<T>> From<Pair<T>> for Pair<U>`
    /// due to absence of specialization
    pub fn map<U>(self, mut map_fn: impl FnMut(T) -> U) -> Pair<U> {
        Pair::new(map_fn(self.left), map_fn(self.right))
    }

    pub fn swap_if(self, swap: bool) -> Self {
        if swap {
            Self::new(self.right, self.left)
        } else {
            self
        }
    }
}

impl<T, U: From<T>> From<(T, T)> for Pair<U> {
    fn from(pair: (T, T)) -> Self {
        Self {
            left: pair.0.into(),
            right: pair.1.into(),
        }
    }
}

impl<T, U: From<T>> From<[T; 2]> for Pair<U> {
    fn from([a, b]: [T; 2]) -> Self {
        (a, b).into()
    }
}

impl<T, U: From<T>> From<Pair<T>> for (U, U) {
    fn from(pair: Pair<T>) -> Self {
        (pair.left.into(), pair.right.into())
    }
}

impl<T, U: From<T>> From<Pair<T>> for [U; 2] {
    fn from(pair: Pair<T>) -> Self {
        [pair.left.into(), pair.right.into()]
    }
}
