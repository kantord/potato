FROM rust:slim-bookworm AS dispatcher-builder
WORKDIR /build
COPY dispatcher/Cargo.toml ./Cargo.toml
COPY dispatcher/src ./src
RUN cargo build --release

FROM debian:bookworm-slim
LABEL io.github.kantord.spudkit.version="1"
RUN apt-get update && apt-get install -y --no-install-recommends s6 && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/gui /app/bin /app/templates /run/spudkit
COPY --from=dispatcher-builder /build/target/release/dispatcher /usr/local/bin/dispatch
CMD ["s6-ipcserver", "-P", "-c", "200", "/run/spudkit/exec.sock", "/usr/local/bin/dispatch"]
