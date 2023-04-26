extern crate core;

mod chain;
#[cfg(not(target = "wasm32"))]
pub mod dex;
#[cfg(target = "wasm32")]
mod dex;
#[cfg(not(target = "wasm32"))]
pub mod fp;
#[cfg(target = "wasm32")]
mod fp;

pub use chain::wasm::*;

#[cfg(not(target = "wasm32"))]
pub use chain::*;
