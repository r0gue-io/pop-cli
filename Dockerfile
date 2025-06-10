FROM rust as builder
RUN apt-get update && apt-get -y install cmake \
    && apt-get install -y clang \
    && apt-get install --no-install-recommends --assume-yes protobuf-compiler
WORKDIR /pop
COPY . /pop
RUN rustup show active-toolchain || rustup toolchain install
RUN cargo build --release

# Build image, preinstalling all dependencies for general Polkadot development
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    libgcc-s1 \
    libstdc++6 \
    && apt-get clean

COPY --from=builder /pop/target/release/pop /usr/bin/pop
RUN /usr/bin/pop install -y
CMD ["/usr/bin/pop"]
