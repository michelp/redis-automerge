///! Search indexing support for Automerge documents.
///!
///! This module provides functionality to automatically sync Automerge document fields
///! to Redis Hashes or RedisJSON documents that can be indexed by RediSearch.

use crate::ext::{RedisAutomergeClient, TypedValue};
use crate::index_config_key;
use redis_module::{Context, RedisError, RedisResult, RedisValue};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::sync::RwLock;

/// Prefix for shadow Hash keys
const INDEX_KEY_PREFIX: &str = "am:idx:";

/// Sentinel field embedded in every shadow document this module owns. The
/// value is the source Automerge document key, giving the shadow a clear
/// owner stamp. Before overwriting or deleting a shadow at `am:idx:<key>`,
/// the indexer probes for this field and refuses to touch keys that lack
/// it, so a user's pre-existing key at e.g. `am:idx:foo` is never silently
/// clobbered. See SECURITY_AUDIT.md #13.
pub const INDEX_SENTINEL_FIELD: &str = "__am_idx__";

/// Result of probing a candidate shadow key before we write/delete.
enum ShadowState {
    /// The key does not exist; safe to create a fresh shadow.
    Absent,
    /// The key exists and belongs to this module (sentinel matches).
    OwnedByUs,
    /// The key exists but is not a shadow we own. Caller must abort the
    /// write rather than clobber an unrelated user key.
    Conflict(String),
}

/// Probe whether the shadow key at `index_key` is safe to overwrite or
/// delete. The probe uses `TYPE` (no allocation, O(1)) followed by either
/// `HGET` (Hash format) or `JSON.GET` (JSON format) to read the sentinel
/// field. Audit #13.
fn check_shadow_ownership(
    ctx: &Context,
    index_key: &str,
    am_key: &str,
    format: IndexFormat,
) -> RedisResult<ShadowState> {
    let key_rs = ctx.create_string(index_key);
    let type_result = ctx.call("TYPE", &[&key_rs])?;
    let type_str = match type_result {
        RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s,
        _ => return Ok(ShadowState::Conflict("unexpected TYPE response".to_string())),
    };
    if type_str == "none" {
        return Ok(ShadowState::Absent);
    }
    let expected_type = match format {
        IndexFormat::Hash => "hash",
        IndexFormat::Json => "ReJSON-RL",
    };
    if type_str != expected_type {
        return Ok(ShadowState::Conflict(format!(
            "key type {:?}, expected {:?}",
            type_str, expected_type
        )));
    }
    let stamped = match format {
        IndexFormat::Hash => {
            let result = ctx.call(
                "HGET",
                &[&key_rs, &ctx.create_string(INDEX_SENTINEL_FIELD)],
            )?;
            match result {
                RedisValue::SimpleString(s) | RedisValue::BulkString(s) => Some(s),
                _ => None,
            }
        }
        IndexFormat::Json => {
            let path = format!("$.{}", INDEX_SENTINEL_FIELD);
            let result = ctx.call("JSON.GET", &[&key_rs, &ctx.create_string(path.as_str())])?;
            match result {
                RedisValue::SimpleString(s) | RedisValue::BulkString(s) => {
                    // JSON.GET returns a JSON array (e.g. `["am_key"]`) when
                    // querying with `$.field`. Strip the wrapping to get the
                    // string value back.
                    serde_json::from_str::<Vec<String>>(&s)
                        .ok()
                        .and_then(|mut v| v.pop())
                }
                _ => None,
            }
        }
    };
    match stamped {
        Some(owner) if owner == am_key => Ok(ShadowState::OwnedByUs),
        Some(owner) => Ok(ShadowState::Conflict(format!(
            "shadow owned by {:?}, not {:?}",
            owner, am_key
        ))),
        None => Ok(ShadowState::Conflict(
            "shadow lacks the __am_idx__ sentinel field".to_string(),
        )),
    }
}

/// Returns Err describing the conflict, or Ok with whether the key
/// already existed (so callers can avoid a redundant DEL when Absent).
fn ensure_safe_to_write(
    ctx: &Context,
    index_key: &str,
    am_key: &str,
    format: IndexFormat,
) -> RedisResult<bool> {
    match check_shadow_ownership(ctx, index_key, am_key, format)? {
        ShadowState::Absent => Ok(false),
        ShadowState::OwnedByUs => Ok(true),
        ShadowState::Conflict(why) => Err(RedisError::String(format!(
            "refusing to overwrite shadow key {:?}: {}",
            index_key, why
        ))),
    }
}

