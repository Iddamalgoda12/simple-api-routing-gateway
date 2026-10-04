# Simple API routing gateway (Rust)

A small HTTP reverse proxy for learning how an API gateway works. Axum accepts
requests, and a shared Reqwest client forwards them to an upstream service.
Routes are selected by the longest matching path prefix.

## 1. Fedora prerequisites

This machine already has Rust and Cargo. On a new Fedora workstation, install
the packaged toolchain and Git with:

```bash
sudo dnf install rust cargo git
```

Check the tools:

```bash
rustc --version
cargo --version
```

## 2. Project structure

```text
Cargo.toml          package metadata and dependencies
Cargo.lock          exact dependency versions selected by Cargo
src/main.rs         startup, middleware, listener, graceful shutdown
src/config.rs       YAML and environment configuration validation
src/proxy.rs        path matching and HTTP forwarding
config/routes.yaml  path prefixes and upstream origins
```

Cargo is Rust's package manager and build tool. Axum provides the inbound HTTP
server and request routing; Tokio runs asynchronous network work; Reqwest is the
outbound HTTP client. Tower HTTP adds request tracing and a body size limit.
Rustls provides TLS for outbound HTTPS without linking Fedora OpenSSL libraries.

## 3. Configure routes

Edit `config/routes.yaml` to map path prefixes to upstream origins:

```yaml
routes:
  /users: http://127.0.0.1:9001
  /orders/v1: http://127.0.0.1:9002
```

This gives `/users` -> `http://127.0.0.1:9001/` and `/orders/v1` ->
`http://127.0.0.1:9002/`. The longest matching prefix is selected and removed before forwarding;
for example `/users/42?full=true` reaches the first upstream as `/42?full=true`.
Use an origin URL only (scheme and host, with optional port), without a base
path or query. Unknown paths return 404.

By default the gateway reads `config/routes.yaml`; set `GATEWAY_CONFIG` to use
another file. Listening address and limits remain configurable with environment
variables. Route YAML is deserialized into a typed Rust struct using Serde and
validated before the server binds, so invalid configuration fails at startup.

Optional settings:

```bash
export GATEWAY_LISTEN_ADDR=127.0.0.1:8080
export GATEWAY_REQUEST_TIMEOUT_SECS=15
export GATEWAY_MAX_BODY_BYTES=10485760
```

## 4. Build and run

From this directory:

```bash
cargo run
```

Cargo downloads dependencies on the first build, then compiles and starts the
gateway. Check the health endpoint in another terminal:

```bash
curl -i http://127.0.0.1:8080/healthz
curl -i http://127.0.0.1:8080/users/42?full=true
```

Set `RUST_LOG=debug` to see request tracing. Press Ctrl+C for graceful shutdown.

## Gateway concepts in this implementation

- **Routing:** the gateway chooses an upstream based on the incoming URL path.
- **Reverse proxying:** clients call one public address; the gateway calls the
  selected backend and returns its status, headers, and streamed body.
- **Shared client:** Reqwest's connection pool is reused across calls instead of
  creating a new TCP/TLS connection for every request.
- **Timeouts and errors:** upstream timeouts become 504 responses; connection
  and other upstream failures become 502 responses.
- **Request limits and observability:** a 10 MiB default limit bounds incoming
  bodies, and Tower HTTP emits structured tracing spans.
- **Graceful shutdown:** Ctrl+C and SIGTERM stop accepting new connections and
  allow active requests to finish.

This is a learning baseline. Production gateways also need deliberate policy
for authentication, rate limiting, trusted forwarded headers, TLS termination,
metrics, retries, load balancing, and deployment. In particular, this starter
does not retry requests because retrying non-idempotent requests can duplicate
effects.
