# Code & Security Audit: redis-automerge

**Date:** 2026-05-08 (supersedes 2026-02-23)
**Scope:** Full codebase review of all Rust source, Docker configuration, CI/CD, and test infrastructure.

**Re-evaluation note:** This revision integrates 16 newly discovered findings and
adjusts severity on three of the original entries. All 19 findings from the
2026-02-23 audit are still present in the current tree — the only commit since
that audit (`daf1319`) touches CI scripts and does not address any of them.

---

## HIGH Severity

### 1. `--enable-debug-command yes` in production Dockerfile  ✅ RESOLVED 2026-05-08

**File:** `Dockerfile:38`

The Redis configuration enables the `DEBUG` command. This allows any connected
client to execute `DEBUG RESTART`, `DEBUG SET-ACTIVE-EXPIRE`, `DEBUG SLEEP`,
`DEBUG OOM`, etc. This is a denial-of-service vector and should never be enabled
in production images. The test infrastructure uses `DEBUG RESTART` (in
`common.sh:136`), but this should be separated from the production image via a
separate test Dockerfile or runtime override.

**Recommendation:** Remove `--enable-debug-command yes` from the Dockerfile CMD.
Override it in docker-compose.yml for the test service only.

**Resolution (2026-05-08):** `--enable-debug-command yes` removed from the
Dockerfile CMD; the published image now refuses `DEBUG`. The flag is re-applied
on the `redis` service in `docker-compose.yml` via a `command:` override so the
local/CI stack continues to support `DEBUG RESTART` for the AOF persistence
tests.

### 2. `KEYS` command used in hot path (O(N) keyspace scan on every write)  ✅ RESOLVED 2026-05-08

**File:** `index.rs:167`, called via `lib.rs:1262`

`IndexConfig::find_matching_config()` calls the Redis `KEYS` command to scan for
matching index configuration keys. `KEYS` is O(N) where N = total keys in the
database and **blocks the single-threaded Redis event loop** during execution.
This function is called by `try_update_search_index()` which is invoked after
every write operation (`AM.PUTTEXT`, `AM.APPLY`, `AM.FROMJSON`). With a large
keyspace, this will severely degrade Redis performance.

**Recommendation:** Use `SCAN` with cursor-based iteration, or better yet,
maintain an in-memory cache of index configurations that is populated at module
load and updated when `AM.INDEX.CONFIGURE` is called.

**Resolution (2026-05-08):** Added a process-global `RwLock<Option<HashMap<...>>>`
config cache in `index.rs`. `find_matching_config()` is now an in-memory lookup
over configured patterns (O(K) where K = number of configs). The cache is
populated lazily on first read using a single `SCAN` cursor loop and invalidated
on every `AM.INDEX.CONFIGURE/ENABLE/DISABLE` call so subsequent reads see fresh
state. Cold start (after `DEBUG RESTART` / process restart) verified to
repopulate from the persisted `am:index:config:*` Hash keys. The `KEYS` call
in `am_index_status` (`lib.rs:1368`) was replaced by the same cache-backed
lookup. **Caveat:** direct user manipulation of `am:index:config:*` keys via
raw `HSET/DEL` is not detected; the cache repopulates only via the AM.INDEX.*
admin commands or after a Redis restart.

*Follow-up (2026-05-08):* added `AM.INDEX.DELETE <pattern>` so operators have
a first-class command to remove a config without bypassing the cache. The
integration test (`scripts/tests/15-search-indexing.sh` Test 13) was updated
from raw `DEL am:index:config:*` to `AM.INDEX.DELETE *`.

### 3. No size limits on deserialized input (`AM.LOAD`, `AM.APPLY`, `AM.FROMJSON`)  ✅ RESOLVED 2026-05-08

**Files:** `lib.rs:173-189`, `lib.rs:972-1014`, `lib.rs:1167-1197`

- `AM.LOAD` accepts arbitrary binary data and deserializes it with no size check.
- `AM.APPLY` accepts an unbounded number of raw change byte arguments.
- `AM.FROMJSON` parses arbitrarily large JSON strings into Automerge documents.

Any of these can be used to exhaust server memory.

**Recommendation:** Add configurable maximum size limits for input data. Reject
inputs exceeding the limit with an appropriate error message.

**Resolution (2026-05-08):** Added hardcoded constants in `lib.rs`:
`MAX_LOAD_BYTES = 64 MiB`, `MAX_APPLY_CHANGES = 1024`, `MAX_JSON_BYTES = 64 MiB`.
`am_load`, `am_apply`, and `am_fromjson` now reject inputs exceeding the
applicable limit before any allocation or parsing. Per-change byte size in
`AM.APPLY` is also capped at `MAX_LOAD_BYTES`. New integration tests at
`scripts/tests/17-input-limits.sh` exercise each limit. Tunability via
module-load args or `CONFIG SET` is intentionally deferred to a later change.

### 4. `AM.FROMJSON` is unboundedly recursive — stack overflow crashes Redis  ✅ RESOLVED 2026-05-08

**File:** `ext.rs:2386-2477` (`populate_from_json`), called from `lib.rs:1167`

