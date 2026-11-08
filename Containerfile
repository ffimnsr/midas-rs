FROM docker.io/library/rust:latest AS builder
WORKDIR /usr/src/midas

RUN rustup target add x86_64-unknown-linux-musl
RUN apt update && apt install -y musl-tools

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --verbose --release --target x86_64-unknown-linux-musl -p midas-cli --bins

FROM scratch
COPY --from=builder /usr/src/midas/target/x86_64-unknown-linux-musl/release/midas .
COPY --from=builder /usr/src/midas/target/x86_64-unknown-linux-musl/release/cargo-migrate .
COPY COPYRIGHT LICENSE-APACHE LICENSE-MIT .
USER 1000
CMD ["./midas"]
