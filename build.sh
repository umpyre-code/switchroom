#!/bin/bash
set -e
set -x

echo "Running build for $REPO_NAME"
# GCS w/ sccache currently does not work :/
# export SCCACHE_GCS_BUCKET=umpyre-sccache
# export SCCACHE_GCS_RW_MODE=READ_WRITE
export SCCACHE_GCS_KEY_PATH=/root/sccache.json
export SCCACHE_DIR=/workspace/sccache
mkdir -p $SCCACHE_DIR

mkdir -p $HOME/.ssh
chmod 0700 $HOME/.ssh
ssh-keyscan github.com > $HOME/.ssh/known_hosts

# Don't echo secrets
set +x
echo "$SSH_KEY" > $HOME/.ssh/id_rsa
echo "$SCCACHE_KEY" > $SCCACHE_GCS_KEY_PATH
set -x

chmod 600 $HOME/.ssh/id_rsa
eval `ssh-agent`
ssh-add -k $HOME/.ssh/id_rsa

gcloud auth activate-service-account --key-file=$SCCACHE_GCS_KEY_PATH
gsutil cp gs://umpyre-sccache/$REPO_NAME/cache.tar.gz ./cache.tar.gz || true
gsutil cp gs://umpyre-sccache/$REPO_NAME/cargo.tar.gz ./cargo.tar.gz || true

tar xf cache.tar.gz || true
rm -f cache.tar.gz
tar xf cargo.tar.gz -C $CARGO_HOME || true
rm -f cargo.tar.gz

sccache -s

# Install foundationdb client library
export FDB_VERSION=6.1.8
export FDB_VERSION2=${FDB_VERSION}-1
export FDB_BASE_URL=https://www.foundationdb.org/downloads/${FDB_VERSION}

curl -sL ${FDB_BASE_URL}/ubuntu/installers/foundationdb-clients_${FDB_VERSION2}_amd64.deb > foundationdb-clients.deb
# Add libclang1 for FDB client, and dig for coordinator IP lookup
apt-get update -qq
apt-get install -qqy libclang1 dnsutils
dpkg -i foundationdb-clients.deb
rm foundationdb-clients.deb


yarn install
cargo build --release --out-dir=out -Z unstable-options

sccache -s

tar czf cache.tar.gz sccache target
gsutil -o GSUtil:parallel_composite_upload_threshold=150M cp cache.tar.gz gs://umpyre-sccache/$REPO_NAME/cache.tar.gz || true
rm -f cache.tar.gz
cd $CARGO_HOME
tar czf cargo.tar.gz registry git
gsutil -o GSUtil:parallel_composite_upload_threshold=150M cp cargo.tar.gz gs://umpyre-sccache/$REPO_NAME/cargo.tar.gz || true
rm -f cargo.tar.gz
