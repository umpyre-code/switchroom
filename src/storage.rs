use foundationdb::tuple::{Decode, Encode};
use foundationdb::{self, *};
use futures::Future;
use switchroom_grpc::proto;

#[derive(Debug, Fail)]
enum StorageError {
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

pub struct DB {
    network: foundationdb::network::Network,
    handle: Box<std::thread::JoinHandle<()>>,
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

fn get_blob(
    trx: &Transaction,
    subspace: Subspace,
) -> Box<Future<Item = Vec<u8>, Error = foundationdb::Error>> {
    use foundationdb::transaction::RangeOptionBuilder;
    use futures::future;

    let range = RangeOptionBuilder::from(subspace.range());
    Box::new(trx.get_range(range.build(), 0).and_then(|got_range| {
        let mut buf = Vec::new();

        for key_value in got_range.key_values().as_ref() {
            buf.extend_from_slice(key_value.value());
        }

        future::ok(buf)
    }))
}

impl DB {
    pub fn new() -> Self {
        use futures::future::*;

        let network = foundationdb::init().expect("failed to initialize Fdb client");

        let handle = Box::new(std::thread::spawn(move || {
            let error = network.run();

            if let Err(error) = error {
                panic!("fdb_run_network: {}", error);
            }
        }));

        // wait for the network thread to be started
        network.wait();

        // work with Fdb
        let db = Cluster::new(foundationdb::default_config_path())
            .and_then(|cluster| cluster.create_database())
            .wait()
            .expect("failed to create Cluster");

        DB {
            network,
            handle,
            db,
        }
    }

    pub fn stop(self) -> std::thread::Result<()> {
        // cleanly shutdown the client
        self.network.stop().expect("failed to stop Fdb client");
        self.handle.join()
    }

    pub fn insert_message(
        &self,
        message: proto::Message,
    ) -> Box<dyn Future<Item = (), Error = StorageError>> {
        use chrono::{Datelike, Duration, NaiveDateTime};
        use prost::Message;

        self.db.transact(move |trx| {
            let mut buf = Vec::new();
            message.encode(&mut buf)?;

            let subspace = Subspace::from("M");

            let timestamp = message.received_at.as_ref().unwrap();
            let received_at =
                NaiveDateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32);
            let expiry = (received_at + Duration::days(30)).date();

            let expiry = (expiry.year() as i64) * 10_000
                + (expiry.month() as i64) * 100
                + (expiry.day() as i64);

            // Set blob for `from` client ID
            let subkey1 = (message.to.clone(), message.hash.clone());
            set_blob(
                &trx,
                &subspace.subspace(subkey1.clone()),
                &(buf.clone(), expiry).to_vec(),
            );
            // Set blob for `to` client ID
            let subkey2 = (message.from.clone(), message.hash.clone());
            set_blob(
                &trx,
                &subspace.subspace(subkey2.clone()),
                &(buf, expiry).to_vec(),
            );

            // Set expiry keys
            let exp_subspace = Subspace::from(("E", expiry));
            trx.set(&exp_subspace.pack(subkey1), &().to_vec());
            trx.set(&exp_subspace.pack(subkey2), &().to_vec());
            Ok(())
        })
    }

    // pub fn get_messages_for(
    //     &self,
    //     client_id: &str,
    // ) -> Box<Future<Item = Vec<proto::Message>, Error = StorageError>> {
    // }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;
    use crate::messages::Hashable;

    #[test]
    fn blob_test() {
        let message = proto::Message {
            hash: "".into(),
            from: "from id".into(),
            to: "to id".into(),
            received_at: Some(proto::Timestamp {
                seconds: 1,
                nanos: 2,
            }),
            body: "yoyoyoyo".into(),
        }
        .hashed();

        let db = DB::new();

        let future = db.insert_message(message);

        future.wait();
    }
}
