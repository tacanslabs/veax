use std::marker::PhantomData;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, IntoStorageKey};

use crate::raw_storage_key;

/// Key-value map that allows iteration.
///
/// This map doesn't allow removing random entries, but allows to drain itself using `LinkedListMap::pop` mehtod.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct LinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    key_prefix: Vec<u8>,
    length: u64,
    next_key: Option<K>,
    _phantom_data: PhantomData<V>,
}

#[derive(Debug, BorshDeserialize)]
struct Node<K, V> {
    value: V,
    next_key: Option<K>,
}

#[derive(BorshSerialize)]
struct NodeRef<'a, K, V> {
    value: &'a V,
    next_key: Option<K>,
}

impl<K, V> LinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    pub fn new<S>(key_prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            length: 0,
            key_prefix: key_prefix.into_storage_key(),
            next_key: None,
            _phantom_data: PhantomData,
        }
    }

    /// Inserts new value into map.
    ///
    /// Performs 1 storage read and 1 storage write.
    pub fn insert(&mut self, key: &K, value: &V) -> Option<V> {
        if let Some(node) = self.get_node(key) {
            // Update value if the map already contains the key.
            let new_node = NodeRef {
                value,
                next_key: node.next_key.clone(),
            };
            self.set_node(key, &new_node);
            Some(node.value)
        } else {
            let node = NodeRef {
                value,
                next_key: self.next_key.take(),
            };
            self.set_node(key, &node);
            self.length += 1;
            self.next_key = Some(key.clone());
            None
        }
    }

    /// Checks whether the map contains a key.
    ///
    /// Performs up to 1 storage read.
    pub fn contains_key(&self, key: &K) -> bool {
        self.contains_node(key)
    }

    /// Returns respective value for the specified key.
    ///
    /// Performs up to 1 storage read.
    pub fn get(&self, key: &K) -> Option<V> {
        self.get_node(key).map(|node| node.value)
    }

    /// Removes and returns a key-value pair from the map.
    ///
    /// Performs up to 1 storage read and up to 1 storage remove.
    pub fn pop(&mut self) -> Option<(K, V)> {
        let key = self.next_key.take()?;
        let node = self.get_node(&key).unwrap();
        self.remove_node(&key);
        self.length -= 1;
        self.next_key = node.next_key;
        Some((key, node.value))
    }

    /// Returns iterator over key-value pairs.
    ///
    /// Performs up to 1 storage read per `.next()` call.
    pub fn iter(&self) -> LinkedListMapIter<K, V> {
        LinkedListMapIter {
            map: self,
            key: self.next_key.clone(),
        }
    }

    /// Removes all entries.
    ///
    /// Performs up to 1 storage read and uo to 1 storage remove per entry.
    pub fn clear(&mut self) {
        while let Some(_entry) = self.pop() {
            // do nothing, just pop all entries
        }
    }

    /// Returns number of entries in the collection.
    pub fn len(&self) -> usize {
        self.length.try_into().unwrap()
    }

    /// Checks if the collection is empty.
    ///
    /// Doesn't access storage.
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    fn get_node(&self, key: &K) -> Option<Node<K, V>> {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_read(&raw_key).map(|bytes| Node::deserialize(&mut bytes.as_slice()).unwrap())
    }

    fn set_node(&mut self, key: &K, node: &NodeRef<K, V>) {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        let mut node_bytes = Vec::new();
        node.serialize(&mut node_bytes).unwrap();
        env::storage_write(&raw_key, &node_bytes);
    }

    fn remove_node(&mut self, key: &K) {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_remove(&raw_key);
    }

    fn contains_node(&self, key: &K) -> bool {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_has_key(&raw_key)
    }
}

pub struct LinkedListMapIter<'a, K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    map: &'a LinkedListMap<K, V>,
    key: Option<K>,
}

impl<'a, K, V> Iterator for LinkedListMapIter<'a, K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let key = self.key.take()?;
        let node = self.map.get_node(&key).unwrap();
        self.key = node.next_key;
        Some((key, node.value))
    }
}

impl<'a, K, V> IntoIterator for &'a LinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Item = (K, V);

    type IntoIter = LinkedListMapIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
