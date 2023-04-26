use std::marker::PhantomData;

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, IntoStorageKey};

use crate::raw_storage_key;

/// Key-value map that allows iteration and removing entries.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct DoublyLinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    key_prefix: Vec<u8>,
    length: u64,

    /// We store head node in-place to reduce the number of storage operations.
    ///
    /// If the node were stored in storage, when inserting, we would need to read the head node from storage,
    /// change the `prev_key`, write it back to storage, and then write a new node as well.
    ///
    /// By keeping head node in-place we only need to perform one storage write when inserting.
    head: Option<(K, Vec<u8>)>,

    _phantom_data: PhantomData<V>,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct Node<K, V> {
    value: V,
    prev_key: Option<K>,
    next_key: Option<K>,
}

impl<K, V> DoublyLinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    pub fn new<S>(key_prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            key_prefix: key_prefix.into_storage_key(),
            length: 0,
            head: None,
            _phantom_data: PhantomData,
        }
    }

    /// Inserts new value into map.
    ///
    /// Performs up to 1 storage read and up to 1 storage write.
    pub fn insert(&mut self, key: &K, mut value: V) -> Option<V> {
        if let Some(mut node) = self.get_node(key) {
            // Update value if the map already contains the key.
            core::mem::swap(&mut value, &mut node.value);
            self.set_node(key, &node);
            Some(value)
        } else {
            let next_key = match self.head.take() {
                None => None,
                Some((head_key, head_node_bytes)) => {
                    let mut head_node = Node::deserialize(&mut head_node_bytes.as_slice()).unwrap();
                    // Push head node into storage.
                    head_node.prev_key = Some(key.clone());
                    self.set_stored_node(&head_key, &head_node);
                    Some(head_key)
                }
            };

            let node = Node {
                value,
                prev_key: None,
                next_key,
            };
            // Insert new key and value into head node.
            self.head = Some((key.clone(), Self::serialize_node(&node)));
            self.length += 1;

            None
        }
    }

    /// Checks whether the map contains a key.
    ///
    /// Performs up to 1 storage read.
    pub fn contains_key(&self, key: &K) -> bool {
        self.head
            .as_ref()
            .map_or(false, |(head_key, _head_node)| head_key == key)
            || self.contains_stored_node(key)
    }

    /// Returns respective value for the specified key.
    ///
    /// Performs up to 1 storage read.
    pub fn get(&self, key: &K) -> Option<V> {
        self.get_node(key).map(|node| node.value)
    }

    /// Removes key and respective value from the map.
    ///
    /// If the map doesn't contain the key, performs 1 storage read.
    /// If the map contains the key, performs up to 3 storage reads, up to 2 storage writes, and 1 storage remove.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let node = self.get_node(key)?;
        self.length -= 1;

        match node.prev_key {
            Some(ref prev_key) => {
                // This is not a head node, relinking the list.
                self.remove_stored_node(key);

                let mut prev_node = self.get_node(prev_key).unwrap();
                prev_node.next_key = node.next_key.clone();
                self.set_node(prev_key, &prev_node);

                if let Some(ref next_key) = node.next_key {
                    let mut next_node = self.get_node(next_key).unwrap();
                    next_node.prev_key = node.prev_key.clone();
                    self.set_node(next_key, &next_node);
                }
            }

            None => {
                if let Some(next_key) = node.next_key {
                    // This is a head node, pop the next node from the storage.

                    let mut next_node = self.get_node(&next_key).unwrap();
                    self.remove_stored_node(&next_key);

                    next_node.prev_key = None;
                    self.head = Some((next_key, Self::serialize_node(&next_node)));
                } else {
                    // This is a last node, clear the list.
                    self.head = None;
                }
            }
        }

        Some(node.value)
    }

    /// Removes and returns a key-value pair from the map.
    ///
    /// Performs up to 1 storage read and up to 1 storage remove.
    pub fn pop(&mut self) -> Option<(K, V)> {
        let key = self.head.as_ref().map(|(key, _node)| key.clone())?;
        let value = self.remove(&key).unwrap();
        Some((key, value))
    }

    /// Returns iterator over key-value pairs.
    ///
    /// Performs up to 1 storage read per `.next()` call.
    pub fn iter(&self) -> DoublyLinkedListMapIter<K, V> {
        DoublyLinkedListMapIter {
            map: self,
            key: self.head.as_ref().map(|head| head.0.clone()),
        }
    }

    /// Removes all entries.
    ///
    /// Performs up to 1 storage read and uo to 1 storage remove per entry.
    pub fn clear(&mut self) {
        let mut next_key = self.head.take().map(|(key, _node)| key);

        while let Some(key) = next_key {
            let node = self.get_node(&key).unwrap();
            self.remove_stored_node(&key);
            next_key = node.next_key;
        }

        self.length = 0;
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

    /// Gets node either from the state (head) or from storage.
    fn get_node(&self, key: &K) -> Option<Node<K, V>> {
        if let Some((head_key, head_node_bytes)) = self.head.as_ref() {
            if head_key == key {
                return Some(Self::deserialize_node(head_node_bytes));
            }
        }

        self.get_stored_node(key)
    }

    /// Writes node either to the state (head) or to storage.
    fn set_node(&mut self, key: &K, node: &Node<K, V>) {
        if let Some((head_key, head_node_bytes)) = self.head.as_mut() {
            if head_key == key {
                *head_node_bytes = Self::serialize_node(node);
                return;
            }
        }

        self.set_stored_node(key, node);
    }

    fn get_stored_node(&self, key: &K) -> Option<Node<K, V>> {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_read(&raw_key).map(|bytes| Node::deserialize(&mut bytes.as_slice()).unwrap())
    }

    fn set_stored_node(&mut self, key: &K, node: &Node<K, V>) {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        let mut node_bytes = Vec::new();
        node.serialize(&mut node_bytes).unwrap();
        env::storage_write(&raw_key, &node_bytes);
    }

    fn remove_stored_node(&mut self, key: &K) {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_remove(&raw_key);
    }

    fn contains_stored_node(&self, key: &K) -> bool {
        let raw_key = raw_storage_key(&self.key_prefix, key);
        env::storage_has_key(&raw_key)
    }

    fn serialize_node(node: &Node<K, V>) -> Vec<u8> {
        let mut bytes = Vec::new();
        node.serialize(&mut bytes).unwrap();
        bytes
    }
    fn deserialize_node(mut bytes: &[u8]) -> Node<K, V> {
        Node::deserialize(&mut bytes).unwrap()
    }
}

pub struct DoublyLinkedListMapIter<'a, K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    map: &'a DoublyLinkedListMap<K, V>,
    key: Option<K>,
}

impl<'a, K, V> Iterator for DoublyLinkedListMapIter<'a, K, V>
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

impl<'a, K, V> IntoIterator for &'a DoublyLinkedListMap<K, V>
where
    K: Eq + Clone + BorshSerialize + BorshDeserialize,
    V: BorshSerialize + BorshDeserialize,
{
    type Item = (K, V);

    type IntoIter = DoublyLinkedListMapIter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
