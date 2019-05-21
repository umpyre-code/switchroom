use crate::metrics;

use foundationdb::tuple::{Decode, Encode};
use foundationdb::{self, *};
use futures::Future;
use switchroom_grpc::proto;

#[derive(Debug, Fail)]
pub enum StorageError {
    #[fail(display = "unable to encode message: {:?}", err)]
    EncodingFailure { err: String },
    #[fail(display = "Fdb error: {:?}", err)]
    FdbError { err: String },
}

impl From<foundationdb::Error> for StorageError {
    fn from(err: foundationdb::Error) -> StorageError {
        StorageError::FdbError {
            err: err.to_string(),
        }
    }
}

impl From<prost::EncodeError> for StorageError {
    fn from(err: prost::EncodeError) -> StorageError {
        StorageError::EncodingFailure {
            err: err.to_string(),
        }
    }
}

impl From<(foundationdb::transaction::RangeOption, foundationdb::Error)> for StorageError {
    fn from(err: (foundationdb::transaction::RangeOption, foundationdb::Error)) -> StorageError {
        StorageError::FdbError {
            err: err.1.to_string(),
        }
    }
}

pub struct DB {
    db: foundationdb::Database,
}

const CHUNK_SIZE: usize = 10_000;

fn set_blob(trx: &Transaction, subspace: &Subspace, value: &[u8]) {
    let num_chunks = (value.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
    let chunk_size = (value.len() + num_chunks) / num_chunks;

    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = if (i + 1) * chunk_size <= value.len() {
            (i + 1) * chunk_size
        } else {
            value.len()
        };

        trx.set(&subspace.pack(start as i64), &(value[start..end]).to_vec());
    }
}

impl DB {
    pub fn new() -> Self {
        use futures::future::*;

        let network = foundationdb::init().expect("failed to initialize Fdb client");

        std::thread::spawn(move || {
            let error = network.run();

            if let Err(error) = error {
                panic!("fdb_run_network: {}", error);
            }
        });

        // wait for the network thread to be started
        network.wait();

        // work with Fdb
        let db = Cluster::new(foundationdb::default_config_path())
            .and_then(|cluster| cluster.create_database())
            .wait()
            .expect("failed to create Cluster");

        DB { db }
    }

