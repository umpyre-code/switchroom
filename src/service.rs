use crate::metrics;
use crate::storage;

use futures::future;
use instrumented::instrument;
use std::sync::Arc;
use switchroom_grpc::proto;
use switchroom_grpc::tower_grpc::{Request, Response};

#[derive(Debug, Fail)]
enum RequestError {
    #[fail(display = "resource could not be found")]
    NotFound,
    #[fail(display = "Bad arguments specified for request: {:?}", err)]
    BadArguments { err: String },
    #[fail(display = "Storage error: {:?}", err)]
    StorageError { err: String },
}

impl From<storage::StorageError> for RequestError {
    fn from(err: storage::StorageError) -> RequestError {
        RequestError::StorageError {
            err: err.to_string(),
        }
    }
}

impl From<data_encoding::DecodeError> for RequestError {
    fn from(err: data_encoding::DecodeError) -> RequestError {
        RequestError::BadArguments {
            err: err.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct Switchroom {
    storage: Arc<storage::DB>,
}

impl Switchroom {
    pub fn new(storage: Arc<storage::DB>) -> Self {
        Switchroom { storage }
    }

    #[instrument(INFO)]
    fn handle_send_message(
        &self,
        message: &proto::Message,
    ) -> Result<proto::Message, RequestError> {
        use crate::messages::Timestamped;
        use futures::Future;
        let message = self.storage.insert_message(message.timestamped()).wait()?;
        Ok(message)
    }

    #[instrument(INFO)]
    fn handle_get_messages(
        &self,
        request: &proto::GetMessagesRequest,
    ) -> Result<proto::GetMessagesResponse, RequestError> {
        use crate::bloom_filter::BloomFilter;
        use data_encoding::BASE64_NOPAD;
        use futures::Future;

        let messages = self.storage.get_messages_for(&request.client_id).wait()?;
        if request.sketch.is_empty() {
            // If the sketch is empty, return the full set of messages
            Ok(proto::GetMessagesResponse { messages })
        } else {
            // If a sketch was provided, filter out messages that are present in the bloom filter
            let filter_slice: Vec<u8> = BASE64_NOPAD.decode(request.sketch.as_bytes())?;
            let bf = BloomFilter::from_slice(&filter_slice);
            Ok(proto::GetMessagesResponse {
                messages: messages
                    .iter()
                    .filter(|message| {
                        let hash = BASE64_NOPAD.encode(&message.hash);
                        !bf.test(&hash)
                    })
                    .cloned()
                    .collect(),
            })
        }
    }
}

impl proto::server::Switchroom for Switchroom {
    type SendMessageFuture =
        future::FutureResult<Response<proto::Message>, switchroom_grpc::tower_grpc::Status>;
    fn send_message(&mut self, request: Request<proto::Message>) -> Self::SendMessageFuture {
        use futures::future::IntoFuture;
        use switchroom_grpc::tower_grpc::{Code, Status};
        metrics::SEND_MESSAGE_CALLED.inc();
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
        metrics::GET_MESSAGES_CALLED.inc();
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
}
