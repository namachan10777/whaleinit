FROM rust:1-slim-bookworm as rust
RUN cargo install cargo-chef
WORKDIR /work

FROM rust as planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust as builder
COPY --from=planner /work/recipe.json recipe.json
RUN cargo chef cook
COPY . .
RUN cargo build

FROM debian:bookworm-slim

RUN apt-get update && apt-get -y install ssh nginx
RUN mkdir -p /etc/whaleinit/services
RUN mkdir -p /run/sshd
RUN ln -sf /dev/stdout /var/log/nginx/access.log && \
    ln -sf /dev/stderr /var/log/nginx/error.log
COPY --from=builder /work/target/debug/whaleinit /whaleinit
COPY examples/nginx.toml /etc/whaleinit/services/nginx.toml
COPY examples/sshd.toml /etc/whaleinit/services/sshd.toml
COPY examples/test_child.sh /usr/local/bin/test_child.sh
COPY examples/test_parent.sh /usr/local/bin/test_parent.sh
COPY examples/test.toml /etc/whaleinit/services/test.toml

ENTRYPOINT [ "/whaleinit" ]