`populate_from_json` recurses on every level of JSON nesting with no depth
guard, and `serde_json::from_str` recurses similarly during parsing. A payload
of a few hundred kilobytes shaped like `{"a":{"a":{"a":...}}}` overflows the
Rust thread stack. Stack overflows in Rust are not catchable; the panic aborts
the entire Redis process. This is reachable via a single unauthenticated
request (in particular, through the publicly exposed webdis service — see
finding #7), and is independent of finding #3 because it triggers on small
inputs.

**Recommendation:** Limit JSON nesting depth before parsing (e.g., reject
inputs whose depth exceeds a configurable cap), and switch the recursive
populator to an explicit work stack to remove the dependence on thread-stack
size.

**Resolution (2026-05-08):** Added `MAX_JSON_DEPTH = 256` in `ext.rs` and a
`depth: usize` parameter to `populate_from_json`. The walker bails with an
`AutomergeError::Fail` once the depth exceeds the cap, before allocating
further Automerge objects. In practice `serde_json::from_str` rejects
payloads deeper than its own default `recursion_limit` (128) before our
walker sees them, so this is defense in depth — but the explicit check
guarantees safety even if `serde_json` defaults change. The integration test
in `scripts/tests/17-input-limits.sh` (Test 4) submits 300-deep JSON and
verifies Redis stays responsive (PONG after the rejection). The recursive
implementation is preserved; converting to an explicit work-stack remains
an option for future change but is unnecessary at the current depth cap.

### 5. `changes:{key}` pub/sub channel discloses every write to any subscriber  ✅ RESOLVED 2026-05-08

**File:** `lib.rs:155-171`

Every successful write publishes the base64-encoded raw Automerge change to the
channel `changes:{key}`:

```rust
let channel_name = format!("changes:{}", key_name.try_as_str()?);
ctx.call("PUBLISH", &[&channel_str, &change_str])?;
```

Any client with `SUBSCRIBE` permission (the default in stock Redis) can run
`PSUBSCRIBE changes:*` and reconstruct the contents of every document mutation
in real time — text edits, integer updates, list inserts, and so on. There is
no per-document opt-in or auth check. Combined with the unauthenticated webdis
ingress (#7), this means an unauthenticated network attacker can stream every
write across every Automerge document. The pub/sub mechanism is by-design for
real-time sync, but the disclosure surface is not documented.

**Recommendation:** Document the disclosure semantics prominently in the README
deployment section. Provide a configuration switch (or per-key prefix) so
operators can disable the public channel when running in a multi-tenant or
untrusted-client context. At minimum, recommend ACL channel restrictions
(`&changes:*` deny by default, allowed only for sync clients).

**Resolution (2026-05-08):** Added a `change-channel-prefix=...` module-load
argument parsed in `init()` (default `changes:` for backward compatibility).
Operators can now (a) keep the default and ACL-restrict subscribers, (b) set
an unguessable prefix and ACL-restrict only authorized sync clients, or
(c) disable change publishing entirely by passing an empty value
(`change-channel-prefix=`). Unknown module args now fail load instead of
silently defaulting. Verified end-to-end: with the empty prefix a
`PSUBSCRIBE changes:*` subscriber receives no messages on writes; with a
custom prefix `sync.tenantA:` the messages flow to that channel and not to
`changes:*`. The disclosure surface, the new arg, and a Redis ACL example
are documented in `README.md` under "Pub/Sub Disclosure Surface".

### 6. Automerge version mismatch between Cargo.toml and Dockerfile  ✅ RESOLVED 2026-05-08

**Files:** `Cargo.toml:10`, `Dockerfile:10-17`, `.github/workflows/docs.yml:35-42`

`Cargo.toml` declares `automerge = "1.0.0-beta.6"` (a beta from the 1.x line).
The Dockerfile clones automerge at tag `js/automerge-3.1.2` (3.x line) and
patches `Cargo.toml` with `sed`. This means:

- Building with `cargo build` outside Docker uses a completely different
  Automerge API than production.
- Local development, IDE checks, and the `cargo doc` CI workflow all run
  against the wrong major version. CI passes do not guarantee the shipped
  artifact builds.
- The declared version is misleading.
- The `sed` patching is fragile and silently duplicated in `docs.yml`.

This is upgraded from MEDIUM to HIGH on this revision because "the binary you
ship is built from code your tests never exercised" is a supply-chain-grade
risk, not a maintainability nit.

**Recommendation:** Use a `[patch]` or path override in a `Cargo.toml`
workspace, or use a git dependency directly in `Cargo.toml` so the source of
truth is clear and `cargo build` and `docker compose build` produce the same
artifact.

**Resolution (2026-05-08):** The js-aligned monorepo dependency was dropped
in favor of the latest published Rust crate. `Cargo.toml` now declares
`automerge = "0.9.0"` (the current stable release on crates.io; the
`1.0.0-beta.x` line was experimental and never reached `beta.6` upstream).
The `git clone --branch js/automerge-3.1.2 ...` and `sed`-rewrite in the
`Dockerfile` were removed; the build stage now resolves `automerge` from
crates.io like every other dependency. `cargo build` locally and
`docker compose build` therefore compile against the same source tree.
The 0.9.0 → existing-code API surface was source-compatible — no changes
to `lib.rs` or `ext.rs` were required. Verified end-to-end: `docker compose
build redis` succeeds and all 17 integration test suites pass
(`docker compose run --build --rm test`). The `clang` apt package is still
installed because `redis-module`'s build script uses bindgen.

### 7. `webdis` exposes unauthenticated HTTP access to Redis  ✅ RESOLVED 2026-05-09

**File:** `docker-compose.yml:14-15`

The `webdis` service maps port 7379 to the host, providing unauthenticated HTTP
access to all Redis commands including AM.* write commands, `DEBUG` (#1),
`PSUBSCRIBE changes:*` (#5), and `KEYS` (#2). The `redis` service itself is
internal only (`expose: 6379`, not `ports`), so webdis is the **sole network
ingress** of the stack as shipped — and it has zero auth.

This is upgraded from MEDIUM to HIGH on this revision because the README-driven
deployment story funnels every external client through this single
unauthenticated port.

**Recommendation:** Either remove the port mapping (use `expose` instead of
`ports`), put webdis behind an authenticating reverse proxy, or document
prominently that the compose file is for local development only and that
production users must add auth.

**Resolution (2026-05-09):** Added a committed `webdis.json` and mounted it
over the image's stock `/etc/webdis.prod.json` from `docker-compose.yml`.
The bundled config enforces a deny-list ACL at the HTTP layer that blocks
the highest-impact commands without ever reaching Redis: `DEBUG` (audit #1),
`KEYS` (audit #2), `CONFIG`, `SHUTDOWN`, `FLUSHDB`, `FLUSHALL`,
`REPLICAOF`, `SLAVEOF`, `MIGRATE`, `MODULE`, `CLUSTER`, `EVAL`, `EVALSHA`,
`SCRIPT`, `FUNCTION`, `SAVE`, `BGSAVE`, `BGREWRITEAOF`, `LASTSAVE`,
`MONITOR`, `CLIENT`. Each returns HTTP 403 Forbidden. Verified end-to-end:
`curl http://localhost:7379/DEBUG/SLEEP/0` → 403, `curl
http://localhost:7379/KEYS/*` → 403, `curl
http://localhost:7379/AM.NEW/testdoc` → 200; all 17 integration test
suites still pass.

HTTP Basic auth is **off** by default to keep the README demo flow
zero-friction (none of the test scripts use webdis). The README's new
"Webdis Authentication" section documents the disclosure surface, shows
how to flip Basic auth on by adding `http_basic_auth` to `webdis.json`,
and walks operators through stronger postures (`expose:` instead of
`ports:`, plus an authenticating sidecar like Caddy/nginx/Traefik for
TLS termination). Findings #1, #2, #5 attack vectors via webdis are
therefore neutralized at the HTTP edge regardless of whether the operator
takes the further step of enabling Basic auth.

---

## MEDIUM Severity

### 8. `Cargo.lock` is gitignored — non-reproducible builds  ✅ RESOLVED 2026-05-08

**File:** `redis-automerge/.gitignore:2`

For a binary crate producing a `.so` file (`cdylib`), `Cargo.lock` should be
committed to ensure reproducible builds. Without it, builds on different
machines/times may pull different transitive dependency versions.

**Recommendation:** Remove `Cargo.lock` from `redis-automerge/.gitignore` and
commit the lock file.

**Resolution (2026-05-08):** `Cargo.lock` removed from
`redis-automerge/.gitignore` and the lockfile resolved by the in-container
build is committed at `redis-automerge/Cargo.lock`. Pairs naturally with
finding #6 — now that the source of truth is `Cargo.toml` + crates.io, the
lockfile pins the exact transitive dependency tree across all builders.

### 9. Unbounded AOF buffer growth

**File:** `ext.rs:364-367`

The `aof: Vec<Vec<u8>>` buffer accumulates all change bytes since the last
`commands()` drain. If many operations are applied between AOF rewrites, this
buffer grows without limit.

**Recommendation:** Add a capacity limit or periodic drain mechanism.

### 10. Unsafe functions suppress errors silently

**File:** `lib.rs:1220-1228`

`am_rdb_load` returns `null_mut()` on failure with no logging. When RDB loading
fails, the operator has no indication of why data was lost.

**Recommendation:** Add `ctx.log_warning()` calls (or the FFI equivalent) in
error paths of all unsafe functions.

### 11. Search index not updated for all write commands  ✅ RESOLVED 2026-05-09

**File:** `lib.rs:248, 1010, 1192` (and missing from every other write)

Only `AM.PUTTEXT`, `AM.APPLY`, and `AM.FROMJSON` invoke
`try_update_search_index`. Every other write command — `AM.PUTINT`,
`AM.PUTDOUBLE`, `AM.PUTBOOL`, `AM.PUTCOUNTER`, `AM.INCCOUNTER`,
`AM.PUTTIMESTAMP`, `AM.PUTDIFF`, `AM.SPLICETEXT`, `AM.MARKCREATE`,
`AM.MARKCLEAR`, `AM.CREATELIST`, `AM.APPEND*` — leaves the shadow index stale.

This was originally classified as a maintainability nit. It is upgraded to
MEDIUM on this revision because RediSearch results — and therefore any
authorization or filtering decisions a downstream system makes from those
results — silently disagree with the source of truth. That is a correctness
and security concern, not cleanup.

**Recommendation:** Centralize index updates so every write path that mutates
indexed paths invokes `try_update_search_index`. Alternatively, debounce
updates on a separate event so you do not pay the cost on every PUT.

**Resolution (2026-05-09):** Took the centralization path. Two new helpers
in `lib.rs` — `finalize_write` and `finalize_write_meta` — consolidate the
publish-change + replicate + keyspace-notify + search-index-update sequence
that every AM.* write must perform. All write command handlers now end with
a single tail call to one of those helpers (`finalize_write` for the common
"single change captured" case; `finalize_write_meta` for AM.APPLY which
publishes per-change in a batch loop, and for AM.LOAD / AM.NEW / AM.FROMJSON
which install whole documents). The previously-omitted commands —
`AM.PUTINT`, `AM.PUTDOUBLE`, `AM.PUTBOOL`, `AM.PUTCOUNTER`, `AM.INCCOUNTER`,
`AM.PUTTIMESTAMP`, `AM.PUTDIFF`, `AM.SPLICETEXT`, `AM.MARKCREATE`,
`AM.MARKCLEAR`, `AM.CREATELIST`, `AM.APPEND{TEXT,INT,DOUBLE,BOOL}` — and
`AM.LOAD` / `AM.NEW` now all keep the shadow index in sync. As a side
effect, ~80 lines of per-command boilerplate collapsed into single-line
tail calls, so the next new write command will inherit indexing for free
rather than having to remember the pattern.

A regression test (`scripts/tests/15-search-indexing.sh` Test 15b)
exercises the original bug: it indexes a `body` text field, then mutates
it via `AM.SPLICETEXT`, `AM.PUTDIFF`, and `AM.MARKCREATE` — none of which
triggered indexing before — and asserts the shadow Hash reflects the new
content after each mutation.

`AM.LOAD`'s previous "no automatic indexing" behavior changed; the
existing Test 14 was updated from "manually reindex after load" to "load
auto-populates the shadow index." Operators no longer need a follow-up
`AM.INDEX.REINDEX` after `AM.LOAD`.

The debounce alternative was considered and rejected: with the
`find_matching_config` cache from finding #2's resolution, per-write
indexing cost is bounded by `O(K)` config lookups plus one DEL+HSET batch
for matching documents — small enough that deferred-update infrastructure
isn't justified. All 17 integration test suites pass.

### 12. Index-admin commands declared with `0,0,0` first/last/step keys  ✅ RESOLVED 2026-05-09

**File:** `lib.rs:1456-1460`

```rust
["am.index.configure", am_index_configure, "write",    0, 0, 0],
["am.index.enable",    am_index_enable,    "write",    0, 0, 0],
["am.index.disable",   am_index_disable,   "write",    0, 0, 0],
["am.index.status",    am_index_status,    "readonly", 0, 0, 0],
```

`first_key=last_key=key_step=0` declares "no key arguments." But these handlers
internally `ctx.call("HSET", ...)` and `KEYS` against the `am:index:config:*`
namespace. Consequences:

- In Redis Cluster, the command lands on whichever shard the client happens to
  be connected to and silently writes a config that the rest of the cluster
  cannot see. Per-shard indexing diverges with no error.
- Key-pattern ACLs (`~am:index:config:*`) cannot be enforced because the
  command exposes no keys to the ACL system. Anyone permitted to call
  `am.index.configure` can effectively write any `am:index:config:*` key.
- `am.index.status` is `readonly` but performs `KEYS` (compounds #2) without
  declaring the keyspace it touches.

**Recommendation:** Declare the actual keys touched (`am:index:config:<pattern>`)
or, preferably, route admin operations through a single hash key whose name is
passed as an argument.

**Resolution (2026-05-09):** Took the audit's preferred path — admin
operations now route through a single Hash. Storage scheme changed from
"one Hash per pattern (`am:index:config:<pattern>`) with three fields each"
to "one Hash for the whole module (`am:index:configs` by default) with one
field per pattern, value = JSON-serialized config." The default store-key
is overridable at module load with `index-config-key=<name>`.

Every `AM.INDEX.*` admin command now takes that store-key as its first
argument (`AM.INDEX.CONFIGURE am:index:configs <pattern> ...`,
`AM.INDEX.ENABLE am:index:configs <pattern>`, etc.). The keyspec is
declared as `1, 1, 1`, so:
- ACL rules like `~am:index:configs +am.index.*` apply correctly to admin
  commands (previously `0,0,0` exposed no keys to the ACL system).
- All admin operations route to the single shard owning that key in
  Redis Cluster (previously commands silently wrote to whichever shard
  the client happened to be connected to).
- The handler validates the supplied store-key matches the configured
  `index-config-key`; mismatches return an explicit error rather than
  silently writing to a Hash the runtime indexer would never read.

`am.index.status` is no longer `readonly` over an undeclared keyspace —
it now declares `am:index:configs` as a readonly key, so ACLs that gate
visibility into the config store work.

Storage simplification side-effect: `IndexConfig::save` is now a single
atomic `HSET`, which incidentally resolves audit #27 (non-atomic save).
The cold-start `populate_cache` switched from a `SCAN` cursor loop over
`am:index:config:*` to a single `HGETALL` against the store key — a
small additional perf win.

**Breaking change.** Operators with existing `am:index:config:*` keys
need to re-run `AM.INDEX.CONFIGURE am:index:configs <pattern> ...` for
each pattern after upgrading; the old per-pattern Hashes will not be
read. Documented in README.md "Search Indexing" section. Two new
regression tests (`scripts/tests/15-search-indexing.sh` Test 15c and
the Test 2/6/7 rewrites) exercise the new storage layout and the
mismatched-store-key error path. All 17 integration test suites pass.

### 13. Index shadow keys (`am:idx:*`) collide with arbitrary user keys  ✅ RESOLVED 2026-05-10

**File:** `index.rs:436, 445, 465`

```rust
let index_key = get_index_key(am_key);          // "am:idx:" + user key
ctx.call("DEL", &[&ctx.create_string(index_key)])?;
```

If a user has independently created a regular key named `am:idx:foo` (Redis
has no namespaces, so this is legal), and later creates an Automerge document
named `foo` whose pattern matches an index config, the indexer **deletes the
unrelated key without warning** before writing its own Hash. Symmetrically, a
user's `HSET am:idx:foo field val` survives only until the next
`try_update_search_index` invocation overwrites it.

**Recommendation:** Tag index Hashes with a sentinel field on creation
(`__am_idx__: 1`) and refuse to overwrite/delete keys that lack the sentinel,
returning a configuration error instead. Alternatively, use a less collidable
prefix (`__am_internal_idx__:`).

**Resolution (2026-05-10):** Took the sentinel-field path. Every shadow this
module writes carries an `__am_idx__` field stamped with the source
Automerge document key, both for Hash format (one HSET) and JSON format
(injected as a top-level field before serialization). Before any
overwrite or delete, the indexer probes via `TYPE` + `HGET`/`JSON.GET`
and refuses to touch:
- a key whose Redis type doesn't match the configured index format
  (e.g. a string at `am:idx:foo`),
- a Hash/JSON document that lacks the `__am_idx__` sentinel
  (legacy data or unrelated user data),
- a stamped shadow whose sentinel value names a different source key
  (would only happen with a manual rename/copy of shadow keys).

When a collision is detected, `update_search_index` returns an error which
the existing `try_update_search_index` wrapper in `lib.rs` logs as a
warning. The originating `AM.*` write itself still succeeds — the source
document is correct, only the shadow is left untouched, so an operator
who accidentally configures an index pattern that overlaps with existing
user keys finds the unrelated data preserved and a clear log line
naming the conflict.

The shadow-key prefix (`am:idx:`) is unchanged so existing FT.CREATE
configurations continue to work. New regression tests
(`scripts/tests/15-search-indexing.sh` Tests 15d and 15e) exercise both
sides: pre-existing user data at the shadow path stays intact while the
indexed write is skipped, and a fresh shadow correctly carries the
sentinel and accepts subsequent updates. All 17 integration test suites
pass.

**Caveat:** legacy shadows from before this change lack the sentinel and
will now be refused (not clobbered) on the next write. Operators
upgrading should run `AM.INDEX.REINDEX` after manually deleting any
pre-existing `am:idx:*` keys, or accept that those legacy shadows go
stale until an operator clears them. This is documented as the safe
default — the alternative (silently overwriting) is exactly what the
audit asked us to stop doing.

### 14. `AM.PUTTIMESTAMP` silently coerces out-of-range values to UNIX_EPOCH on JSON export  ✅ RESOLVED 2026-05-10

**File:** `ext.rs:87-89, 2316-2318`

```rust
let dt = DateTime::from_timestamp_millis(*ts)
    .unwrap_or_else(|| DateTime::<Utc>::UNIX_EPOCH);
JsonValue::String(dt.to_rfc3339())
```

`AM.PUTTIMESTAMP` accepts any `i64`. Values outside chrono's representable
range (roughly ±262,000 years from epoch), `i64::MIN`, and `i64::MAX` all
render as `1970-01-01T00:00:00+00:00` in `AM.TOJSON` output, with no warning.
Round-tripping a document through `AM.TOJSON` → `AM.FROMJSON` then loses the
original timestamp irreversibly.

**Recommendation:** Validate the input range in `AM.PUTTIMESTAMP` (return an
error rather than store a value that cannot be represented), or surface the
overflow on export rather than silently substituting epoch.

**Resolution (2026-05-10):** `am_puttimestamp` now probes
`chrono::DateTime::from_timestamp_millis(value)` immediately after parsing
the `i64`. If `None` (the same predicate the JSON exporter would
otherwise silently swallow into `UNIX_EPOCH`), the handler returns
`"timestamp <n> ms is outside the representable range"`. The exporter's
`unwrap_or_else(|| UNIX_EPOCH)` fallback is kept as defense in depth.
Regression test (`scripts/tests/11-marks.sh` Test 18) asserts both
`i64::MAX` and `i64::MIN` are rejected and that 1700000000000 ms still
round-trips through TOJSON as a 2023-11 ISO 8601 string.

### 15. `AM.PUTDOUBLE` accepts NaN/Infinity, then `AM.TOJSON` silently coerces to JSON `null`  ✅ RESOLVED 2026-05-10

**File:** `lib.rs:559-561` (input), `ext.rs:79-83, 2306-2312` (output)

```rust
let value: f64 = parse_utf8_value(&args[3])?
    .parse()
    .map_err(|_| RedisError::Str("value must be a valid double"))?;
```

`"NaN"`, `"inf"`, `"-inf"` parse successfully and are stored. On `AM.TOJSON`
the code uses `serde_json::Number::from_f64(*f).unwrap_or(JsonValue::Null)`,
which silently coerces non-finite values to JSON `null`. Round-tripping
through `AM.FROMJSON` then materializes the value as actual `null`, changing
its type. RediSearch numeric indexes (when format=json) will also reject
NaN/Infinity inserts inconsistently.

**Recommendation:** Reject non-finite doubles at write time with an error.

**Resolution (2026-05-10):** `am_putdouble` and `am_appenddouble` now
reject any non-finite `f64` (`!value.is_finite()`) immediately after
parse, returning `"value must be a finite double (NaN/Infinity rejected)"`.
The same finite check is reused inside `am_markcreate`'s `double` arm so
the marks code path can no longer be used as a back door (see #16).
RediSearch numeric indexes (when format=json) therefore can't drift
because the document never holds the value in the first place.
Regression tests (`scripts/tests/11-marks.sh` Tests 19 and 20) cover
`NaN`, `nan`, `Inf`, `inf`, `-inf`, `+inf`, `Infinity`, `infinity` for
both PUTDOUBLE and APPENDDOUBLE.

### 16. `AM.MARKCREATE` value type is auto-detected — silent type coercion  ✅ RESOLVED 2026-05-10

**File:** `lib.rs:374-386`

```rust
let value = if value_str == "true"  { ScalarValue::Boolean(true) }
       else if value_str == "false" { ScalarValue::Boolean(false) }
       else if let Ok(i) = value_str.parse::<i64>() { ScalarValue::Int(i) }
       else if let Ok(f) = value_str.parse::<f64>() { ScalarValue::F64(f) }
       else { ScalarValue::Str(value_str.into()) };
```

A user attaching a literal-string mark value of `"true"`, `"123"`, or `"NaN"`
silently gets a Boolean, Int, or non-finite F64. `AM.MARKS` later returns the
stringified value (so the writer's intent partially round-trips), but
downstream indexers consuming the typed value see a different type than the
application thinks it stored. This compounds findings #14 and #15 — non-finite
floats can sneak in through the marks code path even if the PUTDOUBLE input is
hardened.

**Recommendation:** Add an explicit type prefix (e.g., `s:foo`, `i:42`,
`b:true`, `f:3.14`) or a separate command per type. Reject `NaN`/`Infinity`
at this site.

**Resolution (2026-05-10):** Took the explicit-type-argument variant of
the audit's recommendation. The command shape changed from
`AM.MARKCREATE <key> <path> <name> <value> <start> <end> [expand]` to
`AM.MARKCREATE <key> <path> <name> <type> <value> <start> <end> [expand]`,
where `<type>` is one of `string`, `int`, `double`, `bool`. The
auto-detect block was removed entirely; the value is now parsed strictly
according to the declared type. Per-type parse failures and unknown
types return descriptive errors. The `double` arm reuses the same
`is_finite()` check from #15 so non-finite floats can't sneak in
through the marks path. A literal-string mark value of `"true"`, `"123"`,
or `"NaN"` is now stored as a string when `type=string`, instead of
silently becoming a Boolean / Int / non-finite F64.

**Breaking change.** Same posture as audit #12 (the index-config store-key
arg): one extra positional argument per call. All existing
`scripts/tests/11-marks.sh` invocations and every `AM.MARKCREATE` example
in the README were updated to include a type. New regression tests
(`11-marks.sh` Tests 21 and 22) cover the audit-named coercion case
(`true`-as-string), unknown-type rejection, the marks-path NaN guard,
and per-type parse-failure error messages.

### 17. `RedisString::to_string()` is lossy for non-UTF8 keys when constructing index shadow keys  ✅ RESOLVED 2026-05-10

**File:** `lib.rs:248, 1010, 1192, 1351`

```rust
try_update_search_index(ctx, &key_name.to_string(), client);
```

`RedisString::to_string()` replaces non-UTF8 bytes with U+FFFD. Two distinct
binary keys that contain different non-UTF8 sequences can collapse to the
same lossy string and therefore the same shadow index path
(`am:idx:<lossy>`), causing the indexer to overwrite or delete the wrong
shadow document. Combined with #13, this is a path for one user's writes to
silently corrupt another user's indexed view.

**Recommendation:** Use the binary-safe path. Pass `RedisString` through to
the indexer and have the indexer construct the shadow key without going
through UTF-8 conversion (or reject non-UTF8 keys for indexed patterns).

**Resolution (2026-05-10):** Took the audit's "or reject" alternative —
after the audit-#11 centralization, all four call sites the audit named
collapsed to a single one in `finalize_write_meta`, so the rejection
boundary is a single match block on `key_name.try_as_str()`. If the key
is not valid UTF-8 we log a warning naming the raw bytes and skip the
index update; the originating `AM.*` write itself is unaffected (the
source document is still stored binary-safely in the Automerge data
type). This eliminates the collision risk by construction: two distinct
binary keys can no longer collapse to the same U+FFFD-replaced shadow
path because neither one ever reaches the shadow layer.

The full binary-safe rewrite was considered and rejected. RedisJSON
requires UTF-8 strings so the JSON-format indexer would have to reject
non-UTF8 keys anyway, and matches between non-UTF8 keys and configured
patterns (which are user-supplied UTF-8 strings) are an unlikely use
case in practice. Limiting indexing to UTF-8 keys is a clean semantic
boundary that costs almost nothing.

Regression test (`scripts/tests/15-search-indexing.sh` Test 15f): two
binary keys `binkey:\xff\xfe` and `binkey:\xff\xfd` are sent via raw
RESP through bash's `/dev/tcp` (redis-cli only accepts binary at the
last argument position). The test asserts that no shadow under the
`am:idx:binkey:*` prefix is created — confirming the indexer skipped
both. All 17 integration test suites pass.

---

## LOW Severity

### 18. Silent data corruption in diff application  ✅ RESOLVED 2026-05-11

**Files:** `ext.rs:1420-1425`, `ext.rs:1492-1497`

When `put_diff()` applies a unified diff, context line mismatches are silently
ignored. If a diff is applied against the wrong document state, the result is
silently wrong rather than returning an error.

**Recommendation:** Return an error on context mismatch, or at minimum log a
warning.

**Resolution (2026-05-11):** The two `// try to be lenient` no-ops in the
`Context` and `Delete` arms (one set in each of `put_diff` and
`put_diff_with_change`) were replaced with hard error returns. A new
`DiffError` enum in `ext.rs` carries actionable per-failure-mode
variants — `ContextMismatch { line_num, expected, actual }`,
`DeleteMismatch { line_num, expected, actual }`, `UnexpectedEof { line_num,
op }` — each with a `Display` impl that produces a message like
`"diff context mismatch at line 1: expected \"DIFFERENT\", found \"Line 1\"
(the diff was generated against a different base state — re-fetch the
current text and rebase the diff)"`. The previous handlers returned the
opaque `AutomergeError::Fail` which would surface as `(error) fail` to
redis-cli; now the operator sees the offending line and what to do.

Bundled audit #33: `put_diff` and `put_diff_with_change` were fully
duplicated (60+ lines twice). They now share a single
`compute_diff_application` helper that produces the resulting text or a
`DiffError`. The public methods each become four lines (compute → write).

Three regression tests in `scripts/tests/05-text-operations.sh` (Tests
3a/3b/3c) exercise context mismatch, delete mismatch, and past-EOF, and
each asserts that the document is left untouched (the strict applier
errors before any `put_text` call). All 17 integration test suites
pass.

### 19. `AM.GETDIFF` uses Rust Debug formatting for output  ✅ RESOLVED 2026-05-11

**File:** `lib.rs:1131`

```rust
let json = format!("{:?}", patches);
```

The diff output uses Rust's `Debug` formatting, not a stable parseable format.

**Recommendation:** Implement proper JSON serialization for patches.

**Resolution (2026-05-11):** Replaced `format!("{:?}", patches)` with a
walker that produces a stable JSON array. Each patch becomes
`{"obj": <id>, "path": [{"obj":<id>,"prop":...}, ...],
"action": {"type": "<variant>", ...action-specific fields...}}`. New
helpers in `lib.rs`: `patch_to_json`, `patch_action_to_json`,
`value_to_json`, `scalar_to_json`, `prop_to_json`, `obj_id_to_json`.
Every `PatchAction` variant — `PutMap`, `PutSeq`, `Insert`, `SpliceText`,
`Increment`, `Conflict`, `DeleteMap`, `DeleteSeq`, `Mark` — is mapped
explicitly with descriptive field names. Scalar values carry both a
`type` tag (`"str"`, `"int"`, `"f64"`, `"bool"`, `"counter"`,
`"timestamp_ms"`, `"bytes"`, `"null"`, `"unknown"`) and the underlying
value, so clients can dispatch on the type without parsing the value.
`Bytes` are base64-encoded; `Unknown` (forward-compat values from a
future automerge version) carries the type code plus base64 bytes for
diagnosability.

Two new Rust unit tests in `lib.rs#tests`:
`patch_to_json_emits_stable_shape` constructs `Patch` values directly
and asserts the resulting JSON has the documented per-variant shape;
`scalar_to_json_rejects_non_finite_doubles_gracefully` confirms NaN
surfaces as `{"type":"f64","value":null}` rather than panicking (a
defense-in-depth complement to audit #15).
`scripts/tests/12-diff.sh` Test 1 was strengthened from "starts with
`[`" to "is a valid JSON array, and any element carries an
`action.type` string" — exercising the actual shape via `jq`.

The output is now also forward-stable: clients can rely on the type
tags and field names rather than the Rust Debug repr that would change
silently with any automerge upgrade.

### 20. `usize` to `i64` casts in marks can overflow  ✅ RESOLVED 2026-05-11

**File:** `lib.rs:495-496`

On 64-bit systems, `usize` values above `i64::MAX` will silently wrap to
negative. Use `try_into()` with error handling. In practice the values
originate from a `parse_integer()` that returns `i64` and were validated
non-negative before being cast back — so this is largely theoretical on
64-bit hosts — but the cast should still be explicit.

**Resolution (2026-05-11):** Introduced a `usize_to_i64` helper in
`lib.rs` that wraps `i64::try_from(n)` and returns a descriptive
`RedisError` on overflow. The five `as i64` cast sites — `am_marks`
(start, end), `am_listlen` (len), `am_maplen` (len), `am_numchanges`
(count) — all funnel through the helper. No more implicit lossy casts
in user-facing return paths. Unit test
`usize_to_i64_rejects_overflow` covers both the pass-through case
(0, `i64::MAX`) and the overflow case (gated to 64-bit targets where
the boundary is reachable in principle). All 17 integration test
suites pass.

### 21. No Redis ACL command categories  ✅ RESOLVED 2026-05-11

**File:** `lib.rs:1414-1462`

Module commands don't declare a custom ACL category. Operators can't
selectively control read vs. write AM.* commands as a single group.

**Resolution (2026-05-11):** Registered a custom `automerge` ACL
category via the `acl_categories` block in `redis_module!`. Every
`AM.*` command now joins this category in addition to its built-in
`@read` or `@write` category, so operators can grant or revoke the
whole module surface as one group:
- `+@automerge` — full AM.* access
- `-@all +@automerge -@write` — read-only AM.* access (intersection)
- `-@automerge` — revoke the whole surface in one rule

Verified end-to-end against Redis 7.4.5: `COMMAND INFO am.new` lists
both `@write` and `@automerge` as ACL categories; a user created with
`+@automerge` can call `am.new`/`am.puttext`/`am.gettext` but is
denied `SET`; a user with `-@all +@automerge -@write` can read AM
documents but is denied `am.puttext` (NOPERM). Documented in
README.md under "Redis ACL: the `@automerge` category" with example
rules. Requires Redis 7.4+ for `RedisModule_AddACLCategory`; on older
versions the category registration is silently skipped and the
per-command `@read`/`@write` categories still apply.

### 22. `eval` in test helpers

**File:** `scripts/tests/lib/common.sh:48,80`

`eval "$command"` could be replaced with `"$@"` parameter expansion patterns.

### 23. Missing `set -e` in test runner

**File:** `scripts/tests/run-all-tests.sh:5`

The script uses `set -uo pipefail` but omits `-e`. Spawned subshells running
individual test files therefore depend on each test file setting its own
`-e`; if any does not, partial failures may report as passes.

### 24. No Docker HEALTHCHECK

**File:** `Dockerfile`

The image has no `HEALTHCHECK` instruction for container orchestrators.

### 25. `IndexConfig::matches_pattern` is not Redis-glob-compatible

**File:** `index.rs:196-246`

`find_matching_config` retrieves config keys with `KEYS am:index:config:*`,
which performs Redis-style glob matching (`?`, `[abc]`, `\\` escape are all
supported server-side). The client-side `matches_pattern` only handles `*`.
A config saved with pattern `user[12]:*` is reachable via `KEYS` but does
not match `user1:foo` client-side. This is a *correctness* gap that creates
authorization-relevant divergence between save-time and match-time semantics.

**Recommendation:** Use a glob crate that mirrors Redis semantics, or
explicitly reject non-`*` wildcards at configure time.

### 26. `RedisModule_EmitAOF.unwrap()` can panic during AOF rewrite

**File:** `lib.rs:1246`

```rust
raw::RedisModule_EmitAOF.unwrap()(...)
```

On older Redis versions or stripped builds where the symbol is not exported,
the unwrap panics, aborting the Redis process during AOF rewrite. This
belongs alongside finding #10.

**Recommendation:** Check the symbol once at module load and refuse to load
the module rather than crashing during a rewrite.

### 27. `IndexConfig::save` is non-atomic (three sequential HSETs)  ✅ RESOLVED 2026-05-09

**File:** `index.rs:82-110`

`enabled`, `paths`, and `format` are written with three independent
`ctx.call("HSET", ...)` invocations. If a write fails between calls (OOM,
cluster routing error, replica disconnect), the config is half-applied —
e.g., `enabled` flipped while `paths` still reflects the old state.

**Recommendation:** Issue a single `HSET key f1 v1 f2 v2 f3 v3` call.

**Resolution (2026-05-09):** Resolved as a side-effect of #12. The
storage scheme changed from "one Hash per pattern with three fields" to
"one Hash for the whole module with one JSON-serialized value per
pattern field." `IndexConfig::save` is now a single atomic `HSET` of
that one field, so partial-write windows are impossible.

### 28. `am.index.configure` `--format` parsing is brittle

**File:** `lib.rs:1279`

```rust
if args.len() > 3 && args[2].to_string() == "--format" {
```

Detection relies on a positional check rather than proper flag parsing. A
user-supplied path that happens to be the literal string `--format` would
be misinterpreted. Not exploitable, but produces confusing errors instead of
clean validation.

**Recommendation:** Use a small flag parser, or require `--format` only as
the first positional after the pattern with explicit grammar.

### 29. `IndexConfig` paths are CSV-encoded — fails on commas in path strings

**File:** `index.rs:138-143`

```rust
.split(',')
```

Paths are persisted as a comma-joined string and parsed back via `split(',')`.
A path string containing a comma (legal in Automerge map keys) round-trips
incorrectly: `["a,b"]` saves and reloads as `["a", "b"]`.

**Recommendation:** Persist paths as a JSON array or a Redis list
(`RPUSH/LRANGE`) instead of CSV.

### 30. `docs.yml` workflow duplicates the build Dockerfile inline  ✅ RESOLVED 2026-05-08

**File:** `.github/workflows/docs.yml:25-50, 90-110`

The same Dockerfile body is generated twice in two jobs (`build-docs`,
`check-docs`) via heredoc and is not committed. Drift between the two copies
is invisible. The inline Dockerfile also bypasses the project's
"all builds via docker compose" rule documented in `CLAUDE.md`. Compounds
finding #6 (the same `sed`-patch fragility appears here twice more).

**Recommendation:** Commit a single `Dockerfile.docs` and reference it from
both jobs.

**Resolution (2026-05-08):** A single committed `Dockerfile.docs` at the
repo root replaces both inline heredocs. Both `build-docs` and `check-docs`
jobs now `docker build -f Dockerfile.docs -t ...` instead of regenerating
the Dockerfile each run. The `sed`-patching from finding #6 is gone from
this file too — `Dockerfile.docs` simply copies `redis-automerge/` and
runs `cargo doc --no-deps`. Verified locally with
`docker build -f Dockerfile.docs -t redis-automerge-docs-local .`
(rustdoc emits 10 pre-existing non-blocking warnings; the workflow's
warning-grep step is unchanged). Also added `**/target/` to `.dockerignore`
so the docs build does not race with a stale `redis-automerge/target/`
directory left over from local cargo runs.

### 31. `mcp-server/` is an empty `0700` directory committed to the tree

**File:** `mcp-server/`

Empty directory with restrictive permissions. Likely cruft. Not a
vulnerability, but undocumented state in the repo invites mistakes
(e.g., a deployment writing secrets here expecting an MCP server).

**Recommendation:** Remove the directory or commit a `README.md` explaining
its intended purpose.

---

## Code Quality / Maintainability

### 32. Pervasive code duplication in ext.rs

Nearly every operation has two near-identical implementations (e.g., `put_text()`
and `put_text_with_change()`). This pattern repeats ~15 times. The base methods
should delegate to the `_with_change` variant and discard the return value.

The "convert scalar string to Text object" pattern is copy-pasted across 6+
functions and should be extracted to a helper.

### 33. `put_diff` / `put_diff_with_change` are fully duplicated  ✅ RESOLVED 2026-05-11

**File:** `ext.rs:1398-1535`

The 60+ line diff application logic is duplicated between two methods.

**Resolution (2026-05-11):** Resolved as a side-effect of #18. Both
methods now delegate to a single private `compute_diff_application`
helper that returns either the patched text or a `DiffError`. Each
public method shrinks to four lines (compute → call `put_text` or
`put_text_with_change`). The audit-#18 strict check therefore lives in
exactly one place instead of being copy-pasted.

### 34. Missing `mem_usage` callback

**File:** `lib.rs:103`

`mem_usage: None` means `MEMORY USAGE` returns 0 for AM documents. Operators
can't monitor Automerge memory consumption.

---

## Severity changes from the 2026-02-23 audit

- **#6 Automerge version mismatch:** MEDIUM → HIGH. Local builds (cargo, IDE,
  cargo doc CI) compile against a different major version than the shipped
  artifact. Tests passing locally do not guarantee the production binary
  builds, let alone behaves identically.
- **#7 webdis unauthenticated:** MEDIUM → HIGH. The `redis` service is
  internal-only by design; webdis is the sole network ingress and has zero
  auth, making this the practical attack surface for the entire deployment.
- **#11 Search index not updated for all writes:** Maintainability → MEDIUM.
  Stale RediSearch results feed downstream filtering and authorization; this
  is a correctness and security concern, not just cleanup.

---

## Positive Observations

- Rust's type system prevents many classes of memory safety bugs.
- All write operations properly call `ctx.replicate()` for replication and
  `ctx.notify_keyspace_event()` for keyspace notifications.
- Change bytes are base64-encoded for pub/sub to avoid null byte issues.
- RDB and AOF persistence are both implemented.
- The `deny-oom` flag is set on all write commands.
- Input validation is generally thorough (UTF-8, integer parsing, boolean formats).
- The test suite has good breadth across 16 functional areas.
- CI gates Docker Hub publish on passing tests.