/// Process-global cache of `IndexConfig` entries keyed by pattern.
///
/// `None` means the cache has not been populated yet. The first read after a
/// cold start (or after invalidation) calls [`populate_cache`] which performs
/// a single `HGETALL` against the configured index-config storage key.
/// Subsequent reads are O(K) over the number of configured patterns rather
/// than O(N) over the keyspace.
///
/// Invalidated by every write through `AM.INDEX.CONFIGURE`, `AM.INDEX.ENABLE`,
/// `AM.INDEX.DISABLE`, and `AM.INDEX.DELETE`. Direct user manipulation of
/// the storage Hash (e.g. via raw `HSET`/`HDEL`) is not detected; restart
/// Redis to re-read.
static CONFIG_CACHE: RwLock<Option<HashMap<String, IndexConfig>>> = RwLock::new(None);

/// Populate the cache from Redis with a single `HGETALL` against the
/// configured store key. Each field is one pattern; each value is the
/// JSON-serialized `IndexConfig` written by [`IndexConfig::save`].
fn populate_cache(ctx: &Context) -> RedisResult<HashMap<String, IndexConfig>> {
    let mut map: HashMap<String, IndexConfig> = HashMap::new();
    let store_key = ctx.create_string(index_config_key());
    let result = ctx.call("HGETALL", &[&store_key])?;

    let items = match result {
        RedisValue::Array(items) => items,
        _ => return Err(RedisError::Str("unexpected HGETALL response shape")),
    };

    let mut iter = items.into_iter();
    while let (Some(field), Some(value)) = (iter.next(), iter.next()) {
        let pattern = match field {
            RedisValue::BulkString(s) | RedisValue::SimpleString(s) => s,
            _ => continue,
        };
        let serialized = match value {
            RedisValue::BulkString(s) | RedisValue::SimpleString(s) => s,
            _ => continue,
        };
        if let Some(cfg) = IndexConfig::deserialize(&pattern, &serialized) {
            map.insert(pattern, cfg);
        }
    }

    Ok(map)
}

/// Ensure the cache is populated; no-op if already initialized.
fn ensure_cache(ctx: &Context) -> RedisResult<()> {
    {
        let guard = CONFIG_CACHE
            .read()
            .map_err(|_| RedisError::Str("index config cache poisoned"))?;
        if guard.is_some() {
            return Ok(());
        }
    }
    let map = populate_cache(ctx)?;
    let mut guard = CONFIG_CACHE
        .write()
        .map_err(|_| RedisError::Str("index config cache poisoned"))?;
    if guard.is_none() {
        *guard = Some(map);
    }
    Ok(())
}

/// Drop the cache so the next read re-populates from Redis. Called after every
/// `AM.INDEX.*` write so subsequent lookups see fresh state.
pub fn invalidate_cache() {
    if let Ok(mut guard) = CONFIG_CACHE.write() {
        *guard = None;
    }
}

/// Return a snapshot of every cached `IndexConfig`. Used by `AM.INDEX.STATUS`
/// instead of a `KEYS` scan.
pub fn list_configs(ctx: &Context) -> RedisResult<Vec<IndexConfig>> {
    ensure_cache(ctx)?;
    let guard = CONFIG_CACHE
        .read()
        .map_err(|_| RedisError::Str("index config cache poisoned"))?;
    Ok(guard
        .as_ref()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default())
}

/// Reject patterns that use Redis-glob metacharacters the in-tree matcher
/// does not implement. The matcher in [`IndexConfig::matches_pattern`] only
/// honors `*`; if a user configures `user[12]:*` expecting Redis-style glob
/// semantics, the match would silently fail. Rather than vendor a full glob
/// grammar, this gate makes the supported subset loud at configure-time.
/// Audit #25.
///
/// Returns `Err` if `pattern` is empty or contains `?`, `[`, `]`, or `\`.
pub fn validate_pattern(pattern: &str) -> Result<(), RedisError> {
    if pattern.is_empty() {
        return Err(RedisError::Str("index pattern must not be empty"));
    }
    for c in pattern.chars() {
        if matches!(c, '?' | '[' | ']' | '\\') {
            return Err(RedisError::String(format!(
                "index pattern {:?} contains unsupported glob metacharacter {:?}; \
                 only '*' is supported (audit #25)",
                pattern, c
            )));
        }
    }
    Ok(())
}

