use futures::future;
use instrumented::{instrument, prometheus, register};
use switchroom_grpc::proto;
use switchroom_grpc::tower_grpc::{Request, Response};

fn make_intcounter(name: &str, description: &str) -> prometheus::IntCounter {
    let counter = prometheus::IntCounter::new(name, description).unwrap();
    register(Box::new(counter.clone())).unwrap();
    counter
}

lazy_static! {
    static ref SEND_MESSAGE_CALLED: prometheus::IntCounter =
        make_intcounter("send_message_called", "Send message endpoint called");
    static ref GET_MESSAGES_CALLED: prometheus::IntCounter =
        make_intcounter("get_messages_called", "Get messages endpoint called");
}

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

    #[instrument(INFO)]
    fn handle_send_message(
        &self,
        message: &proto::Message,
    ) -> Result<proto::Message, RequestError> {
        Ok(message.clone())
    }

    #[instrument(INFO)]
    fn handle_get_messages(
        &self,
        _request: &proto::GetMessagesRequest,
    ) -> Result<proto::GetMessagesResponse, RequestError> {
        Ok(proto::GetMessagesResponse { messages: vec![] })
    }
}

impl proto::server::Switchroom for Switchroom {
    type SendMessageFuture =
        future::FutureResult<Response<proto::Message>, switchroom_grpc::tower_grpc::Status>;
    fn send_message(&mut self, request: Request<proto::Message>) -> Self::SendMessageFuture {
        use futures::future::IntoFuture;
        use switchroom_grpc::tower_grpc::{Code, Status};
        SEND_MESSAGE_CALLED.inc();
        self.handle_send_message(request.get_ref())
            .map(Response::new)
            .map_err(|err| Status::new(Code::InvalidArgument, err.to_string()))
            .into_future()
    }

    type GetMessagesFuture = future::FutureResult<
        Response<proto::GetMessagesResponse>,
        switchroom_grpc::tower_grpc::Status,
    >;
    fn get_messages(
        &mut self,
        request: Request<proto::GetMessagesRequest>,
    ) -> Self::GetMessagesFuture {
        use futures::future::IntoFuture;
        use switchroom_grpc::tower_grpc::{Code, Status};
        GET_MESSAGES_CALLED.inc();
        self.handle_get_messages(request.get_ref())
            .map(Response::new)
            .map_err(|err| Status::new(Code::InvalidArgument, err.to_string()))
            .into_future()
    }

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
