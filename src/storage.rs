use crate::metrics;

use foundationdb::tuple::{Decode, Encode, Result};
use foundationdb::{self, *};
use futures::Future;
use switchroom_grpc::proto;

#[derive(Debug, Fail)]
pub enum StorageError {
    #[fail(display = "unable to encode message: {:?}", err)]
    EncodingFailure { err: String },
    #[fail(display = "unable to decode message: {:?}", err)]
    DecodingFailure { err: String },
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

impl From<prost::DecodeError> for StorageError {
    fn from(err: prost::DecodeError) -> StorageError {
        StorageError::DecodingFailure {
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

impl From<foundationdb::tuple::Error> for StorageError {
    fn from(err: foundationdb::tuple::Error) -> StorageError {
        StorageError::FdbError {
            err: err.to_string(),
        }
    }
}

pub struct DB {
    db: foundationdb::Database,
    expiry_days: i64,
}

const CHUNK_SIZE: usize = 10_000;

type BlobKey = (String, String, Vec<u8>, i64);
type ExpKey = (String, i64, Vec<u8>, Vec<u8>);

fn set_blob(trx: &Transaction, subspace: &Subspace, value: &[u8], expiry: i64) {
    use prost::Message;

    let num_chunks = (value.len() + CHUNK_SIZE - 1) / CHUNK_SIZE;
    let chunk_size = (value.len() + num_chunks) / num_chunks;

    for i in 0..num_chunks {
        let start = i * chunk_size;
        let end = if (i + 1) * chunk_size <= value.len() {
            (i + 1) * chunk_size
        } else {
            value.len()
        };

        let blob_value = proto::BlobValue {
            blob_length: value.len() as i64,
            blob_chunk: i as i64,
            expiry,
            payload: value[start..end].into(),
        };
        let mut blob_value_buf = Vec::new();
        blob_value
            .encode(&mut blob_value_buf)
            .expect("Failed to encode message");

        trx.set(&subspace.pack(start as i64), &blob_value_buf);
    }
}

fn to_integer_date(expiry: chrono::Date<chrono::Utc>) -> i64 {
    use chrono::Datelike;
    i64::from(expiry.year()) * 10_000 + i64::from(expiry.month()) * 100 + i64::from(expiry.day())
}

impl DB {
    pub fn new(expiry_days: i64) -> Self {
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

        DB { db, expiry_days }
    }

    pub fn insert_message(
        &self,
        message: proto::Message,
    ) -> Box<dyn Future<Item = proto::Message, Error = StorageError>> {
        use chrono::prelude::*;
        use prost::Message;
        let expiry_days = self.expiry_days;

        self.db.transact(move |trx| {
            let mut buf = Vec::new();
            message.encode(&mut buf).expect("Failed to encode message");

            let m_subspace = Subspace::from("M");

            let timestamp = message
                .received_at
                .as_ref()
                .expect("Couldn't get timestamp");
            let received_at = DateTime::<Utc>::from_utc(
                NaiveDateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32),
                Utc,
            );

            let received = received_at.date();
            let received = to_integer_date(received);

            // Set blob for `from` client ID
            let subkey1 = (message.to.clone(), message.hash.clone());
            set_blob(&trx, &m_subspace.subspace(subkey1.clone()), &buf, received);
            // Set blob for `to` client ID
            let subkey2 = (message.from.clone(), message.hash.clone());
            set_blob(&trx, &m_subspace.subspace(subkey2.clone()), &buf, received);

            // Set expiry keys
            let exp_subspace = Subspace::from(("R", received));
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
        use foundationdb::transaction::RangeOptionBuilder;
        use futures::{stream, Stream};
        use prost::Message;

        let range = RangeOptionBuilder::from(("M", client_id)).build();

        self.db.transact(move |trx| {
            let range = range.clone();

            // bytebuffer for message bytes
            let mut buf = Vec::new();

            let messages: Vec<proto::Message> = trx
                .get_ranges(range)
                .map_err(StorageError::from)
                .map(|item| {
                    let kvs = item.key_values();
                    let mut messages: Vec<proto::Message> = vec![];
                    for kv in kvs.as_ref() {
                        let result: Result<BlobKey> = Decode::try_from(kv.key());
                        match result {
                            Ok((_prefix, _client_id, _hash, n)) => {
                                match proto::BlobValue::decode(kv.value()) {
                                    Ok(mut blob_value) => {
                                        buf.append(&mut blob_value.payload);
                                        if buf.len() == blob_value.blob_length as usize {
                                            match proto::Message::decode(&buf) {
                                                Ok(message) => {
                                                    messages.push(message);
                                                }
                                                Err(err) => {
                                                    error!("failed to decode message: {:?}", err);
                                                    metrics::MESSAGE_DECODE_FAILURE.inc();
                                                }
                                            }
                                            buf.clear();
                                        }
                                    }
                                    Err(err) => error!("failed to decode blob value: {:?}", err),
                                }
                            }
                            Err(err) => error!("failed to decode blob key: {:?}", err),
                        }
                    }
                    stream::iter_ok::<Vec<proto::Message>, StorageError>(messages)
                })
                .flatten()
                .collect()
                .wait()?;

            Ok(messages)
        })
    }

    pub fn clear_expired(&self) -> Box<dyn Future<Item = (), Error = StorageError>> {
        use chrono::prelude::*;
        use foundationdb::keyselector::KeySelector;
        use foundationdb::transaction::RangeOptionBuilder;
        use futures::future;
        use futures::{stream, Stream};

        let expiry_date = (Utc::now() - chrono::Duration::days(self.expiry_days)).date();
        let expiry = to_integer_date(expiry_date);

        let start = KeySelector::new(("R", 0).to_vec(), true, 0);
        let end = KeySelector::new(("R", expiry).to_vec(), true, 0);

        let range = RangeOptionBuilder::new(start, end).build();

        self.db.transact(move |trx| {
            let range = range.clone();
            trx.get_ranges(range)
                .map_err(StorageError::from)
                .map(|item| {
                    let kvs = item.key_values();
                    stream::iter_ok::<Vec<(Vec<u8>, Vec<u8>)>, StorageError>(
                        kvs.as_ref()
                            .iter()
                            .filter_map(|kv| {
                                let result: Result<ExpKey> = Decode::try_from(kv.key());
                                match result {
                                    Ok((_prefix, _expiry, client_id, hash)) => {
                                        println!(
                                            "prefix={} expiry={} client_id={:?} hash={:?}",
                                            _prefix, _expiry, client_id, hash
                                        );
                                        Some((client_id, hash))
                                    }
                                    Err(err) => {
                                        error!("error decoding key: {:?}", err);
                                        None
                                    }
                                }
                            })
                            .collect(),
                    )
                })
                .map(|item| {
                    println!("{:?}", item);
                })
                .collect()
                .wait()?;

            Ok(())
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
        static ref TEST_DB: DB = { DB::new(1) };
    }

    #[test]
    fn small_blob_test() {
        use self::rand::{thread_rng, Rng};

        for _ in 0..10 {
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
        }
    }

    #[test]
    fn big_blob_test() {
        use self::rand::{thread_rng, Rng};

        for _ in 0..3 {
            let mut arr = [0u8; 40000];
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
        }
    }

    #[test]
    fn expired_keys_test() {
        use self::rand::{thread_rng, Rng};

        for _ in 0..10 {
            let mut arr = [0u8; 5];
            thread_rng().fill(&mut arr[..]);

            let message = proto::Message {
                hash: "".into(),
                from: "be expired".into(),
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
        }

        TEST_DB.clear_expired().wait().unwrap();

        let future = TEST_DB.get_messages_for("be expired");
        let result = future.wait();

        assert_eq!(result.is_ok(), true);
        assert_eq!(result.unwrap().len(), 0);
    }
}
