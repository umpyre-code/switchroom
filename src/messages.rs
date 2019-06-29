use prost::Message;
use switchroom_grpc::proto;

pub trait Hashable {
    fn hashed(&self) -> Self;
    fn verify(&self) -> Result<(), ()>;
}

impl Hashable for proto::Message {
    fn hashed(&self) -> Self {
        // copy fields from incoming message into new message, and update timestamp
        let mut hash_message = proto::Message {
            received_at: Some(get_timestamp()),
            hash: b"".to_vec(),
            ..self.clone()
        };
        // compute the hash
        let hash = b2b_hash(&hash_message, 16);
        // put hash into message
        hash_message.hash = hash;
        hash_message
    }

    fn verify(&self) -> Result<(), ()> {
        // copy fields from incoming message into new message, and update timestamp
        let hash_message = proto::Message {
            hash: b"".to_vec(),
            ..self.clone()
        };
        // compute the hash
        let hash = b2b_hash(&hash_message, 16);
        if hash == self.hash {
            Ok(())
        } else {
            Err(())
        }
    }
}

fn b2b_hash(message: &proto::Message, digest_size: usize) -> Vec<u8> {
    use sodiumoxide::crypto::generichash;
    let mut hasher = generichash::State::new(digest_size, None).unwrap();
    let mut buf = Vec::new();
    message.encode(&mut buf).unwrap();
    hasher.update(&buf).unwrap();
    let digest = hasher.finalize().unwrap();
    digest.as_ref().to_vec()
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
        };
        let hash = b2b_hash(&message, 16);
        assert_eq!(
            hash,
            vec![148, 44, 62, 197, 93, 210, 223, 152, 205, 88, 92, 237, 234, 34, 17, 6]
        );
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
        };
        let new_message = message.hashed();
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
        };
        let new_message = message.hashed();
        assert_eq!(new_message.from, "from id");
        assert_eq!(new_message.to, "to id");
        assert_eq!(new_message.body, b"yoyoyoyo");
        let verified = new_message.verify();
        assert_eq!(verified.is_ok(), true);

        let not_verified = message.verify();
        assert_eq!(not_verified.is_err(), true);
    }
}