/// Format for shadow index documents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexFormat {
    /// Store as Redis Hash (flat key-value pairs)
    Hash,
    /// Store as RedisJSON document (preserves structure)
    Json,
}

impl IndexFormat {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "hash" => Some(IndexFormat::Hash),
            "json" => Some(IndexFormat::Json),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            IndexFormat::Hash => "hash",
            IndexFormat::Json => "json",
        }
    }
}

/// Configuration for indexing a key pattern
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// The key pattern (e.g., "article:*", "user:*")
    pub pattern: String,
    /// Whether indexing is enabled for this pattern
    pub enabled: bool,
    /// Paths to extract and index (e.g., ["title", "content", "author.name"])
    pub paths: Vec<String>,
    /// Format for shadow documents (hash or json)
    pub format: IndexFormat,
}

impl IndexConfig {
    /// Create a new index configuration (defaults to Hash format)
    pub fn new(pattern: String, paths: Vec<String>) -> Self {
        Self::new_with_format(pattern, paths, IndexFormat::Hash)
    }

    /// Create a new index configuration with specified format
    pub fn new_with_format(pattern: String, paths: Vec<String>, format: IndexFormat) -> Self {
        Self {
            pattern,
            enabled: true,
            paths,
            format,
        }
    }

    /// Serialize this config to the JSON form stored in the index-config
    /// Hash. The `pattern` field is omitted from the value because it is
    /// already the Hash field name; this keeps the storage compact and
    /// avoids the redundant-pattern-mismatch failure mode.
    fn serialize(&self) -> String {
        let body = serde_json::json!({
            "enabled": self.enabled,
            "paths": self.paths,
            "format": self.format.as_str(),
        });
        body.to_string()
    }

    /// Deserialize a single Hash field value back into an `IndexConfig`. The
    /// caller supplies the pattern (the Hash field name). Returns `None` if
    /// the value cannot be parsed; we tolerate corrupted entries silently
    /// rather than failing the whole `HGETALL` populate, but a warning is
    /// not yet logged because `populate_cache` does not have a `Context`
    /// suitable for logging from the cold-start path. Audit-followup item.
    fn deserialize(pattern: &str, serialized: &str) -> Option<Self> {
        let v: JsonValue = serde_json::from_str(serialized).ok()?;
        let obj = v.as_object()?;
        let enabled = obj.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true);
        let paths = obj
            .get("paths")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let format = obj
            .get("format")
            .and_then(|x| x.as_str())
            .and_then(IndexFormat::from_str)
            .unwrap_or(IndexFormat::Hash);
        Some(Self {
            pattern: pattern.to_string(),
            enabled,
            paths,
            format,
        })
    }

    /// Save configuration to Redis. Writes one JSON-serialized value into a
    /// single Hash field of the configured store key (audit #12 / #27 — one
    /// atomic `HSET` per save, no per-pattern Hash sprawl).
    pub fn save(&self, ctx: &Context) -> RedisResult<()> {
        let store_key = ctx.create_string(index_config_key());
        ctx.call(
            "HSET",
            &[
                &store_key,
                &ctx.create_string(self.pattern.as_str()),
                &ctx.create_string(self.serialize().as_str()),
            ],
        )?;
        Ok(())
    }

    /// Load configuration from Redis.
    pub fn load(ctx: &Context, pattern: &str) -> RedisResult<Option<Self>> {
        let store_key = ctx.create_string(index_config_key());
        let result = ctx.call("HGET", &[&store_key, &ctx.create_string(pattern)])?;
        let serialized = match result {
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s,
            RedisValue::Null => return Ok(None),
            _ => return Ok(None),
        };
        Ok(Self::deserialize(pattern, &serialized))
    }

    /// Delete a configuration entry from the store. Returns the number of
    /// fields actually removed (0 or 1) so callers can pass the count back
    /// through `AM.INDEX.DELETE`.
    pub fn delete(ctx: &Context, pattern: &str) -> RedisResult<i64> {
        let store_key = ctx.create_string(index_config_key());
        let result = ctx.call("HDEL", &[&store_key, &ctx.create_string(pattern)])?;
        Ok(match result {
            RedisValue::Integer(n) => n,
            _ => 0,
        })
    }

    /// Find the configuration that matches a given key.
    ///
    /// Backed by the process-global [`CONFIG_CACHE`]. The first call after a
    /// cold start triggers a single `SCAN` over `am:index:config:*`; every
    /// subsequent call is an in-memory lookup over the configured patterns.
    pub fn find_matching_config(ctx: &Context, key: &str) -> RedisResult<Option<Self>> {
        ensure_cache(ctx)?;
        let guard = CONFIG_CACHE
            .read()
            .map_err(|_| RedisError::Str("index config cache poisoned"))?;
        let map = match guard.as_ref() {
            Some(m) => m,
            None => return Ok(None),
        };
        for (pattern, cfg) in map.iter() {
            if Self::matches_pattern(key, pattern) {
                return Ok(Some(cfg.clone()));
            }
        }
        Ok(None)
    }

    /// Check if a key matches a pattern.
    ///
    /// Only the `*` wildcard is honored — `?`, `[abc]`, and `\` escapes from
    /// Redis-style glob syntax are NOT supported. Patterns containing those
    /// metacharacters are rejected at configure-time by [`validate_pattern`]
    /// so callers never reach this function with an unsupported pattern
    /// (audit #25). Pre-existing stored patterns from before that gate was
    /// added are still tolerated by [`deserialize`] and will simply fail to
    /// match the way they always did.
    pub(crate) fn matches_pattern(key: &str, pattern: &str) -> bool {
        // Simple wildcard matching (* matches any characters)
        if pattern == "*" {
            return true;
        }

        if !pattern.contains('*') {
            return key == pattern;
        }

        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            // Single wildcard: "prefix*" or "*suffix" or "prefix*suffix"
            let prefix = parts[0];
            let suffix = parts[1];

            if prefix.is_empty() {
                return key.ends_with(suffix);
            } else if suffix.is_empty() {
                return key.starts_with(prefix);
            } else {
                return key.starts_with(prefix) && key.ends_with(suffix);
            }
        }

        // Multiple wildcards - simplified matching
        let mut key_pos = 0;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            if let Some(pos) = key[key_pos..].find(part) {
                if i == 0 && pos != 0 {
                    return false; // First part must match at start
                }
                key_pos += pos + part.len();
            } else {
                return false;
            }
        }

        // Last part must match at end
        if let Some(last) = parts.last() {
            if !last.is_empty() && !key.ends_with(last) {
                return false;
            }
        }

        true
    }
}

