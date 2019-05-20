use futures::future;
use instrumented::{instrument, prometheus, register};
use switchroom_grpc::proto;
use switchroom_grpc::tower_grpc::{Request, Response};

fn make_intcounter(name: &str, description: &str) -> prometheus::IntCounter {
    let counter = prometheus::IntCounter::new(name, description).unwrap();
    register(Box::new(counter.clone())).unwrap();
    counter
}

lazy_static! {}

#[derive(Debug, Fail)]
enum RequestError {
    #[fail(display = "resource could not be found")]
    NotFound,
    #[fail(display = "Bad arguments specified for request")]
    BadArguments,
}

#[derive(Debug, Clone)]
pub struct Switchroom;

impl Switchroom {
    pub fn new() -> Self {
        Switchroom {}
    }
}

impl proto::server::Switchroom for Switchroom {
    type CheckFuture = future::FutureResult<
        Response<proto::HealthCheckResponse>,
        switchroom_grpc::tower_grpc::Status,
    >;
    fn check(&mut self, _request: Request<proto::HealthCheckRequest>) -> Self::CheckFuture {
        future::ok(Response::new(proto::HealthCheckResponse {
            status: proto::health_check_response::ServingStatus::Serving as i32,
        }))
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use std::sync::Mutex;

    lazy_static! {
        static ref LOCK: Mutex<i32> = Mutex::new(0);
    }

}
