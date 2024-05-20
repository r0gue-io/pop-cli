FROM rust as builder

WORKDIR /pop
COPY . /pop

RUN apt-get update && \
  apt upgrade -y && \
  apt-get -y install cmake protobuf-compiler libprotobuf-dev libclang-dev && \
  cargo build --release && \
  cp ./target/release/pop /usr/bin

CMD tail -f /dev/null