/// Extract configured paths from an Automerge document for Hash-based indexing
pub fn extract_indexed_fields(
    client: &RedisAutomergeClient,
    paths: &[String],
) -> HashMap<String, String> {
    let mut fields = HashMap::new();

    for path in paths {
        // Try to get the value at this path
        if let Ok(Some(value)) = client.get_text(path) {
            // For nested paths, flatten with underscores for Hash field names
            let field_name = path.replace('.', "_").replace('[', "_").replace(']', "");
            fields.insert(field_name, value);
        }
        // Could also handle other types (int, bool, etc.) by converting to string
        // For now, focus on text fields for full-text search
    }

    fields
}

/// Build a JSON document from configured paths for RedisJSON-based indexing
///
/// This extracts values from the Automerge document at the specified paths and
/// builds a nested JSON object that preserves the path structure.
///
/// # Examples
///
/// Given paths `["title", "content", "meta.count", "tags"]`:
/// ```json
/// {
///   "title": "Article Title",
///   "content": "Article content...",
///   "meta": {
///     "count": 42
///   },
///   "tags": ["rust", "redis"]
/// }
/// ```
pub fn build_json_document(
    client: &RedisAutomergeClient,
    paths: &[String],
) -> Option<JsonValue> {
    let mut root = Map::new();

    for path in paths {
        // Get typed value at this path
        let typed_value = match client.get_typed_value(path) {
            Ok(Some(val)) => val,
            _ => continue, // Skip missing or error values
        };

        // Split path into segments
        let segments: Vec<&str> = path.split('.').collect();

        // Insert value at the correct nested location
        insert_nested_value(&mut root, &segments, typed_value);
    }

    if root.is_empty() {
        None
    } else {
        Some(JsonValue::Object(root))
    }
}

/// Helper function to insert a typed value into a nested JSON object
fn insert_nested_value(root: &mut Map<String, JsonValue>, segments: &[&str], value: TypedValue) {
    if segments.is_empty() {
        return;
    }

    if segments.len() == 1 {
        // Base case: insert the value
        root.insert(segments[0].to_string(), value.to_json());
    } else {
        // Recursive case: navigate or create nested objects
        let key = segments[0].to_string();
        let remaining = &segments[1..];

        // Get or create the nested object
        let nested = root
            .entry(key.clone())
            .or_insert_with(|| JsonValue::Object(Map::new()));

        // Ensure it's an object
        if let JsonValue::Object(nested_map) = nested {
            insert_nested_value(nested_map, remaining, value);
        } else {
            // If there's a conflict (existing non-object value), replace it
            let mut new_map = Map::new();
            insert_nested_value(&mut new_map, remaining, value);
            root.insert(key, JsonValue::Object(new_map));
        }
    }
}

