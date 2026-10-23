# FerrisDB

A high-performance, concurrent Redis clone written in Rust.

> **Status: Work in Progress** — Core networking and RESP2 protocol parsing are up and running. Currently working on the in-memory storage engine (Phase 3).

---

## What is this?

FerrisDB is a personal systems engineering project to build a Redis-compatible in-memory database from the ground up using **Rust (2024 Edition)**.

Instead of building everything in a single file, the project is structured as a modular Cargo Workspace where each layer (protocol, networking, storage, configuration, and telemetry) is isolated and tested independently.

## Current Progress

- [x] **Foundations** — Workspace layout, CLI config (`clap`), and structured logging (`tracing`).
- [x] **RESP2 Codec** — Zero-copy frame parser (`bytes`), SIMD line scanning (`memchr`), and protocol safety limits.
- [x] **TCP Network Engine** — Async task-per-connection loop, connection limits (`Semaphore`), command pipelining, and graceful shutdown.
- [ ] **In-Memory Storage (Next up)** — Sharded key-value store, `GET`/`SET`/`DEL`, TTL expiration, and memory eviction.
- [ ] **Data Structures** — Lists, Hashes, Sets, and atomic flags.
- [ ] **Persistence** — Append-Only File (AOF) and disk recovery.

## Project Structure

```text
.
├── crates/
│   ├── codec/       # RESP2 zero-copy parser and encoder
│   ├── config/      # CLI arguments and server settings
│   ├── network/     # Tokio TCP listener, connection loop & pipelining
│   └── telemetry/   # Logging setup
└── server/          # Main binary entrypoint
```

## Trying it out

Right now the server runs as a high-speed RESP2 protocol engine (answering PING commands) while the storage layer is being built.

```bash
# Run the server (listens on 127.0.0.1:6379 by default)
cargo run --release --bin server

# Run all unit and integration tests
cargo test --workspace
```

## Early Benchmarks (Network + Codec Layer)

Tested locally with redis-benchmark (50 concurrent clients) to validate command pipelining and batched socket I/O:

| Benchmark | Mode | Concurrency | Total Requests | Throughput (QPS) | Latency (p50) | Improvement |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **PING** | Sequential | 50 clients | 100,000 | **87,489 req/s** | 0.279 ms | Baseline |
| **PING** | Pipelined (`-P 16`) | 50 clients | 1,000,000 | **353,356 req/s** | 0.255 ms | **+303.9% (4.04x)** |


```bash
# Sequential benchmark
redis-benchmark -p 6379 -n 100000 -c 50 -q "PING"

# Aggressive pipelined benchmark
redis-benchmark -p 6379 -n 1000000 -c 50 -P 16 -q "PING"
```

## License

MIT