// Short join codes (`123-456-789`), apart from Cloudflare's runtime so that `node --test` can
// try them (codes.test.mjs). A game hosting a session registers its session code (the long
// `OMSI-...` one) and is given nine digits that lead to it for as long as the game says,
// every minute, that it still hosts; a player types the nine digits and the game asks here
// for the session code behind them. The session code itself never stops working: a game
// that cannot reach this service shows and takes it as before.

export const CODE_TTL_MS = 3 * 60 * 1000; // a code is forgotten this long after its last refresh
export const MAX_CODES = 50000;
export const CODES_PER_HOUR = 30; // registrations one address may make
export const LOOKUPS_PER_MINUTE = 20; // lookups one address may make (nine digits are guessable)

const LEGACY = /^OMSI(-[0-9A-Z]{1,8}){2,16}$/;

// A session code as a game writes it (`OMSI-` and groups of letters and digits), upper case;
// null for anything else.
export function legacyCode(v) {
  if (typeof v !== "string") return null;
  const t = v.trim().toUpperCase();
  return t.length <= 96 && LEGACY.test(t) ? t : null;
}

// The nine digits of a short code, however it was typed (`123-456-789`, `123 456 789`);
// null for anything else. A code never starts with 0.
export function shortDigits(v) {
  if (typeof v !== "string") return null;
  const d = v.replace(/[\s-]/g, "");
  return /^[1-9][0-9]{8}$/.test(d) ? d : null;
}

// A fresh code from random 32-bit numbers (`random32`: the worker's `crypto.getRandomValues`),
// every code as likely as another: a number at or above the largest multiple of 900 000 000
// below 2^32 is drawn again rather than folded over (which would favour the low codes).
const SPAN = 900000000;
const LIMIT = Math.floor(2 ** 32 / SPAN) * SPAN;
export function drawShort(random32) {
  for (;;) {
    const n = random32();
    if (n < LIMIT) return String(100000000 + (n % SPAN));
  }
}

export class Codes {
  constructor() {
    this.codes = new Map();
  }

  sweep(now) {
    for (const [k, e] of this.codes) if (e.until <= now) this.codes.delete(k);
  }

  live(short, now) {
    const e = this.codes.get(short);
    return e && e.until > now ? e : null;
  }

  // A code for `code`: `draw` gives candidates, `tokenHash` is what a refresh or release
  // must match (the hash of the token the worker hands out).
  register(code, tokenHash, now, draw) {
    const c = legacyCode(code);
    if (!c) return { status: 400, body: { error: "not a session code" } };
    if (this.codes.size >= MAX_CODES) this.sweep(now);
    if (this.codes.size >= MAX_CODES) return { status: 503, body: { error: "too many codes: try again later" } };
    for (let i = 0; i < 32; i++) {
      const short = draw();
      if (!this.live(short, now)) {
        this.codes.set(short, { code: c, tokenHash, until: now + CODE_TTL_MS });
        return { status: 201, body: { short, ttl: CODE_TTL_MS / 1000 } };
      }
    }
    return { status: 503, body: { error: "no free code: try again later" } };
  }

  // The host still hosts (its session code may have changed: a new address).
  refresh(short, tokenHash, code, now) {
    const e = this.live(shortDigits(short), now);
    if (!e) return { status: 404, body: { error: "unknown code" } };
    if (e.tokenHash !== tokenHash) return { status: 403, body: { error: "wrong token" } };
    if (code !== undefined) {
      const c = legacyCode(code);
      if (!c) return { status: 400, body: { error: "not a session code" } };
      e.code = c;
    }
    e.until = now + CODE_TTL_MS;
    return { status: 200, body: { ttl: CODE_TTL_MS / 1000 } };
  }

  release(short, tokenHash, now) {
    const d = shortDigits(short);
    const e = this.live(d, now);
    if (e && e.tokenHash === tokenHash) this.codes.delete(d);
    return { status: 204 };
  }

  lookup(short, now) {
    const e = this.live(shortDigits(short), now);
    return e ? { status: 200, body: { code: e.code } } : { status: 404, body: { error: "no session has this code now" } };
  }
}
