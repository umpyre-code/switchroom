extern crate env_logger;
#[macro_use]
extern crate log;
extern crate switchroom;

use std::sync::Arc;
use switchroom::{config, storage};

pub fn main() {
    use std::env;

    ::env_logger::init();

    config::load_config();

    // Allow disablement of metrics reporting for testing
    if env::var_os("DISABLE_INSTRUMENTED").is_none() {
        instrumented::init(&config::CONFIG.metrics.bind_to_address);
    }

    let storage = Arc::new(storage::DB::new(config::CONFIG.message_expiry_days));

    storage.clear_expired();
}
