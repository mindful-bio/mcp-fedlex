# Container-Image fuer mcp-fedlex (Binaer mcp-reader).
#
# Zwei Stufen. Die erste baut das Release-Binaer im offiziellen Rust-Image, die
# zweite legt es in ein distroless-cc-Image (glibc + libgcc, kein Shell, kein
# Paketmanager). Laeuft als nonroot (UID 65532), passend zum SecurityContext im
# Deployment (runAsUser 65532).
#
# mcp-reader haengt ueber fedlex-store am redis-store-Feature (nur tokio-comp,
# kein TLS), darum genuegt glibc ohne OpenSSL.

# --- Build-Stufe ---------------------------------------------------------------
FROM rust:1-bookworm AS builder

WORKDIR /src

# Erst nur die Manifeste kopieren, damit der Dependency-Layer cachebar bleibt.
COPY Cargo.toml Cargo.lock ./
COPY crates/fedlex-core/Cargo.toml      crates/fedlex-core/Cargo.toml
COPY crates/fedlex-store/Cargo.toml     crates/fedlex-store/Cargo.toml
COPY crates/fedlex-jolux/Cargo.toml     crates/fedlex-jolux/Cargo.toml
COPY crates/fedlex-telemetry/Cargo.toml crates/fedlex-telemetry/Cargo.toml
COPY crates/mcp-reader/Cargo.toml       crates/mcp-reader/Cargo.toml
COPY crates/mcp-ingest/Cargo.toml       crates/mcp-ingest/Cargo.toml

# Jetzt die Quellen.
COPY crates/ crates/

# Nur das Reader-Binaer bauen. Die Features kommen aus dem Crate-Graph (mcp-reader
# zieht fedlex-store mit redis-store).
RUN cargo build --release -p mcp-reader --bin mcp-reader \
    && strip target/release/mcp-reader

# --- Laufzeit-Stufe ------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /src/target/release/mcp-reader /usr/local/bin/mcp-reader

EXPOSE 8080
USER 65532:65532

# Das Deployment setzt args=[mcp-reader]. Ueber den PATH (/usr/local/bin) wird das
# Binaer aufgeloest. CMD spiegelt das fuer den Standalone-Lauf.
CMD ["mcp-reader"]
