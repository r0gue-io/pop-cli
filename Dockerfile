FROM rust as builder
RUN apt-get update && apt-get -y install cmake \
    && apt-get install -y clang \
    && apt-get install --no-install-recommends --assume-yes protobuf-compiler
WORKDIR /pop
COPY . /pop
RUN rustup show active-toolchain || rustup toolchain install
RUN cargo build --release

# Build image, preinstalling all dependencies for general Polkadot development
FROM rust:slim
COPY --from=builder /pop/target/release/pop /usr/bin/pop
RUN apt-get update && pop install -y && apt-get clean
CMD ["/usr/bin/pop"]
