FROM rust:1-slim-bookworm AS rust
RUN cargo install cargo-chef
WORKDIR /work

FROM rust AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust AS builder
COPY --from=planner /work/recipe.json recipe.json
RUN cargo chef cook --release
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN mkdir -p /etc/whaleinit/services
COPY --from=builder /work/target/release/whaleinit /whaleinit

ENTRYPOINT [ "/whaleinit" ]

