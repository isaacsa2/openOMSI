# Public server list

A Cloudflare Worker with one Durable Object (the same shape as `../presence`) that lists the
openOMSI servers whose owners set `public_server = 1` in `server.cfg`. The launcher
(Multiplayer -> Servers -> Public servers) and the game ask it for the list.

```
POST /v1/register   {name, motd, map, max_players, version, protocol, web_port, address?, icon?}
                    -> {id, token, challenge, ttl, interval}
POST /v1/heartbeat  {id, token, players, map}      -> {verified, error?}     (every minute)
POST /v1/unlist     {id, token}                    -> 204
GET  /v1/servers    -> {servers: [{id, address, name, motd, map, players, max_players,
                                   version, protocol, has_icon, age}]}      (cached 15 s)
GET  /v1/servers/<id>/icon.png
```

## What keeps it honest

* **Reachable, and the server's own.** A registration gets a random challenge. The server
  puts it on its own `GET /status` (`list_challenge`); the first heartbeat makes the service
  fetch `<address>/status` and look for it, with the name. Only then is the server listed, and
  the page is read again every 30 minutes. The address is the server's tunnel
  (`https://….trycloudflare.com`, a plain https host name) or **the address the request came
  from** plus the `web_port` it names - nobody can list an address that is not theirs, and
  the service fetches nothing anybody else chose except the https host name of a tunnel (no
  redirects, 5 s, 64 KB). Addresses of private networks are refused.
* **Alive.** A listing is forgotten 3 minutes after its last heartbeat; heartbeats faster than
  every 10 s are refused. Updating needs the token the registration returned (kept as a
  SHA-256 only).
* **Abuse.** At most 8 registrations an hour and 3 listings per source address, 2000 in all,
  bodies of 32 KB, an icon of 16 KB and 64x64 at most, names of 48 characters and messages of
  120 without hidden characters, addresses or invitations; `BLOCKED_TERMS` (a Worker variable,
  comma separated) adds the operator's own words.
* **Privacy.** Nothing about players is sent or kept: only the server's name, message, map,
  player count, version and address. The sender's address is kept as a hash for the rate limit
  and in the listing only as the server's address when it has no tunnel. Everything is in
  memory; nothing is written to storage.
* **The service is not the only check.** The server's name, message and icon pass the game's
  moderation (`crates/omsi-serverlist`) before it registers, and every launcher and game checks
  each listing again before showing it (a flagged text is covered, an icon is shown only if the
  client cleared it itself). A custom icon is therefore only uploaded when the server's image
  check cleared it, and shown only when the viewer's check does.

## Deploy

As `../presence`: `npx wrangler deploy` in this folder (Workers Builds on the repository, root
directory `services/serverlist`). The address it prints is `DEFAULT_SERVICE` in
`crates/omsi-serverlist/src/lib.rs`; a server can name another with `public_list_url`.
`node --test logic.test.mjs` tries the rules.

Open points for the first deploy: whether the Worker may fetch `http://<ip>:<web_port>/status`
(a port that is not 80/443) on the plan in use - if not, only tunnelled servers (https, 443)
get verified, which is what most hosts have anyway; and the free plan's 100 000 requests a
day (a server's heartbeat is one request a minute, so about 60 servers around the clock leave
room for the launchers' reads, which the 15 s edge cache keeps small).

## Short join codes

The same Worker gives a game that hosts a session nine digits (`123-456-789`) for its session code (`OMSI-...`), so players can tell each other a short code (`codes.js`, tried by `node --test services/serverlist/codes.test.mjs`).

- `POST /v1/codes {code}` → `{short, token, ttl}`: a fresh code that no live session holds.
- `POST /v1/codes/refresh {short, token, code?}`: the host still hosts. A game refreshes every minute and sends its current session code, which changes with its address.
- `POST /v1/codes/release {short, token}`: the session is over.
- `GET /v1/codes/<nine digits>` → `{code}`, or 404 when no session holds the code.

A code is forgotten three minutes after its last refresh. Only the token that registered a code can refresh or release it.

Lookups are limited to 20 a minute per address, because nine digits can be guessed. Registrations are limited to 30 an hour per address.

Codes live in the Durable Object's memory, like the listings. After a restart, a host's next refresh gets a 404 and the host registers a new code.

The session code itself always works. A game that cannot reach this service shows and takes the session code as before. `OMSI_CODE_SERVICE` names another service, or `off` to send nothing.
