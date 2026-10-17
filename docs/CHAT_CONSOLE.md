# Chat-only client for a multiplayer server

A lightweight terminal client is available as the omsi-net example chat_only. It uses
the same authenticated-by-session transport and admission checks as the game:
HELLO / WELCOME, INFO, a one-second empty-vehicle heartbeat, and CHAT / SAY.
It does not load OMSI content, start the game window, create a bus, or use a GPU.

This is a **normal participant**, not an invisible or privileged connection. It
appears in the session's player count and normal join/leave notifications, takes
one player slot, and can be refused, kicked or banned as any other player. A
chosen nickname is not proof of affiliation with the openOMSI organization.

## Build and connect

From the repository root:

~~~sh
cargo run -p omsi-net --example chat_only -- 127.0.0.1:27015 --name Isaac

# Legacy protocol 7 (explicit)
cargo run -p omsi-net --example chat_only -- https://your-server.example --name DEV_TEST_001 --protocol 7
~~~

Other supported targets:

~~~sh
# Server reached through its standard WebSocket gateway or Cloudflare tunnel
cargo run -p omsi-net --example chat_only -- https://example.trycloudflare.com --name Isaac

# Normal openOMSI session code, subject to the usual availability/network checks
cargo run -p omsi-net --example chat_only -- OMSI-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX --name Isaac

# Send one line, stay connected briefly, then disconnect
cargo run -p omsi-net --example chat_only -- 127.0.0.1:27015 --name Isaac --message "Hello everyone"
~~~

Replace example addresses and session codes with real ones. The official server
alias is also resolved through the existing signed endpoint.

For a standalone executable:

~~~sh
cargo build --release -p omsi-net --example chat_only
# Windows: target/release/examples/chat_only.exe
# Linux/macOS: target/release/examples/chat_only
~~~

In interactive mode, type a line and press Enter. Use :help for the local help,
or :quit to disconnect cleanly. The console prints incoming chat and notices.
Messages must be 1-160 characters, excluding protocol separators and controls.
The existing one-message-per-second server rate limit still applies.

## Announce without joining: server owners only

A dedicated server already accepts administrative messages **from its own machine**
through its localhost-only POST /admin endpoint (provided its admin_password
is configured). The existing administration command is:

~~~text
say Scheduled restart in five minutes
~~~

This is a better choice for a hosted system announcement: no player slot is
occupied. The endpoint requires X-Admin-Password and is intentionally not
available as a public remote admin API. Do not expose /admin through reverse
proxies or a Cloudflare tunnel, or embed the password in a public client.

## Testing

~~~sh
cargo test -p omsi-net --example chat_only
cargo test -p omsi-net
~~~

Manual integration smoke test:
1. Host a dedicated server and join with the chat console. Verify join/leave
   notices, correct nickname and no spawned vehicle.
2. Exchange messages with a regular game client. Confirm chat reaches both.
3. Repeat over a wss:// gateway. Drop/recover the connection and ensure the
   console can resume using the existing reconnect mechanism.
4. Try a full server, an incompatible protocol version, and a server ban.
   The console must respect each rejection.
5. Check the one-shot --message option, :quit, overlong lines and the chat
   rate limit.

No live multiplayer server or CI result is implied by this document.

## Legacy protocol 7

Use --protocol 7 for servers that reject protocol 14. This separate adapter
is in crates/omsi-net/examples/chat_only/legacy.rs and follows the original
protocol 7 HELLO/WELCOME, INFO, empty-vehicle heartbeat, CHAT and SAY frames.

~~~sh
cargo run -p omsi-net --example chat_only -- https://your-tunnel.trycloudflare.com/ --name DEV_TEST_001 --protocol 7
~~~

This is an **ordinary player connection**. It obeys rejection, capacity,
kicking, bans and the legacy chat rate limit, without supplying any staff
identity or admin privilege. The protocol is never downgraded automatically.
Protocol 7's protections are older than protocol 14's; only use it with
servers you trust.

WebSocket tunnels and direct UDP addresses are supported. Session codes work
when one of their IP addresses is directly reachable; v7 mode does not
implement rendezvous/punch-through. Restart the client after a WebSocket
reconnection (the legacy challenge requires fresh admission). Versions other
than 7 or the current 14 are not implemented.

Test the adapter's layouts and no-vehicle heartbeat:

~~~sh
cargo test -p omsi-net --example chat_only
~~~

A live protocol-7 test server is still needed to verify chat end to end.