    pub fn insert_message(
        &self,
        message: proto::Message,
    ) -> Box<dyn Future<Item = proto::Message, Error = StorageError>> {
        use chrono::{Datelike, Duration, NaiveDateTime};
        use prost::Message;

        self.db.transact(move |trx| {
            let mut buf = Vec::new();
            message.encode(&mut buf);

            let m_subspace = Subspace::from("M");

            let timestamp = message.received_at.as_ref().unwrap();
            let received_at =
                NaiveDateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32);
            let expiry = (received_at + Duration::days(30)).date();

            let expiry = i64::from(expiry.year()) * 10_000
                + i64::from(expiry.month()) * 100
                + i64::from(expiry.day());

            // Set blob for `from` client ID
            let subkey1 = (message.to.clone(), message.hash.clone());
            set_blob(
                &trx,
                &m_subspace.subspace(subkey1.clone()),
                &(buf.clone(), expiry).to_vec(),
            );
            // Set blob for `to` client ID
            let subkey2 = (message.from.clone(), message.hash.clone());
            set_blob(
                &trx,
                &m_subspace.subspace(subkey2.clone()),
                &(buf, expiry).to_vec(),
            );

            // Set expiry keys
            let exp_subspace = Subspace::from(("E", expiry));
            trx.set(&exp_subspace.pack(subkey1), &().to_vec());
            trx.set(&exp_subspace.pack(subkey2), &().to_vec());

            // Return message
            futures::future::ok(message.clone())
        })
    }

    pub fn get_messages_for(
        &self,
        client_id: &str,
    ) -> Box<dyn Future<Item = Vec<proto::Message>, Error = StorageError>> {
        use foundationdb::keyselector::KeySelector;
        use foundationdb::transaction::RangeOptionBuilder;
        use futures::stream;
        use futures::Stream;
        use prost::Message;

        let mut subspace_start = ("M", client_id).to_vec();
        subspace_start.push(0);
        let mut subspace_end = ("M", client_id).to_vec();
        subspace_end.push(0xff);

        self.db.transact(move |trx| {
            let start = KeySelector::new(subspace_start.clone(), true, 0);
            let end = KeySelector::new(subspace_end.clone(), true, 0);
            let range = RangeOptionBuilder::new(start, end).build();
            // let range = RangeOptionBuilder::from(subspace.range()).build();
            println!("range={:?}", range);

            // bytebuffer for messages
            let mut buf = Vec::new();
            let mut count = 0;

            let mut messages: Vec<proto::Message> = trx
                .get_ranges(range)
                .map(|item| {
                    let kvs = item.key_values();
                    let mut messages: Vec<proto::Message> = vec![];
                    for kv in kvs.as_ref() {
                        count += 1;
                        if let Ok(((_prefix, _client_id, _hash, n), _size)) =
                            <(String, String, Vec<u8>, i64)>::decode_from(kv.key())
                        {
                            println!(
                                "prefix={} client_id='{}' hash={} n={}",
                                _prefix,
                                _client_id,
                                _hash.iter().map(|n| u64::from(*n)).sum::<u64>(),
                                n
                            );
                            if let Ok((mut bytes, _size)) = <(Vec<u8>)>::decode_from(kv.value()) {
                                if !buf.is_empty() && n == 0 {
                                    if let Ok(message) = proto::Message::decode(buf.clone()) {
                                        println!("message decoded successfully");
                                        messages.push(message);
                                    } else {
                                        println!("message decode failure");
                                        metrics::MESSAGE_DECODE_FAILURE.inc();
                                    }
                                    buf.clear();
                                    buf.append(&mut bytes);
                                } else {
                                    buf.append(&mut bytes);
                                }
                            }
                        } else {
                            println!("failed to decode key: {:?}", kv.key());
                        }
                    }
                    stream::iter_ok::<
                        Vec<proto::Message>,
                        (foundationdb::transaction::RangeOption, foundationdb::Error),
                    >(messages)
                })
                .flatten()
                .collect()
                .wait()
                .unwrap();

            // Try decoding any message left in the buffer
            if !buf.is_empty() {
                if let Ok(message) = proto::Message::decode(buf.clone()) {
                    println!("message decoded successfully");
                    messages.push(message);
                } else {
                    println!("message decode failure");
                    metrics::MESSAGE_DECODE_FAILURE.inc();
                }
            }
            println!("count={} messages={}", count, messages.len());

            Ok(messages)
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate rand;
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use crate::messages::Hashable;

    lazy_static! {
        static ref TEST_DB: DB = { DB::new() };
    }

    #[test]
    fn small_blob_test() {
        use self::rand::{thread_rng, Rng};

        // for _ in 0..100 {
        let mut arr = [0u8; 5];
        thread_rng().fill(&mut arr[..]);

        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: arr.to_vec(),
        }
        .hashed();

        let future = TEST_DB.insert_message(message.clone());
        let stored_message = future.wait().unwrap();
        assert_eq!(message, stored_message);

        let future = TEST_DB.get_messages_for("from id");
        let result = future.wait();

        assert_eq!(result.is_ok(), true);
        assert_eq!(
            result
                .unwrap()
                .iter()
                .any(|m| m.hash == stored_message.hash),
            true
        );
        // }
    }

    #[test]
    fn big_blob_test() {
        use self::rand::{thread_rng, Rng};

        // for _ in 0..100 {
        let mut arr = [0u8; 20000];
        thread_rng().fill(&mut arr[..]);

        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: arr.to_vec(),
        }
        .hashed();

        let future = TEST_DB.insert_message(message.clone());
        let stored_message = future.wait().unwrap();
        assert_eq!(message, stored_message);

        let future = TEST_DB.get_messages_for("from id");
        let result = future.wait();

        assert_eq!(result.is_ok(), true);
        assert_eq!(
            result
                .unwrap()
                .iter()
                .any(|m| m.hash == stored_message.hash),
            true
        );
        // }
    }
}
