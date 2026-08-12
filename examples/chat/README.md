# Chat Experiments

This folder contains a small socket-based chat experiment built on top of `tiny-wasm-runtime`.

Programs:

- `experiment-directory-server`
  A central directory server that stores `user -> ip:port + availability`.
- `experiment-chat-peer`
  A chat peer that can either listen for incoming sessions or dial another user by first querying the directory.
- `experiment-chat-demo`
  A scripted end-to-end scenario that starts the directory server, starts a listening peer, runs two chat initiators, and demonstrates waiting until a busy user becomes available again.

These are `crate-type = ["cdylib"]` `[[example]]` targets exporting an async
`wasi:cli/run` (see the root README), so `cargo run --example` doesn't apply
-- build, then invoke wasmtime directly. Cargo also normalizes hyphens to
underscores in the actual `.wasm` filename (`experiment-chat-demo` ->
`experiment_chat_demo.wasm`).

## Build

```powershell
cargo build --target wasm32-wasip2 --examples
```

## Run The Scripted Demo

Use `wasmtime` directly with the workspace directory preopened so the experiment can write session logs:

```powershell
wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y --dir . target\wasm32-wasip2\debug\examples\experiment_chat_demo.wasm
```

Logs are written to:

- `examples/chat/logs/directory-server.log`
- `examples/chat/logs/listener-bob.log`
- `examples/chat/logs/initiator-*.log`
- `examples/chat/logs/session-*.log`

## Run The Programs Manually

Start the directory server:

```powershell
wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y --dir . target\wasm32-wasip2\debug\examples\experiment_directory_server.wasm
```

Start a listening peer:

```powershell
wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y --dir . target\wasm32-wasip2\debug\examples\experiment_chat_peer.wasm listen bob 64100 "bob here|session complete" 2 1200
```

Start a caller that waits if the target is busy:

```powershell
wasmtime -W component-model-async=y -S cli=y -S inherit-network=y -S tcp=y -S udp=y -S p3=y --dir . target\wasm32-wasip2\debug\examples\experiment_chat_peer.wasm dial alice bob "hello bob|checking in" wait
```

## Notes

- The chat payloads are scripted text messages separated with `|`.
- The directory records include both address information and a simple `AVAILABLE` / `BUSY` state.
- The experiment currently keeps short-lived directory request sockets alive until process exit to avoid a Preview 2 resource teardown edge in the runtime's current socket lifecycle.
