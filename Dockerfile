# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:bookworm AS builder

# System deps for rdkafka (libsasl2) and sqlx (sqlite3)
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev \
    pkg-config \
    libsasl2-dev \
    libsqlite3-dev \
    cmake \
    curl \
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install protoc from the official GitHub release.
# The apt package (protobuf-compiler) doesn't ship the well-known .proto files
# alongside the binary, which breaks prost-build for transitive deps (substrait, etc.).
# The official release bundles include/google/protobuf/ next to the binary.
ARG PROTOC_VERSION=25.3
RUN ARCH=$(uname -m | sed 's/x86_64/x86_64/;s/aarch64/aarch_64/') && \
    curl -Lo /tmp/protoc.zip \
      "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-${ARCH}.zip" && \
    unzip /tmp/protoc.zip -d /usr/local && \
    rm /tmp/protoc.zip && \
    chmod +x /usr/local/bin/protoc

# Tell prost-build / tonic-build where protoc and its bundled includes live.
# Every build.rs in the dep graph (ours + substrait, ballista-core, etc.) picks these up.
ENV PROTOC=/usr/local/bin/protoc
ENV PROTOC_INCLUDE=/usr/local/include

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY proto/      proto/
COPY migrations/ migrations/
COPY crates/     crates/

# Build all service binaries.
# Cache mounts keep the cargo registry and incremental build artifacts between
# docker build runs — only changed crates recompile after the first build.
# Binaries are copied to /out so they survive after the cache mount scope closes.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build \
        -p elan-central \
        -p elan-coordinator \
        -p elan-executor \
        -p elan-query && \
    mkdir /out && \
    cp target/debug/elan-central \
       target/debug/elan-coordinator \
       target/debug/elan-executor \
       target/debug/elan-query \
       /out/

# ── Runtime stage ────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl3 \
    libsasl2-2 \
    libsqlite3-0 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy all service binaries from the builder
COPY --from=builder /out/elan-central     /usr/local/bin/
COPY --from=builder /out/elan-coordinator /usr/local/bin/
COPY --from=builder /out/elan-executor    /usr/local/bin/
COPY --from=builder /out/elan-query       /usr/local/bin/

WORKDIR /app
