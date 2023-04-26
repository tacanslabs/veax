#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
mod non_wasm;
#[cfg(not(target_arch = "wasm32"))]
pub use non_wasm::*;

pub fn log_str(s: &str) {
    log_str_impl(s);
}

pub fn log(args: std::fmt::Arguments<'_>) {
    log_str(&std::fmt::format(args));
}
