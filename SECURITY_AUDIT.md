# Code & Security Audit: redis-automerge

**Date:** 2026-02-23
**Scope:** Full codebase review of all Rust source, Docker configuration, CI/CD, and test infrastructure.

---

## HIGH Severity

### 1. `--enable-debug-command yes` in production Dockerfile

**File:** `Dockerfile:38`

The Redis configuration enables the `DEBUG` command. This allows any connected
client to execute `DEBUG RESTART`, `DEBUG SET-ACTIVE-EXPIRE`, `DEBUG SLEEP`,
`DEBUG OOM`, etc. This is a denial-of-service vector and should never be enabled
in production images. The test infrastructure uses `DEBUG RESTART` (in
`common.sh:136`), but this should be separated from the production image via a
separate test Dockerfile or runtime override.

**Recommendation:** Remove `--enable-debug-command yes` from the Dockerfile CMD.
Override it in docker-compose.yml for the test service only.

### 2. `KEYS` command used in hot path (O(N) keyspace scan on every write)

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

### 3. No size limits on deserialized input (`AM.LOAD`, `AM.APPLY`, `AM.FROMJSON`)

**Files:** `lib.rs:173-189`, `lib.rs:972-1014`, `lib.rs:1167-1197`

- `AM.LOAD` accepts arbitrary binary data and deserializes it with no size check.
- `AM.APPLY` accepts an unbounded number of raw change byte arguments.
- `AM.FROMJSON` parses arbitrarily large JSON strings into Automerge documents.

Any of these can be used to exhaust server memory.

**Recommendation:** Add configurable maximum size limits for input data. Reject
inputs exceeding the limit with an appropriate error message.

---

## MEDIUM Severity

### 4. `Cargo.lock` is gitignored — non-reproducible builds

**File:** `redis-automerge/.gitignore:2`

For a binary crate producing a `.so` file (`cdylib`), `Cargo.lock` should be
committed to ensure reproducible builds. Without it, builds on different
machines/times may pull different transitive dependency versions.

**Recommendation:** Remove `Cargo.lock` from `redis-automerge/.gitignore` and
commit the lock file.

### 5. Automerge version mismatch between Cargo.toml and Dockerfile

**Files:** `Cargo.toml:10`, `Dockerfile:10-17`

`Cargo.toml` declares `automerge = "1.0.0-beta.6"` (a beta from the 1.x line).
The Dockerfile clones automerge at tag `js/automerge-3.1.2` (3.x line) and
patches `Cargo.toml` with `sed`. This means:

- Building with `cargo build` outside Docker uses a completely different Automerge API.
- The declared version is misleading.
- The `sed` patching is fragile.

**Recommendation:** Use a `[patch]` or path override in a `Cargo.toml` workspace,
or use a git dependency directly in `Cargo.toml` so the source of truth is clear.

### 6. Unbounded AOF buffer growth

**File:** `ext.rs:364-367`

The `aof: Vec<Vec<u8>>` buffer accumulates all change bytes since the last
`commands()` drain. If many operations are applied between AOF rewrites, this
buffer grows without limit.

**Recommendation:** Add a capacity limit or periodic drain mechanism.

### 7. Unsafe functions suppress errors silently

**File:** `lib.rs:1220-1228`

`am_rdb_load` returns `null_mut()` on failure with no logging. When RDB loading
fails, the operator has no indication of why data was lost.

**Recommendation:** Add `ctx.log_warning()` calls (or the FFI equivalent) in
error paths of all unsafe functions.

### 8. `webdis` exposes unauthenticated HTTP access to Redis

**File:** `docker-compose.yml:14-15`

The `webdis` service maps port 7379 to the host, providing unauthenticated HTTP
access to all Redis commands including AM.* write commands.

**Recommendation:** Either remove the port mapping (use `expose` instead of
`ports`), add authentication, or document that this is for development only.

---

## LOW Severity

### 9. Silent data corruption in diff application

**Files:** `ext.rs:1420-1425`, `ext.rs:1492-1497`

When `put_diff()` applies a unified diff, context line mismatches are silently
ignored. If a diff is applied against the wrong document state, the result is
silently wrong rather than returning an error.

**Recommendation:** Return an error on context mismatch, or at minimum log a
warning.

### 10. `AM.GETDIFF` uses Rust Debug formatting for output

**File:** `lib.rs:1131`

```rust
let json = format!("{:?}", patches);
```

The diff output uses Rust's `Debug` formatting, not a stable parseable format.

**Recommendation:** Implement proper JSON serialization for patches.

### 11. `usize` to `i64` casts in marks can overflow

**File:** `lib.rs:495-496`

On 64-bit systems, `usize` values above `i64::MAX` will silently wrap to
negative. Use `try_into()` with error handling.

### 12. No Redis ACL command categories

**File:** `lib.rs:1414-1462`

Module commands don't declare ACL categories. Operators can't selectively
control read vs. write AM.* commands.

### 13. `eval` in test helpers

**File:** `scripts/tests/lib/common.sh:48,80`

`eval "$command"` could be replaced with `"$@"` parameter expansion patterns.

### 14. Missing `set -e` in test runner

**File:** `scripts/tests/run-all-tests.sh:5`

The script uses `set -uo pipefail` but omits `-e`.

### 15. No Docker HEALTHCHECK

**File:** `Dockerfile`

The image has no `HEALTHCHECK` instruction for container orchestrators.

---

## Code Quality / Maintainability

### 16. Pervasive code duplication in ext.rs

Nearly every operation has two near-identical implementations (e.g., `put_text()`
and `put_text_with_change()`). This pattern repeats ~15 times. The base methods
should delegate to the `_with_change` variant and discard the return value.

The "convert scalar string to Text object" pattern is copy-pasted across 6+
functions and should be extracted to a helper.

### 17. `put_diff` / `put_diff_with_change` are fully duplicated

**File:** `ext.rs:1398-1534`

The 60+ line diff application logic is duplicated between two methods.

### 18. Missing `mem_usage` callback

**File:** `lib.rs:103`

`mem_usage: None` means `MEMORY USAGE` returns 0 for AM documents. Operators
can't monitor Automerge memory consumption.

### 19. Search index not updated for all write commands

Only `AM.PUTTEXT`, `AM.APPLY`, and `AM.FROMJSON` update the search index. All
other write commands (`AM.PUTINT`, `AM.PUTDOUBLE`, `AM.PUTBOOL`, etc.) do not,
causing index staleness.

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
