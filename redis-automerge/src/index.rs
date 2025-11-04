///! Search indexing support for Automerge documents.
///!
///! This module provides functionality to automatically sync Automerge document fields
///! to Redis Hashes or RedisJSON documents that can be indexed by RediSearch.

use crate::ext::{RedisAutomergeClient, TypedValue};
use redis_module::{Context, RedisError, RedisResult, RedisString, RedisValue};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;

/// Prefix for index configuration keys
const INDEX_CONFIG_PREFIX: &str = "am:index:config:";

/// Prefix for shadow Hash keys
const INDEX_KEY_PREFIX: &str = "am:idx:";

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

    /// Get the Redis key for storing this configuration
    fn config_key(&self) -> String {
        format!("{}{}", INDEX_CONFIG_PREFIX, self.pattern)
    }

    /// Save configuration to Redis
    pub fn save(&self, ctx: &Context) -> RedisResult<()> {
        let key = ctx.create_string(self.config_key());

        // Store as Hash with fields: enabled, paths, format
        ctx.call(
            "HSET",
            &[
                &key,
                &ctx.create_string("enabled"),
                &ctx.create_string(if self.enabled { "1" } else { "0" }),
            ],
        )?;

        let paths_str = self.paths.join(",");
        ctx.call(
            "HSET",
            &[
                &key,
                &ctx.create_string("paths"),
                &ctx.create_string(paths_str),
            ],
        )?;

        ctx.call(
            "HSET",
            &[
                &key,
                &ctx.create_string("format"),
                &ctx.create_string(self.format.as_str()),
            ],
        )?;

        Ok(())
    }

    /// Load configuration from Redis
    pub fn load(ctx: &Context, pattern: &str) -> RedisResult<Option<Self>> {
        let key = ctx.create_string(format!("{}{}", INDEX_CONFIG_PREFIX, pattern));

        // Check if key exists
        let exists_result = ctx.call("EXISTS", &[&key])?;
        let exists: i64 = match exists_result {
            RedisValue::Integer(i) => i,
            _ => return Err(RedisError::Str("Unexpected response from EXISTS")),
        };

        if exists == 0 {
            return Ok(None);
        }

        // Get enabled field
        let enabled_result = ctx.call("HGET", &[&key, &ctx.create_string("enabled")])?;
        let enabled = match enabled_result {
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s == "1",
            _ => true, // Default to enabled
        };

        // Get paths field
        let paths_result = ctx.call("HGET", &[&key, &ctx.create_string("paths")])?;
        let paths = match paths_result {
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => s
                .split(',')
                .map(|p| p.to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            _ => Vec::new(),
        };

        // Get format field (default to Hash for backward compatibility)
        let format_result = ctx.call("HGET", &[&key, &ctx.create_string("format")])?;
        let format = match format_result {
            RedisValue::SimpleString(s) | RedisValue::BulkString(s) => {
                IndexFormat::from_str(&s).unwrap_or(IndexFormat::Hash)
            }
            _ => IndexFormat::Hash, // Default to Hash
        };

        Ok(Some(Self {
            pattern: pattern.to_string(),
            enabled,
            paths,
            format,
        }))
    }

    /// Find the configuration that matches a given key
    pub fn find_matching_config(ctx: &Context, key: &str) -> RedisResult<Option<Self>> {
        // Get all configuration keys
        let pattern = format!("{}*", INDEX_CONFIG_PREFIX);
        let result = ctx.call("KEYS", &[&ctx.create_string(pattern)])?;

        // Handle the Array result
        let config_keys: Vec<RedisString> = match result {
            RedisValue::Array(keys) => keys
                .into_iter()
                .filter_map(|v| match v {
                    RedisValue::BulkString(s) => Some(ctx.create_string(s)),
                    RedisValue::SimpleString(s) => Some(ctx.create_string(s)),
                    _ => None,
                })
                .collect(),
            _ => return Ok(None),
        };

        // Check each configuration to see if its pattern matches the key
        for config_key in config_keys {
            let config_key_str = config_key.to_string();
            if let Some(pattern) = config_key_str.strip_prefix(INDEX_CONFIG_PREFIX) {
                if Self::matches_pattern(key, pattern) {
                    return Self::load(ctx, pattern);
                }
            }
        }

        Ok(None)
    }

    /// Check if a key matches a pattern (supports * wildcard)
    fn matches_pattern(key: &str, pattern: &str) -> bool {
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
