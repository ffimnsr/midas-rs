FROM docker.io/library/rust:latest AS builder
WORKDIR /usr/src

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools

RUN cargo new midas-app
WORKDIR /usr/src/midas-app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --verbose --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /usr/src/midas-app/target/x86_64-unknown-linux-musl/release/midas .
COPY --from=builder /usr/src/midas-app/target/x86_64-unknown-linux-musl/release/cargo-migrate .
COPY COPYRIGHT LICENSE-APACHE LICENSE-MIT .
USER 1000
CMD ["./midas"]
