pub fn log_str_impl(msg: &str) {
    near_sdk::env::log_str(msg);
}
