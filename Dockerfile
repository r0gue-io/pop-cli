FROM rust as builder
RUN apt-get update && apt-get -y install cmake
WORKDIR /pop
COPY . /pop
RUN cargo build --release

# Build image
FROM rust:slim
RUN apt-get update && apt install -y openssl ca-certificates protobuf-compiler
COPY --from=builder /pop/target/release/pop /usr/bin/pop
CMD ["/usr/bin/pop"]
