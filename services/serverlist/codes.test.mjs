// node --test services/serverlist
import test from "node:test";
import assert from "node:assert/strict";
import { Codes, CODE_TTL_MS, MAX_CODES, legacyCode, shortDigits, drawShort } from "./codes.js";

const CODE = "OMSI-ABCD-EFGH-2345-6789-KLMN";

test("session codes and short codes are read as games write and players type them", () => {
  assert.equal(legacyCode(" omsi-abcd-efgh-2345 "), "OMSI-ABCD-EFGH-2345");
  for (const bad of ["", "OMSI", "OMSI-", "ABCD-EFGH", "OMSI-AB CD-EF", "OMSI-ABCDEFGHI-JK", "https://x.example.org", 5, null, `OMSI-${"A".repeat(8)}-`.repeat(12)]) {
    assert.equal(legacyCode(bad), null, String(bad));
  }
  assert.equal(shortDigits("123-456-789"), "123456789");
  assert.equal(shortDigits(" 123 456 789 "), "123456789");
  for (const bad of ["012-345-678", "12-345-678", "1234-567-890", "123-456-78a", "", 123456789]) {
    assert.equal(shortDigits(bad), null, String(bad));
  }
  const from = (...n) => () => n.shift();
  assert.equal(drawShort(from(0)), "100000000");
  assert.equal(drawShort(from(899999999)), "999999999");
  assert.equal(drawShort(from(3599999999)), "999999999");
  assert.equal(drawShort(from(3600000000, 4294967295, 5)), "100000005", "the top of the range is drawn again");
});

test("a code leads to its session while the host refreshes it, then is forgotten", () => {
  const c = new Codes();
  const r = c.register(CODE, "h", 0, () => "123456789");
  assert.deepEqual(r, { status: 201, body: { short: "123456789", ttl: CODE_TTL_MS / 1000 } });
  assert.deepEqual(c.lookup("123-456-789", 1000).body, { code: CODE });
  assert.equal(c.refresh("123456789", "h", undefined, CODE_TTL_MS - 1).status, 200);
  assert.equal(c.lookup("123456789", CODE_TTL_MS + 1000).status, 200, "refreshed");
  assert.equal(c.lookup("123456789", 2 * CODE_TTL_MS).status, 404, "not refreshed again");
  assert.equal(c.refresh("123456789", "h", undefined, 2 * CODE_TTL_MS).status, 404);
});

test("only the host's token refreshes, moves or releases its code", () => {
  const c = new Codes();
  c.register(CODE, "h", 0, () => "555000111");
  assert.equal(c.refresh("555000111", "other", undefined, 10).status, 403);
  assert.equal(c.refresh("555000111", "h", "not a code", 10).status, 400);
  assert.equal(c.refresh("555000111", "h", "OMSI-ZZZZ-YYYY-2222", 10).status, 200);
  assert.deepEqual(c.lookup("555000111", 20).body, { code: "OMSI-ZZZZ-YYYY-2222" });
  c.release("555000111", "other", 30);
  assert.equal(c.lookup("555000111", 40).status, 200, "a wrong token releases nothing");
  c.release("555-000-111", "h", 50);
  assert.equal(c.lookup("555000111", 60).status, 404);
});

test("a live code is never given twice; a forgotten one may be", () => {
  const c = new Codes();
  const draws = ["111111111", "111111111", "222222222"];
  c.register(CODE, "a", 0, () => draws.shift());
  assert.equal(c.register(CODE, "b", 1, () => draws.shift()).body.short, "222222222");
  assert.equal(c.register(CODE, "c", 2, () => "111111111").status, 503, "every candidate taken");
  assert.equal(c.register(CODE, "d", CODE_TTL_MS + 5, () => "111111111").status, 201);
  assert.equal(c.register("junk", "e", 0, () => "333333333").status, 400);
});

test("the number of codes is bounded", () => {
  const c = new Codes();
  let n = 100000000;
  for (let i = 0; i < MAX_CODES; i++) c.codes.set(String(n++), { code: CODE, tokenHash: "x", until: 1000 });
  assert.equal(c.register(CODE, "h", 10, () => String(n++)).status, 503);
  assert.equal(c.register(CODE, "h", 2000, () => String(n++)).status, 201, "expired codes make room");
});