/// Get the index key for a given Automerge key
pub fn get_index_key(am_key: &str) -> String {
    format!("{}{}", INDEX_KEY_PREFIX, am_key)
}

/// Update the JSON search index for a given Automerge key
///
/// This creates or updates a RedisJSON document with the configured fields.
/// The JSON document preserves the nested structure of paths.
///
/// # Arguments
///
/// * `ctx` - Redis context for making commands
/// * `am_key` - The Automerge document key
/// * `client` - RedisAutomergeClient containing the document
/// * `config` - Index configuration with paths to extract
///
/// # Returns
///
/// Returns `Ok(true)` if index was updated, `Ok(false)` if no fields were indexed
pub fn update_json_index(
    ctx: &Context,
    am_key: &str,
    client: &RedisAutomergeClient,
    config: &IndexConfig,
) -> RedisResult<bool> {
    let index_key = get_index_key(am_key);
    let mut json_doc = match build_json_document(client, &config.paths) {
        Some(JsonValue::Object(map)) => map,
        Some(_) => Map::new(), // shouldn't happen — build_json_document always returns an object
        None => {
            match check_shadow_ownership(ctx, &index_key, am_key, IndexFormat::Json)? {
                ShadowState::Absent => return Ok(false),
                ShadowState::OwnedByUs => {
                    ctx.call("DEL", &[&ctx.create_string(index_key.as_str())])?;
                    return Ok(false);
                }
                ShadowState::Conflict(why) => {
                    return Err(RedisError::String(format!(
                        "refusing to delete shadow key {:?}: {}",
                        index_key, why
                    )));
                }
            }
        }
    };

    // Inject the ownership sentinel before serializing (audit #13).
    json_doc.insert(
        INDEX_SENTINEL_FIELD.to_string(),
        JsonValue::String(am_key.to_string()),
    );
    let json_str = serde_json::to_string(&JsonValue::Object(json_doc))
        .map_err(|e| RedisError::String(format!("Failed to serialize JSON: {}", e)))?;

    // Refuse to overwrite a key we don't own.
    let _ = ensure_safe_to_write(ctx, &index_key, am_key, IndexFormat::Json)?;

    ctx.call(
        "JSON.SET",
        &[
            &ctx.create_string(index_key.as_str()),
            &ctx.create_string("$"),
            &ctx.create_string(json_str.as_str()),
        ],
    )?;

    Ok(true)
}

/// Update the search index for a given Automerge key
///
/// This is the main entry point for index updates. It dispatches to either
/// Hash-based or JSON-based indexing depending on the configured format.
pub fn update_search_index(
    ctx: &Context,
    am_key: &str,
    client: &RedisAutomergeClient,
) -> RedisResult<bool> {
    // Find matching configuration
    let config = match IndexConfig::find_matching_config(ctx, am_key)? {
        Some(cfg) if cfg.enabled => cfg,
        _ => return Ok(false), // No config or disabled
    };

    // Dispatch based on configured format
    match config.format {
        IndexFormat::Json => update_json_index(ctx, am_key, client, &config),
        IndexFormat::Hash => update_hash_index(ctx, am_key, client, &config),
    }
}

