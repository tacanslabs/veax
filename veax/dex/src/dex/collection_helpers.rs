use std::marker::PhantomData;
use std::ops::Deref;

/// Structure which imitates reference to value inside collection
///
/// Used by persistent collection iterators which return deserialized values
/// instead of references
pub struct StorageRef<'a, T>(T, PhantomData<&'a T>);

impl<'a, T> StorageRef<'a, T> {
    /// Creates new `StorageRef` from value
    ///
    /// Note that ability to spawn such pseudo-reference isn't an issue itself
    pub fn new(value: T) -> Self {
        Self(value, PhantomData)
    }
}

impl<'a, T> Deref for StorageRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
/// Wraps by-value iterator and returns values as reference-like `StorageRef`'s
pub struct StorageRefIter<'a, T, I: Iterator<Item = T> + 'a>(I, PhantomData<&'a I>);

impl<'a, T, I: Iterator<Item = T> + 'a> StorageRefIter<'a, T, I> {
    pub fn new(iter: impl IntoIterator<Item = T, IntoIter = I>) -> Self {
        Self(iter.into_iter(), PhantomData)
    }
}

impl<'a, T> StorageRefIter<'a, T, Box<dyn Iterator<Item = T> + 'a>> {
    pub fn new_boxed<I: IntoIterator<Item = T>>(iter: I) -> Self
    where
        <I as IntoIterator>::IntoIter: 'a,
    {
        Self(Box::new(iter.into_iter()), PhantomData)
    }
}

impl<'a, T: 'a, I: Iterator<Item = T> + 'a> Iterator for StorageRefIter<'a, T, I> {
    type Item = StorageRef<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(StorageRef::new)
    }
}
/// Wraps any iterator over 2-tuples and returns 0th element of each one
pub struct PairKeyIter<'a, K, V, I: Iterator<Item = (K, V)> + 'a>(I, PhantomData<&'a I>);

impl<'a, K, V, I: Iterator<Item = (K, V)>> PairKeyIter<'a, K, V, I> {
    pub fn new(iter: impl IntoIterator<IntoIter = I>) -> Self {
        Self(iter.into_iter(), PhantomData)
    }
}

impl<'a, K, V, I: Iterator<Item = (K, V)>> Iterator for PairKeyIter<'a, K, V, I> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(k, _)| k)
    }
}
/// Wraps by-value iterator of 2-tuples and returns 2-tuples
/// where elements are wrapped as reference-like `StorageRef`'s
pub struct StorageRefPairIter<'a, K, V, I: Iterator<Item = (K, V)> + 'a>(I, PhantomData<&'a I>);

impl<'a, K, V, I: Iterator<Item = (K, V)> + 'a> StorageRefPairIter<'a, K, V, I> {
    pub fn new(iter: impl IntoIterator<IntoIter = I>) -> Self {
        Self(iter.into_iter(), PhantomData)
    }
}

impl<'a, K, V> StorageRefPairIter<'a, K, V, Box<dyn Iterator<Item = (K, V)> + 'a>> {
    pub fn new_boxed<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self
    where
        <I as IntoIterator>::IntoIter: 'a,
    {
        Self(Box::new(iter.into_iter()), PhantomData)
    }
}

impl<'a, K: 'a, V: 'a, I: Iterator<Item = (K, V)> + 'a> Iterator
    for StorageRefPairIter<'a, K, V, I>
{
    type Item = (StorageRef<'a, K>, StorageRef<'a, V>);

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|(k, v)| (StorageRef::new(k), StorageRef::new(v)))
    }
}
