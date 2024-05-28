FROM rust as builder

WORKDIR /pop
COPY . /pop

RUN apt-get update && \
  apt upgrade -y && \
  apt-get -y install cmake protobuf-compiler libprotobuf-dev libclang-dev && \
  cargo build --release && \
  cargo install --locked --no-default-features --features contract,parachain --path ./crates/pop-cli && \
  . "$HOME/.cargo/env" && \
  pop install -y

CMD tail -f /dev/null

