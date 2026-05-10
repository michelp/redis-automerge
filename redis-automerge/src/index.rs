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

    /// Check if a key matches a pattern (supports * wildcard)
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
    // Build JSON document from configured paths
    let json_doc = match build_json_document(client, &config.paths) {
        Some(doc) => doc,
        None => {
            // No fields to index - delete the index if it exists
            let index_key = get_index_key(am_key);
            ctx.call("DEL", &[&ctx.create_string(index_key)])?;
            return Ok(false);
        }
    };

    // Serialize JSON to string
    let json_str = serde_json::to_string(&json_doc)
        .map_err(|e| RedisError::String(format!("Failed to serialize JSON: {}", e)))?;

    // Store as RedisJSON document
    let index_key = get_index_key(am_key);
    ctx.call(
        "JSON.SET",
        &[
            &ctx.create_string(index_key),
            &ctx.create_string("$"),
            &ctx.create_string(json_str),
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
    // Extract configured fields
    let fields = extract_indexed_fields(client, &config.paths);

    if fields.is_empty() {
        // No fields to index - delete the index Hash
        let index_key = get_index_key(am_key);
        ctx.call("DEL", &[&ctx.create_string(index_key)])?;
        return Ok(false);
    }

    // Update Hash with extracted fields
    let index_key = get_index_key(am_key);
    let index_key_rs = ctx.create_string(index_key.clone());

    // Delete existing Hash first to ensure clean state
    ctx.call("DEL", &[&index_key_rs])?;

    // Set each field
    for (field, value) in &fields {
        ctx.call(
            "HSET",
            &[
                &index_key_rs,
                &ctx.create_string(field.clone()),
                &ctx.create_string(value.clone()),
            ],
        )?;
    }

    Ok(true)
}

/// Delete the search index Hash for a given Automerge key
pub fn delete_search_index(ctx: &Context, am_key: &str) -> RedisResult<()> {
    let index_key = get_index_key(am_key);
    ctx.call("DEL", &[&ctx.create_string(index_key)])?;
    Ok(())
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
}
