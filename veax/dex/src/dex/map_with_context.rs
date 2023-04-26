use super::{ErrorKind, Result};
use crate::error_here;
use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "near")]
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};

pub trait MapContext {
    /// Produces error which is returned when specified key wasn't found
    fn not_found_error() -> ErrorKind;
}
/// Wrapper type for map-like collections which provides some additional capabilities
/// * methods which produce predefined "not found" error
pub struct MapWithContext<T, E: MapContext>(T, PhantomData<E>);

#[cfg(feature = "near")]
impl<T: BorshSerialize, E: MapContext> BorshSerialize for MapWithContext<T, E> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.0.serialize(writer)
    }
}

#[cfg(feature = "near")]
impl<T: BorshDeserialize, E: MapContext> BorshDeserialize for MapWithContext<T, E> {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        BorshDeserialize::deserialize(buf).map(Self::new)
    }
}

impl<T, E: MapContext> From<T> for MapWithContext<T, E> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T, E: MapContext> Deref for MapWithContext<T, E> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, E: MapContext> DerefMut for MapWithContext<T, E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T, E: MapContext> MapWithContext<T, E> {
    pub fn new(inner: T) -> Self {
        Self(inner, PhantomData)
    }
}

impl<T: super::Map, E: MapContext> MapWithContext<T, E> {
    #[track_caller]
    #[inline]
    pub fn try_inspect_or<R>(
        &self,
        key: &T::Key,
        error: ErrorKind,
        inspect_fn: impl FnOnce(&T::Value) -> R,
    ) -> Result<R> {
        self.inspect(key, inspect_fn)
            .ok_or_else(|| error_here!(error))
    }

    #[track_caller]
    #[inline]
    pub fn try_update_or<R>(
        &mut self,
        key: &T::Key,
        error: ErrorKind,
        update_fn: impl FnOnce(&mut T::Value) -> Result<R>,
    ) -> Result<R> {
        self.update(key, update_fn)
            .ok_or_else(|| error_here!(error))?
    }
    /// Tries to find specified key and pass reference to found value to `inspect_fn`
    /// Unlike `dex::Map::inspect`, returns error defined by `E::not_found_error` if entry wasn't found
    #[track_caller]
    #[inline]
    pub fn try_inspect<R>(
        &self,
        key: &T::Key,
        inspect_fn: impl FnOnce(&T::Value) -> R,
    ) -> Result<R> {
        self.try_inspect_or(key, E::not_found_error(), inspect_fn)
    }
    /// Tries to find specified key and pass mutable reference to found value to `update_fn`
    /// Unlike `dex::Map::update`, returns error defined by `E::not_found_error` if entry wasn't found
    #[track_caller]
    #[inline]
    pub fn try_update<R>(
        &mut self,
        key: &T::Key,
        update_fn: impl FnOnce(&mut T::Value) -> Result<R>,
    ) -> Result<R> {
        self.try_update_or(key, E::not_found_error(), update_fn)
    }
}
