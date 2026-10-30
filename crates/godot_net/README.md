# godot_net — multiplayer + WebSocket + UPnP (module 08, COMPLETE)

Port of `modules/multiplayer` (`MultiplayerAPI` + `SceneMultiplayer` semantics),
`modules/websocket`, `modules/webrtc` (channel part), `modules/upnp`.

## What’s inside

- **`multiplayer`**: peer 1 = server (Godot authority), `register_rpc` with
  `Authority/AnyPeer/Disabled` modes, broadcast `rpc` + targeted `rpc_id`,
  per-peer queues drained by `poll` (handlers run, authority enforced),
  property-map `snapshot` replication primitive.
- **`websocket`**: `WebSocketChannel` ordered message channels + `loopback_pair`
  for tests/single-machine play; same framing works over `TcpStream`.
- **`upnp`**: gateway discovery stub (reports 0 headless, mockable), port mapping.

## Depth

- `TcpLink`/`TcpHost`: length-prefixed framing over non-blocking `std::net`
  sockets (loopback-tested, same framing any Godot-side peer can speak).

## Verify

`cargo test -p godot_net` — RPC, authority, WebSocket, UPnP, TCP round-trip.
