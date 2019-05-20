extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate tokio;
extern crate tower_hyper;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;
extern crate data_encoding;
extern crate foundationdb;
extern crate instrumented;
extern crate prost;
extern crate sodiumoxide;
extern crate switchroom_grpc;
extern crate toml;
extern crate url;
extern crate yansi;

mod config;
mod messages;
mod service;
mod storage;

use futures::{Future, Stream};
use switchroom_grpc::proto::server;
use tokio::net::TcpListener;
use tower_hyper::server::{Http, Server};

pub fn main() {
    use std::env;

    ::env_logger::init();

    config::load_config();

    // Allow disablement of metrics reporting for testing
    if env::var_os("DISABLE_INSTRUMENTED").is_none() {
        instrumented::init(&config::CONFIG.metrics.bind_to_address);
    }

    let new_service = server::SwitchroomServer::new(service::Switchroom::new());

    let mut server = Server::new(new_service);

    let http = Http::new().http2_only(true).clone();

    let addr = config::CONFIG.service.bind_to_address.parse().unwrap();
    let bind = TcpListener::bind(&addr).expect("bind");

    let serve = bind
        .incoming()
        .for_each(move |sock| {
            let addr = sock.peer_addr().ok();
            info!("New connection from addr={:?}", addr);

            let serve = server.serve_with(sock, http.clone());
            tokio::spawn(serve.map_err(|e| error!("hyper error: {:?}", e)));

            Ok(())
        })
        .map_err(|e| error!("accept error: {}", e));

    let mut rt = tokio::runtime::Builder::new()
        .core_threads(config::CONFIG.service.worker_threads)
        .build()
        .expect("Unable to build tokio runtime");

    rt.spawn(serve);
    info!(
        "Started server with {} threads, listening on {}",
        config::CONFIG.service.worker_threads,
        addr
    );
    rt.shutdown_on_idle().wait().expect("Error in main loop");
}
