use instrumented::{prometheus, register};

pub fn make_intcounter(name: &str, description: &str) -> prometheus::IntCounter {
    let counter = prometheus::IntCounter::new(name, description).unwrap();
    register(Box::new(counter.clone())).unwrap();
    counter
}

lazy_static! {
    pub static ref SEND_MESSAGE_CALLED: prometheus::IntCounter =
        make_intcounter("send_message_called_total", "Send message endpoint called");
    pub static ref GET_MESSAGES_CALLED: prometheus::IntCounter =
        make_intcounter("get_messages_called_total", "Get messages endpoint called");
    pub static ref MESSAGE_DECODE_FAILURE: prometheus::IntCounter =
        make_intcounter("message_decode_failure_total", "Message decoding failure");
}
