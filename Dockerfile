FROM gcr.io/umpyre/github.com/umpyre-code/rust:latest

# Install foundationdb client library
ENV FDB_VERSION=6.1.8
ENV FDB_VERSION2=${FDB_VERSION}-1
ENV FDB_BASE_URL=https://www.foundationdb.org/downloads/${FDB_VERSION}

ADD ${FDB_BASE_URL}/ubuntu/installers/foundationdb-clients_${FDB_VERSION2}_amd64.deb /foundationdb-clients.deb
# Add libclang1 for FDB client, and dig for coordinator IP lookup
RUN apt-get update -qq \
  && apt-get install -qqy libclang1 dnsutils \
  && dpkg -i /foundationdb-clients.deb \
  && rm /foundationdb-clients.deb

ARG SSH_KEY
ARG SCCACHE_KEY

WORKDIR /app

ADD out/* /usr/bin/
ADD entrypoint.sh /app

ENV RUST_LOG=info
ENV RUST_BACKTRACE=full

ENTRYPOINT [ "/app/entrypoint.sh" ]
