use near_sdk::borsh::BorshSerialize;

mod doubly_linked_list_map;
mod linked_list_map;

pub use doubly_linked_list_map::*;
pub use linked_list_map::*;

pub(crate) fn raw_storage_key(prefix: impl AsRef<[u8]>, key: &impl BorshSerialize) -> Vec<u8> {
    let mut key_buff = Vec::new();
    key.serialize(&mut key_buff).unwrap();
    [prefix.as_ref(), key_buff.as_slice()].concat()
}
