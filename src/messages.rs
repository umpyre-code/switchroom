use prost::Message;
use switchroom_grpc::proto;

pub trait Timestamped {
    fn timestamped(&self) -> Self;
}

impl Timestamped for proto::Message {
    fn timestamped(&self) -> Self {
        // copy fields from incoming message into new message, and update timestamp
        proto::Message {
            received_at: Some(get_timestamp()),
            ..self.clone()
        }
    }
}

fn get_timestamp() -> proto::Timestamp {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    proto::Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn hash_test() {
        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: "yoyoyoyo".into(),
            nonce: "nonce".into(),
            sender_public_key: "1".into(),
            recipient_public_key: "2".into(),
            pda: "PDA".into(),
            sent_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            signature: "signature".into(),
        };
    }

    #[test]
    fn test_hash_from() {
        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: "yoyoyoyo".into(),
            nonce: "".into(),
            sender_public_key: "1".into(),
            recipient_public_key: "2".into(),
            pda: "PDA".into(),
            sent_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            signature: "signature".into(),
        };
        let new_message = message.timestamped();
        assert_eq!(new_message.from, "from id");
        assert_eq!(new_message.to, "to id");
        assert_eq!(new_message.body, b"yoyoyoyo");
    }

    #[test]
    fn test_verify() {
        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: "yoyoyoyo".into(),
            nonce: "".into(),
            sender_public_key: "1".into(),
            recipient_public_key: "2".into(),
            pda: "PDA".into(),
            sent_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            signature: "signature".into(),
        };
        let new_message = message.timestamped();
        assert_eq!(new_message.from, "from id");
        assert_eq!(new_message.to, "to id");
        assert_eq!(new_message.body, b"yoyoyoyo");
    }
}