/// Update the Hash-based search index for a given Automerge key
fn update_hash_index(
    ctx: &Context,
    am_key: &str,
    client: &RedisAutomergeClient,
    config: &IndexConfig,
) -> RedisResult<bool> {
    let fields = extract_indexed_fields(client, &config.paths);
    let index_key = get_index_key(am_key);

    if fields.is_empty() {
        // No fields to index — drop the shadow if we own one. If the key
        // belongs to someone else (audit #13) refuse rather than DEL it.
        match check_shadow_ownership(ctx, &index_key, am_key, IndexFormat::Hash)? {
            ShadowState::Absent => return Ok(false),
            ShadowState::OwnedByUs => {
                ctx.call("DEL", &[&ctx.create_string(index_key.as_str())])?;
                return Ok(false);
            }
            ShadowState::Conflict(why) => {
                return Err(RedisError::String(format!(
                    "refusing to delete shadow key {:?}: {}",
                    index_key, why
                )));
            }
        }
    }

    let existed = ensure_safe_to_write(ctx, &index_key, am_key, IndexFormat::Hash)?;
    let index_key_rs = ctx.create_string(index_key.clone());

    if existed {
        // We own this shadow — wipe it for clean state.
        ctx.call("DEL", &[&index_key_rs])?;
    }

    // Stamp the sentinel first so an interrupted write still carries the
    // ownership marker.
    ctx.call(
        "HSET",
        &[
            &index_key_rs,
            &ctx.create_string(INDEX_SENTINEL_FIELD),
            &ctx.create_string(am_key),
        ],
    )?;
    for (field, value) in &fields {
        ctx.call(
            "HSET",
            &[
                &index_key_rs,
                &ctx.create_string(field.as_str()),
                &ctx.create_string(value.as_str()),
            ],
        )?;
    }

    Ok(true)
}

/// Delete the search index Hash for a given Automerge key
pub fn delete_search_index(ctx: &Context, am_key: &str) -> RedisResult<()> {
    let index_key = get_index_key(am_key);
    // Try Hash first, then JSON. Either way refuse to clobber unowned keys.
    let state = match check_shadow_ownership(ctx, &index_key, am_key, IndexFormat::Hash)? {
        ShadowState::Conflict(_) => {
            // Could be a JSON shadow we own; recheck under that format.
            check_shadow_ownership(ctx, &index_key, am_key, IndexFormat::Json)?
        }
        s => s,
    };
    match state {
        ShadowState::Absent | ShadowState::OwnedByUs => {
            ctx.call("DEL", &[&ctx.create_string(index_key.as_str())])?;
            Ok(())
        }
        ShadowState::Conflict(why) => Err(RedisError::String(format!(
            "refusing to delete shadow key {:?}: {}",
            index_key, why
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(IndexConfig::matches_pattern("article:123", "article:*"));
        assert!(IndexConfig::matches_pattern("user:abc", "user:*"));
        assert!(!IndexConfig::matches_pattern("post:123", "article:*"));
        assert!(IndexConfig::matches_pattern("anything", "*"));
        assert!(IndexConfig::matches_pattern("test:key:here", "test:*:here"));
        assert!(!IndexConfig::matches_pattern("test:key:there", "test:*:here"));
    }

    #[test]
    fn test_index_key_generation() {
        assert_eq!(get_index_key("article:123"), "am:idx:article:123");
        assert_eq!(get_index_key("user:abc"), "am:idx:user:abc");
    }

    #[test]
    fn test_validate_pattern_accepts_star_and_literals() {
        assert!(validate_pattern("*").is_ok());
        assert!(validate_pattern("user:*").is_ok());
        assert!(validate_pattern("*:tail").is_ok());
        assert!(validate_pattern("p:*:s").is_ok());
        assert!(validate_pattern("article:123").is_ok());
    }

    #[test]
    fn test_validate_pattern_rejects_unsupported_globs() {
        // Audit #25: `?`, `[`, `]`, and `\` are Redis-glob metacharacters
        // that our matcher does not implement, so we reject them at
        // configure-time instead of silently mismatching.
        for bad in ["", "user?", "user[12]:*", "tag\\:*", "user[1-9]:foo"] {
            assert!(
                validate_pattern(bad).is_err(),
                "expected pattern {:?} to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_paths_round_trip_with_commas_and_quotes() {
        // Audit #29: paths were once CSV-joined, which silently split
        // values that contained commas. The current storage format
        // serializes `paths` as a JSON array, so commas, quotes,
        // backslashes, and other JSON-sensitive characters round-trip
        // cleanly through `serialize`/`deserialize`.
        let cfg = IndexConfig::new_with_format(
            "doc:*".to_string(),
            vec![
                "ordinary".to_string(),
                "key,with,commas".to_string(),
                "key\"with\"quotes".to_string(),
                "back\\slash".to_string(),
                "a.dotted.path".to_string(),
                "list[0].field".to_string(),
            ],
            IndexFormat::Hash,
        );
        let s = cfg.serialize();
        let back =
            IndexConfig::deserialize(&cfg.pattern, &s).expect("config must round-trip");
        assert_eq!(back.paths, cfg.paths);
        assert_eq!(back.pattern, cfg.pattern);
        assert_eq!(back.format, cfg.format);
        assert_eq!(back.enabled, cfg.enabled);
    }
}
