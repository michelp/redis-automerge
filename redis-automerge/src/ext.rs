//! Extension trait and implementation for integrating Automerge CRDT with Redis.
//!
//! This module provides the core functionality for managing Automerge documents
//! within Redis, including:
//! - JSON-like path operations with support for nested maps and arrays
//! - Type-safe operations for text, integers, doubles, and booleans
//! - List/array manipulation with append operations
//! - Persistence and change tracking for Redis RDB and AOF
//!
//! # Path Syntax
//!
//! The module supports RedisJSON-compatible path syntax:
//! - Simple keys: `"name"`, `"user"`
//! - Nested maps: `"user.profile.name"`, `"data.config.port"`
//! - Array indices: `"users[0]"`, `"items[5].name"`
//! - JSONPath style: `"$.user.name"`, `"$.items[0].title"`
//!
//! # Examples
//!
//! ```rust,no_run
//! use redis_automerge::ext::RedisAutomergeClient;
//!
//! let mut client = RedisAutomergeClient::new();
//!
//! // Set nested values
//! client.put_text("user.name", "Alice").unwrap();
//! client.put_int("user.age", 30).unwrap();
//!
//! // Create and populate a list
//! client.create_list("items").unwrap();
//! client.append_text("items", "first").unwrap();
//! client.append_text("items", "second").unwrap();
//!
//! // Access list elements
//! let value = client.get_text("items[0]").unwrap();
//! assert_eq!(value, Some("first".to_string()));
//! ```

use automerge::{
    marks::{ExpandMark, Mark},
    transaction::Transactable,
    Automerge, AutomergeError, Change, ChangeHash, ObjId, Patch, ReadDoc, ScalarValue, Value, ROOT,
};
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

/// Maximum JSON nesting depth accepted by `from_json` / `AM.FROMJSON`.
/// Defense in depth against stack-overflow DoS via deeply nested input.
/// See SECURITY_AUDIT.md #4.
pub const MAX_JSON_DEPTH: usize = 256;

/// Represents a diff operation parsed from unified diff format
#[derive(Debug, PartialEq)]
enum DiffOp {
    /// Context line (unchanged)
    Context(String),
    /// Line to be deleted
    Delete(String),
    /// Line to be added
    Add(String),
}

/// Error type returned by `put_diff` / `put_diff_with_change`. Wraps the
/// generic `AutomergeError` and adds richly-typed variants for diff
/// application failures so the caller (and ultimately the redis-cli user)
/// sees an actionable message rather than a silent miscompilation.
///
/// Audit #18: previously the diff applier silently swallowed context and
/// delete mismatches with a `// try to be lenient` no-op, producing
/// garbage output when a diff was applied against the wrong base state.
#[derive(Debug)]
pub enum DiffError {
    /// An underlying Automerge operation failed (path parse, get_text,
    /// put_text, etc).
    Automerge(AutomergeError),
    /// A context line in the diff (the unchanged surrounding lines that
    /// disambiguate where the patch applies) did not match the actual
    /// document text at the corresponding position.
    ContextMismatch {
        line_num: usize,
        expected: String,
        actual: String,
    },
    /// A delete line in the diff did not match the actual line at the
    /// corresponding position in the document.
    DeleteMismatch {
        line_num: usize,
        expected: String,
        actual: String,
    },
    /// The diff references more lines than the document contains.
    UnexpectedEof {
        line_num: usize,
        op: &'static str,
    },
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffError::Automerge(e) => write!(f, "{}", e),
            DiffError::ContextMismatch {
                line_num,
                expected,
                actual,
            } => write!(
                f,
                "diff context mismatch at line {}: expected {:?}, found {:?} \
                 (the diff was generated against a different base state — \
                 re-fetch the current text and rebase the diff)",
                line_num, expected, actual
            ),
            DiffError::DeleteMismatch {
                line_num,
                expected,
                actual,
            } => write!(
                f,
                "diff delete mismatch at line {}: expected to delete {:?}, \
                 found {:?} (the diff was generated against a different base \
                 state)",
                line_num, expected, actual
            ),
            DiffError::UnexpectedEof { line_num, op } => write!(
                f,
                "diff {} past end of text at line {} (the document is shorter \
                 than the diff expects)",
                op, line_num
            ),
        }
    }
}

impl std::error::Error for DiffError {}

impl From<AutomergeError> for DiffError {
    fn from(e: AutomergeError) -> Self {
        DiffError::Automerge(e)
    }
}

/// Represents a typed value extracted from an Automerge document
/// This is used for building JSON index documents with proper types
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Text(String),
    Int(i64),
    Double(f64),
    Bool(bool),
    Timestamp(i64),
    Counter(i64),
    Array(Vec<TypedValue>),
    Object(std::collections::HashMap<String, TypedValue>),
    Null,
}

impl TypedValue {
    /// Convert TypedValue to serde_json::Value for JSON serialization
    pub fn to_json(&self) -> JsonValue {
        match self {
            TypedValue::Text(s) => JsonValue::String(s.clone()),
            TypedValue::Int(i) => JsonValue::Number((*i).into()),
            TypedValue::Double(f) => {
                serde_json::Number::from_f64(*f)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
            TypedValue::Bool(b) => JsonValue::Bool(*b),
            TypedValue::Timestamp(ts) => {
                // Convert to ISO 8601 string for JSON
                let dt = DateTime::from_timestamp_millis(*ts)
                    .unwrap_or_else(|| DateTime::<Utc>::UNIX_EPOCH);
                JsonValue::String(dt.to_rfc3339())
            }
            TypedValue::Counter(c) => JsonValue::Number((*c).into()),
            TypedValue::Array(arr) => {
                JsonValue::Array(arr.iter().map(|v| v.to_json()).collect())
            }
            TypedValue::Object(obj) => {
                let map: serde_json::Map<String, JsonValue> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect();
                JsonValue::Object(map)
            }
            TypedValue::Null => JsonValue::Null,
        }
    }
}

/// Parse a unified diff into operations
fn parse_unified_diff(diff: &str) -> Result<Vec<DiffOp>, AutomergeError> {
    let mut ops = Vec::new();

    for line in diff.lines() {
        // Skip header lines
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
            continue;
        }

        if let Some(stripped) = line.strip_prefix('-') {
            ops.push(DiffOp::Delete(stripped.to_string()));
        } else if let Some(stripped) = line.strip_prefix('+') {
            ops.push(DiffOp::Add(stripped.to_string()));
        } else if let Some(stripped) = line.strip_prefix(' ') {
            ops.push(DiffOp::Context(stripped.to_string()));
        } else if !line.is_empty() {
            // Treat lines without prefix as context (for compatibility)
            ops.push(DiffOp::Context(line.to_string()));
        }
    }

    Ok(ops)
}

/// Represents a path segment - either a map key or a list index
#[derive(Debug, PartialEq)]
enum PathSegment {
    Key(String),
    Index(usize),
}

/// Parse a JSON-like path into components.
/// Supports:
/// - "foo.bar" or "$.foo.bar" for map keys
/// - "foo[0]" or "$.foo[0]" for array indices
/// - "foo[0].bar" for mixed paths
///
/// Returns a vector of path segments.
fn parse_path(path: &str) -> Result<Vec<PathSegment>, AutomergeError> {
    let trimmed = path.strip_prefix("$.").unwrap_or(path);
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_bracket = false;
    let mut bracket_content = String::new();

    for ch in trimmed.chars() {
        match ch {
            '.' if !in_bracket => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
            }
            '[' if !in_bracket => {
                if !current.is_empty() {
                    segments.push(PathSegment::Key(current.clone()));
                    current.clear();
                }
                in_bracket = true;
                bracket_content.clear();
            }
            ']' if in_bracket => {
                let index = bracket_content
                    .parse::<usize>()
                    .map_err(|_| AutomergeError::Fail)?;
                segments.push(PathSegment::Index(index));
                in_bracket = false;
                bracket_content.clear();
            }
            _ => {
                if in_bracket {
                    bracket_content.push(ch);
                } else {
                    current.push(ch);
                }
            }
        }
    }

    if in_bracket {
        return Err(AutomergeError::Fail); // Unclosed bracket
    }

    if !current.is_empty() {
        segments.push(PathSegment::Key(current));
    }

    Ok(segments)
}

/// Navigate to a nested object in the document, creating intermediate objects as needed.
/// Returns the ObjId of the target object where the final value should be set.
/// For write operations - does NOT create list elements, only maps.
fn navigate_or_create_path<T: Transactable>(
    tx: &mut T,
    path: &[PathSegment],
) -> Result<ObjId, AutomergeError> {
    let mut current = ROOT;

    for segment in path {
        match segment {
            PathSegment::Key(key) => {
                // Navigate or create map key
                match tx.get(&current, key.as_str())? {
                    Some((Value::Object(_obj_type), obj_id)) => {
                        current = obj_id;
                    }
                    Some(_) => {
                        // Path segment exists but is not an object
                        return Err(AutomergeError::Fail);
                    }
                    None => {
                        // Create a new map at this location
                        current = tx.put_object(&current, key.as_str(), automerge::ObjType::Map)?;
                    }
                }
            }
            PathSegment::Index(idx) => {
                // Navigate to list index (must already exist)
                match tx.get(&current, *idx)? {
                    Some((Value::Object(_obj_type), obj_id)) => {
                        current = obj_id;
                    }
                    Some(_) => {
                        // Element exists but is not an object
                        return Err(AutomergeError::Fail);
                    }
                    None => {
                        // Index out of bounds
                        return Err(AutomergeError::Fail);
                    }
                }
            }
        }
    }

    Ok(current)
}

/// Navigate to a nested object in the document for reading.
/// Returns None if any part of the path doesn't exist.
fn navigate_path_read(
    doc: &Automerge,
    path: &[PathSegment],
) -> Result<Option<ObjId>, AutomergeError> {
    let mut current = ROOT;

    for segment in path {
        match segment {
            PathSegment::Key(key) => match doc.get(&current, key.as_str())? {
                Some((Value::Object(_obj_type), obj_id)) => {
                    current = obj_id;
                }
                Some(_) => return Ok(None),
                None => return Ok(None),
            },
            PathSegment::Index(idx) => match doc.get(&current, *idx)? {
                Some((Value::Object(_obj_type), obj_id)) => {
                    current = obj_id;
                }
                Some(_) => return Ok(None),
                None => return Ok(None),
            },
        }
    }

    Ok(Some(current))
}

/// Helper to get a value from a parent object using a path segment
fn get_value_from_parent<'a, T: ReadDoc>(
    doc: &'a T,
    parent: &ObjId,
    segment: &PathSegment,
) -> Result<Option<(Value<'a>, ObjId)>, AutomergeError> {
    match segment {
        PathSegment::Key(key) => doc.get(parent, key.as_str()),
        PathSegment::Index(idx) => doc.get(parent, *idx),
    }
}

/// Helper to put a value to a parent object using a path segment
fn put_value_to_parent<T: Transactable, V: Into<ScalarValue>>(
    tx: &mut T,
    parent: &ObjId,
    segment: &PathSegment,
    value: V,
) -> Result<(), AutomergeError> {
    match segment {
        PathSegment::Key(key) => {
            tx.put(parent, key.as_str(), value)?;
            Ok(())
        }
        PathSegment::Index(idx) => {
            tx.put(parent, *idx, value)?;
            Ok(())
        }
    }
}

/// Convenience methods for integrating Automerge with Redis persistence layers.
pub trait RedisAutomergeExt {
    /// Load an Automerge document from its persisted binary form.
    ///
    /// This is typically used when restoring a document from Redis' RDB
    /// persistence format.
    fn load(bytes: &[u8]) -> Result<Self, AutomergeError>
    where
        Self: Sized;

    /// Save the current state of the document to a compact binary
    /// representation suitable for RDB persistence.
    fn save(&self) -> Vec<u8>;

    /// Apply a list of changes to the document.
    ///
    /// The raw bytes of the applied changes are recorded internally so that
    /// they can later be emitted as commands for Redis' AOF persistence.
    fn apply(&mut self, changes: Vec<Change>) -> Result<(), AutomergeError>;

    /// Retrieve and clear the buffered AOF commands which represent the
    /// changes previously applied via [`Self::apply`].
    fn commands(&mut self) -> Vec<Vec<u8>>;
}

/// Client for managing an Automerge CRDT document with Redis-specific features.
///
/// This struct wraps an Automerge document and provides:
/// - Path-based access to nested data structures (maps and lists)
/// - Change tracking for AOF persistence
/// - Type-safe operations for common data types
///
/// # Examples
///
/// ```rust,no_run
/// use redis_automerge::ext::RedisAutomergeClient;
///
/// let mut client = RedisAutomergeClient::new();
///
/// // Work with nested maps
/// client.put_text("config.host", "localhost").unwrap();
/// client.put_int("config.port", 6379).unwrap();
///
/// // Work with lists
/// client.create_list("tags").unwrap();
/// client.append_text("tags", "redis").unwrap();
/// client.append_text("tags", "crdt").unwrap();
///
/// // Retrieve values
/// let host = client.get_text("config.host").unwrap();
/// let tag = client.get_text("tags[0]").unwrap();
/// ```
pub struct RedisAutomergeClient {
    doc: Automerge,
    aof: Vec<Vec<u8>>,
}

impl RedisAutomergeClient {
    /// Creates a new client with an empty Automerge document.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let client = RedisAutomergeClient::new();
    /// ```
    pub fn new() -> Self {
        Self {
            doc: Automerge::new(),
            aof: Vec::new(),
        }
    }

    /// Estimate the in-memory footprint of this client in bytes.
    ///
    /// Audit #34: backs the Redis `mem_usage` callback so operators can
    /// monitor per-key Automerge memory consumption via
    /// `MEMORY USAGE <key>`. Automerge 0.9 does not expose a public
    /// memory-accounting API, so this proxy combines three components:
    ///
    /// 1. The struct's own stack-sized fields (`size_of::<Self>()`).
    /// 2. The uncompressed serialized document length
    ///    (`doc.save_nocompress().len()`). This is the closest in-memory
    ///    estimate available from the published `automerge` crate; the
    ///    compressed variant (`save()`) understates the live op-set by
    ///    the LZ4 compression ratio.
    /// 3. The aggregate AOF change-log size: `sum(aof[*].capacity()) +
    ///    aof.capacity() * size_of::<Vec<u8>>()`, accounting for both
    ///    the inner buffers and the outer-Vec spine.
    ///
    /// `MEMORY USAGE` is an operator command issued on demand, so the
    /// cost of serializing the document on each call (which `save_nocompress`
    /// requires) is acceptable. The returned number is an estimate and
    /// will undercount allocator overhead, but it is monotonic with
    /// document growth and useful for catching runaway-growth bugs.
    pub fn estimated_mem_bytes(&self) -> usize {
        let struct_bytes = std::mem::size_of::<Self>();
        let doc_bytes = self.doc.save_nocompress().len();
        let aof_inner_bytes: usize = self.aof.iter().map(|c| c.capacity()).sum();
        let aof_spine_bytes = self.aof.capacity() * std::mem::size_of::<Vec<u8>>();
        struct_bytes
            .saturating_add(doc_bytes)
            .saturating_add(aof_inner_bytes)
            .saturating_add(aof_spine_bytes)
    }

    /// Inserts a text value at the specified path.
    ///
    /// Supports nested paths with automatic intermediate map creation.
    /// Array indices in the path must already exist.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the field (e.g., "name", "user.profile.name", "users[0].name", "$.data.value")
    /// * `value` - Text value to insert
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("user.name", "Alice").unwrap();
    /// client.put_text("$.config.host", "localhost").unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is invalid or empty
    /// - An array index is out of bounds
    /// - A path segment exists but is not an object
    pub fn put_text(&mut self, path: &str, value: &str) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant and discard
        // the returned change bytes. Centralizing the Automerge plumbing
        // in one place keeps the two methods in lockstep instead of
        // drifting via copy-paste edits.
        self.put_text_with_change(path, value).map(|_| ())
    }

    /// Retrieves a text value from the specified path.
    ///
    /// Returns `None` if the path doesn't exist or the value is not text.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the field
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("user.name", "Alice").unwrap();
    ///
    /// let name = client.get_text("user.name").unwrap();
    /// assert_eq!(name, Some("Alice".to_string()));
    ///
    /// let missing = client.get_text("user.email").unwrap();
    /// assert_eq!(missing, None);
    /// ```
    pub fn get_text(&self, path: &str) -> Result<Option<String>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        match get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            // Handle scalar string values
            Some((Value::Scalar(s), _)) => {
                if let ScalarValue::Str(t) = s.as_ref() {
                    return Ok(Some(t.to_string()));
                }
            }
            // Handle Text objects
            Some((Value::Object(automerge::ObjType::Text), obj_id)) => {
                return Ok(Some(self.doc.text(&obj_id)?));
            }
            _ => {}
        }
        Ok(None)
    }

    /// Apply raw Automerge change bytes to this document.
    ///
    /// This allows applying changes generated by one document to another,
    /// enabling real-time synchronization between clients.
    ///
    /// # Arguments
    ///
    /// * `change_bytes` - Raw bytes from an Automerge change
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client1 = RedisAutomergeClient::new();
    /// let change = client1.put_text_with_change("field", "value").unwrap().unwrap();
    ///
    /// let mut client2 = RedisAutomergeClient::new();
    /// client2.apply_change_bytes(&change).unwrap();
    ///
    /// assert_eq!(client2.get_text("field").unwrap(), Some("value".to_string()));
    /// ```
    pub fn apply_change_bytes(&mut self, change_bytes: &[u8]) -> Result<(), AutomergeError> {
        let change = Change::from_bytes(change_bytes.to_vec())?;
        self.doc.apply_changes(vec![change])?;
        Ok(())
    }

    /// Insert a text value and return the raw change bytes.
    ///
    /// Like `put_text()` but returns Automerge change bytes that can
    /// be published to other clients for real-time synchronization.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the field
    /// * `value` - Text value to insert
    ///
    /// # Returns
    ///
    /// - `Some(Vec<u8>)` - Raw change bytes if a change was generated
    /// - `None` - If no change was needed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// let change = client.put_text_with_change("field", "hello").unwrap();
    ///
    /// if let Some(change_bytes) = change {
    ///     // Publish to other clients
    /// }
    /// ```
    pub fn put_text_with_change(
        &mut self,
        path: &str,
        value: &str,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        put_value_to_parent(&mut tx, &parent_obj, &field_name[0], value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Insert an integer value using a path (e.g., "user.age", "users[0].age", or "$.user.age").
    /// Creates intermediate maps as needed. Array indices must already exist.
    pub fn put_int(&mut self, path: &str, value: i64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.put_int_with_change(path, value).map(|_| ())
    }

    /// Retrieve an integer value using a path (e.g., "user.age", "users[0].age", or "$.user.age").
    pub fn get_int(&self, path: &str) -> Result<Option<i64>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        if let Some((Value::Scalar(s), _)) =
            get_value_from_parent(&self.doc, &parent_obj, &field_name[0])?
        {
            if let ScalarValue::Int(i) = s.as_ref() {
                return Ok(Some(*i));
            }
        }
        Ok(None)
    }

    /// Insert an integer value and return the raw change bytes.
    pub fn put_int_with_change(
        &mut self,
        path: &str,
        value: i64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        put_value_to_parent(&mut tx, &parent_obj, &field_name[0], value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Insert a double value using a path (e.g., "metrics.temperature", "temps[0]", or "$.metrics.temperature").
    /// Creates intermediate maps as needed. Array indices must already exist.
    pub fn put_double(&mut self, path: &str, value: f64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.put_double_with_change(path, value).map(|_| ())
    }

    /// Insert a double value and return the raw change bytes.
    pub fn put_double_with_change(
        &mut self,
        path: &str,
        value: f64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        put_value_to_parent(&mut tx, &parent_obj, &field_name[0], value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Retrieve a double value using a path (e.g., "metrics.temperature", "temps[0]", or "$.metrics.temperature").
    pub fn get_double(&self, path: &str) -> Result<Option<f64>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        if let Some((Value::Scalar(s), _)) =
            get_value_from_parent(&self.doc, &parent_obj, &field_name[0])?
        {
            if let ScalarValue::F64(f) = s.as_ref() {
                return Ok(Some(*f));
            }
        }
        Ok(None)
    }

    /// Insert a boolean value using a path (e.g., "flags.active", "flags\[0\]", or "$.flags.active").
    /// Creates intermediate maps as needed. Array indices must already exist.
    pub fn put_bool(&mut self, path: &str, value: bool) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.put_bool_with_change(path, value).map(|_| ())
    }

    /// Retrieve a boolean value using a path (e.g., "flags.active", "flags\[0\]", or "$.flags.active").
    pub fn get_bool(&self, path: &str) -> Result<Option<bool>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        if let Some((Value::Scalar(s), _)) =
            get_value_from_parent(&self.doc, &parent_obj, &field_name[0])?
        {
            if let ScalarValue::Boolean(b) = s.as_ref() {
                return Ok(Some(*b));
            }
        }
        Ok(None)
    }

    /// Insert a boolean value and return the raw change bytes.
    pub fn put_bool_with_change(
        &mut self,
        path: &str,
        value: bool,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        put_value_to_parent(&mut tx, &parent_obj, &field_name[0], value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Insert a counter value using a path (e.g., "stats.views", "counters[0]", or "$.stats.views").
    /// Creates intermediate maps as needed. Array indices must already exist.
    ///
    /// Counters are CRDT values that support increment operations with proper
    /// conflict resolution across distributed systems.
    pub fn put_counter(&mut self, path: &str, value: i64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.put_counter_with_change(path, value).map(|_| ())
    }

    /// Insert a counter value and return the raw change bytes.
    pub fn put_counter_with_change(
        &mut self,
        path: &str,
        value: i64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        // Put counter value
        match &field_name[0] {
            PathSegment::Key(key) => {
                tx.put(&parent_obj, key.as_str(), ScalarValue::Counter(value.into()))?;
            }
            PathSegment::Index(idx) => {
                tx.put(&parent_obj, *idx, ScalarValue::Counter(value.into()))?;
            }
        }

        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Retrieve a counter value using a path (e.g., "stats.views", "counters[0]", or "$.stats.views").
    /// Returns the current counter value as an i64.
    pub fn get_counter(&self, path: &str) -> Result<Option<i64>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        if let Some((Value::Scalar(s), _)) =
            get_value_from_parent(&self.doc, &parent_obj, &field_name[0])?
        {
            if let ScalarValue::Counter(c) = s.as_ref() {
                return Ok(Some(i64::from(c)));
            }
        }
        Ok(None)
    }

    /// Get a value with type information from the specified path.
    ///
    /// This method extracts values preserving their Automerge types, which is
    /// useful for building JSON index documents with proper type representation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the value
    ///
    /// # Returns
    ///
    /// Returns `Some(TypedValue)` if the path exists, `None` otherwise.
    pub fn get_typed_value(&self, path: &str) -> Result<Option<TypedValue>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        match get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            Some((Value::Scalar(s), _)) => {
                let typed_val = match s.as_ref() {
                    ScalarValue::Str(text) => TypedValue::Text(text.to_string()),
                    ScalarValue::Int(i) => TypedValue::Int(*i),
                    ScalarValue::F64(f) => TypedValue::Double(*f),
                    ScalarValue::Boolean(b) => TypedValue::Bool(*b),
                    ScalarValue::Timestamp(ts) => TypedValue::Timestamp(*ts),
                    ScalarValue::Counter(c) => TypedValue::Counter(i64::from(c)),
                    ScalarValue::Null => TypedValue::Null,
                    _ => TypedValue::Null,
                };
                Ok(Some(typed_val))
            }
            Some((Value::Object(obj_type), obj_id)) => {
                // Handle Text objects
                if obj_type == automerge::ObjType::Text {
                    let text = self.doc.text(&obj_id)?;
                    return Ok(Some(TypedValue::Text(text)));
                }

                // Handle List objects - return as Array
                if obj_type == automerge::ObjType::List {
                    let mut arr = Vec::new();
                    let len = self.doc.length(&obj_id);
                    for i in 0..len {
                        if let Some((value, value_obj_id)) = self.doc.get(&obj_id, i)? {
                            if let Some(typed_val) = self.value_to_typed(&value, &value_obj_id)? {
                                arr.push(typed_val);
                            }
                        }
                    }
                    return Ok(Some(TypedValue::Array(arr)));
                }

                // Handle Map objects
                if obj_type == automerge::ObjType::Map {
                    let mut map = std::collections::HashMap::new();
                    for key in self.doc.keys(&obj_id) {
                        if let Some((value, value_obj_id)) = self.doc.get(&obj_id, &key)? {
                            if let Some(typed_val) = self.value_to_typed(&value, &value_obj_id)? {
                                map.insert(key, typed_val);
                            }
                        }
                    }
                    return Ok(Some(TypedValue::Object(map)));
                }

                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Helper method to convert Automerge Value to TypedValue
    fn value_to_typed(
        &self,
        value: &Value,
        obj_id: &ObjId,
    ) -> Result<Option<TypedValue>, AutomergeError> {
        match value {
            Value::Scalar(s) => {
                let typed_val = match s.as_ref() {
                    ScalarValue::Str(text) => TypedValue::Text(text.to_string()),
                    ScalarValue::Int(i) => TypedValue::Int(*i),
                    ScalarValue::F64(f) => TypedValue::Double(*f),
                    ScalarValue::Boolean(b) => TypedValue::Bool(*b),
                    ScalarValue::Timestamp(ts) => TypedValue::Timestamp(*ts),
                    ScalarValue::Counter(c) => TypedValue::Counter(i64::from(c)),
                    ScalarValue::Null => TypedValue::Null,
                    _ => TypedValue::Null,
                };
                Ok(Some(typed_val))
            }
            Value::Object(obj_type) => {
                // Handle Text objects
                if *obj_type == automerge::ObjType::Text {
                    let text = self.doc.text(obj_id)?;
                    return Ok(Some(TypedValue::Text(text)));
                }

                // Handle List objects
                if *obj_type == automerge::ObjType::List {
                    let mut arr = Vec::new();
                    let len = self.doc.length(obj_id);
                    for i in 0..len {
                        if let Some((val, val_obj_id)) = self.doc.get(obj_id, i)? {
                            if let Some(typed_val) = self.value_to_typed(&val, &val_obj_id)? {
                                arr.push(typed_val);
                            }
                        }
                    }
                    return Ok(Some(TypedValue::Array(arr)));
                }

                // Handle Map objects
                if *obj_type == automerge::ObjType::Map {
                    let mut map = std::collections::HashMap::new();
                    for key in self.doc.keys(obj_id) {
                        if let Some((val, val_obj_id)) = self.doc.get(obj_id, &key)? {
                            if let Some(typed_val) = self.value_to_typed(&val, &val_obj_id)? {
                                map.insert(key, typed_val);
                            }
                        }
                    }
                    return Ok(Some(TypedValue::Object(map)));
                }

                Ok(None)
            }
        }
    }

    /// Get all values from a list at the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the list
    ///
    /// # Returns
    ///
    /// Returns `Some(Vec<TypedValue>)` if the path points to a list, `None` otherwise.
    pub fn get_list_values(&self, path: &str) -> Result<Option<Vec<TypedValue>>, AutomergeError> {
        let segments = parse_path(path)?;

        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, &segments)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        // Check if it's a list
        let obj_type = self.doc.object_type(&list_obj)?;
        if obj_type == automerge::ObjType::List {
            let mut values = Vec::new();
            let len = self.doc.length(&list_obj);

            for i in 0..len {
                if let Some((value, value_obj_id)) = self.doc.get(&list_obj, i)? {
                    if let Some(typed_val) = self.value_to_typed(&value, &value_obj_id)? {
                        values.push(typed_val);
                    }
                }
            }

            return Ok(Some(values));
        }

        Ok(None)
    }

    /// Get all keys from a map at the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the map
    ///
    /// # Returns
    ///
    /// Returns `Some(Vec<String>)` if the path points to a map, `None` otherwise.
    pub fn get_map_keys(&self, path: &str) -> Result<Option<Vec<String>>, AutomergeError> {
        let segments = parse_path(path)?;

        let map_obj = if segments.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, &segments)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        // Check if it's a map
        let obj_type = self.doc.object_type(&map_obj)?;
        if obj_type == automerge::ObjType::Map {
            let keys: Vec<String> = self.doc.keys(&map_obj).collect();
            return Ok(Some(keys));
        }

        Ok(None)
    }

    /// Increment a counter at the specified path by the given delta.
    ///
    /// This uses Automerge's CRDT counter increment operation, which properly
    /// merges concurrent increments from different replicas.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the counter field
    /// * `delta` - Amount to increment (can be negative to decrement)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_counter("views", 0).unwrap();
    /// client.inc_counter("views", 1).unwrap();
    /// client.inc_counter("views", 5).unwrap();
    ///
    /// assert_eq!(client.get_counter("views").unwrap(), Some(6));
    /// ```
    pub fn inc_counter(&mut self, path: &str, delta: i64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.inc_counter_with_change(path, delta).map(|_| ())
    }

    /// Increment a counter and return the raw change bytes.
    pub fn inc_counter_with_change(
        &mut self,
        path: &str,
        delta: i64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);

        // Get parent object (don't create it if it doesn't exist for increment)
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Err(AutomergeError::Fail),
            }
        };

        let mut tx = self.doc.transaction();

        // Increment the counter
        match &field_name[0] {
            PathSegment::Key(key) => {
                tx.increment(&parent_obj, key.as_str(), delta)?;
            }
            PathSegment::Index(idx) => {
                tx.increment(&parent_obj, *idx, delta)?;
            }
        }

        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Insert a timestamp value using a path (e.g., "event.created_at", "timestamps[0]", or "$.event.timestamp").
    /// Creates intermediate maps as needed. Array indices must already exist.
    ///
    /// Timestamps are stored as i64 values representing milliseconds since Unix epoch (UTC).
    /// They will be rendered as ISO 8601 UTC datetime strings when exported to JSON.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the field
    /// * `value` - Unix timestamp in milliseconds
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// // Set timestamp to 2024-01-01 00:00:00 UTC (1704067200000 milliseconds)
    /// client.put_timestamp("created_at", 1704067200000).unwrap();
    /// ```
    pub fn put_timestamp(&mut self, path: &str, value: i64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.put_timestamp_with_change(path, value).map(|_| ())
    }

    /// Insert a timestamp value and return the raw change bytes.
    pub fn put_timestamp_with_change(
        &mut self,
        path: &str,
        value: i64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        // Put timestamp value
        match &field_name[0] {
            PathSegment::Key(key) => {
                tx.put(&parent_obj, key.as_str(), ScalarValue::Timestamp(value))?;
            }
            PathSegment::Index(idx) => {
                tx.put(&parent_obj, *idx, ScalarValue::Timestamp(value))?;
            }
        }

        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Retrieve a timestamp value using a path (e.g., "event.created_at", "timestamps[0]", or "$.event.timestamp").
    /// Returns the timestamp as an i64 (milliseconds since Unix epoch).
    pub fn get_timestamp(&self, path: &str) -> Result<Option<i64>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Ok(None);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        if let Some((Value::Scalar(s), _)) =
            get_value_from_parent(&self.doc, &parent_obj, &field_name[0])?
        {
            if let ScalarValue::Timestamp(ts) = s.as_ref() {
                return Ok(Some(*ts));
            }
        }
        Ok(None)
    }

    /// Apply a unified diff to update text value at the specified path.
    ///
    /// This is more efficient than replacing entire text values when only small
    /// portions change. The diff is parsed and applied using Automerge's text
    /// operations (splice_text) to preserve CRDT properties.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text field
    /// * `diff` - Unified diff in git format
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("doc", "Hello World").unwrap();
    ///
    /// let diff = r#"--- a/doc
    /// +++ b/doc
    /// @@ -1 +1 @@
    /// -Hello World
    /// +Hello Rust
    /// "#;
    /// client.put_diff("doc", diff).unwrap();
    ///
    /// assert_eq!(client.get_text("doc").unwrap(), Some("Hello Rust".to_string()));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is invalid or doesn't exist
    /// - The value at path is not text
    /// - The diff cannot be parsed
    /// - The diff cannot be applied to the current text
    pub fn put_diff(&mut self, path: &str, diff: &str) -> Result<(), DiffError> {
        let new_text = self.compute_diff_application(path, diff)?;
        self.put_text(path, &new_text)?;
        Ok(())
    }

    /// Apply a unified diff and return the raw change bytes.
    pub fn put_diff_with_change(
        &mut self,
        path: &str,
        diff: &str,
    ) -> Result<Option<Vec<u8>>, DiffError> {
        let new_text = self.compute_diff_application(path, diff)?;
        Ok(self.put_text_with_change(path, &new_text)?)
    }

    /// Shared diff-application core for [`put_diff`] and
    /// [`put_diff_with_change`] (audit #33 dedup). Returns the resulting text
    /// after strictly applying the diff against the document's current value
    /// at `path`. Audit #18: context-line and delete-line mismatches now
    /// return a `DiffError` with the offending line numbers instead of
    /// silently producing garbage.
    fn compute_diff_application(
        &self,
        path: &str,
        diff: &str,
    ) -> Result<String, DiffError> {
        let segments = parse_path(path)?;
        if segments.is_empty() {
            return Err(DiffError::Automerge(AutomergeError::Fail));
        }

        let current_text = self.get_text(path)?.ok_or(AutomergeError::Fail)?;
        let current_lines: Vec<&str> = current_text.lines().collect();

        let ops = parse_unified_diff(diff)?;

        let mut new_lines = Vec::new();
        let mut idx = 0;
        for op in &ops {
            match op {
                DiffOp::Context(line) => {
                    if idx >= current_lines.len() {
                        return Err(DiffError::UnexpectedEof {
                            line_num: idx + 1,
                            op: "context",
                        });
                    }
                    let actual = current_lines[idx];
                    if actual != line.as_str() {
                        return Err(DiffError::ContextMismatch {
                            line_num: idx + 1,
                            expected: line.clone(),
                            actual: actual.to_string(),
                        });
                    }
                    new_lines.push(actual.to_string());
                    idx += 1;
                }
                DiffOp::Delete(line) => {
                    if idx >= current_lines.len() {
                        return Err(DiffError::UnexpectedEof {
                            line_num: idx + 1,
                            op: "delete",
                        });
                    }
                    let actual = current_lines[idx];
                    if actual != line.as_str() {
                        return Err(DiffError::DeleteMismatch {
                            line_num: idx + 1,
                            expected: line.clone(),
                            actual: actual.to_string(),
                        });
                    }
                    idx += 1;
                }
                DiffOp::Add(line) => {
                    new_lines.push(line.clone());
                }
            }
        }

        // Append any trailing lines beyond the diff's last hunk verbatim.
        while idx < current_lines.len() {
            new_lines.push(current_lines[idx].to_string());
            idx += 1;
        }

        // Preserve the trailing newline iff the original text had one.
        Ok(if current_text.ends_with('\n') {
            new_lines.join("\n") + "\n"
        } else {
            new_lines.join("\n")
        })
    }

    /// Creates a new empty list at the specified path.
    ///
    /// Creates intermediate maps as needed. The final segment must be a map key.
    ///
    /// # Arguments
    ///
    /// * `path` - Path where the list should be created
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.create_list("users").unwrap();
    /// client.create_list("data.items").unwrap();
    ///
    /// assert_eq!(client.list_len("users").unwrap(), Some(0));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty or the final segment is an array index.
    pub fn create_list(&mut self, path: &str) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.create_list_with_change(path).map(|_| ())
    }

    /// Create a new empty list and return the raw change bytes.
    pub fn create_list_with_change(
        &mut self,
        path: &str,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        match &field_name[0] {
            PathSegment::Key(key) => {
                tx.put_object(&parent_obj, key.as_str(), automerge::ObjType::List)?;
            }
            PathSegment::Index(_) => {
                return Err(AutomergeError::Fail); // Cannot create list at index
            }
        }

        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Appends a text value to the end of a list at the specified path.
    ///
    /// The list must already exist at the given path.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the list
    /// * `value` - Text value to append
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.create_list("users").unwrap();
    /// client.append_text("users", "Alice").unwrap();
    /// client.append_text("users", "Bob").unwrap();
    ///
    /// assert_eq!(client.get_text("users[0]").unwrap(), Some("Alice".to_string()));
    /// assert_eq!(client.list_len("users").unwrap(), Some(2));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't exist or doesn't point to a list.
    pub fn append_text(&mut self, path: &str, value: &str) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.append_text_with_change(path, value).map(|_| ())
    }

    /// Append a text value to a list and return the raw change bytes.
    pub fn append_text_with_change(
        &mut self,
        path: &str,
        value: &str,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        // Navigate before creating transaction
        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            navigate_path_read(&self.doc, &segments)?.ok_or(AutomergeError::Fail)?
        };

        let list_len = self.doc.length(&list_obj);
        let mut tx = self.doc.transaction();
        tx.insert(&list_obj, list_len, value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Append an integer value to a list at the specified path.
    pub fn append_int(&mut self, path: &str, value: i64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.append_int_with_change(path, value).map(|_| ())
    }

    /// Append an integer value to a list and return the raw change bytes.
    pub fn append_int_with_change(
        &mut self,
        path: &str,
        value: i64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        // Navigate before creating transaction
        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            navigate_path_read(&self.doc, &segments)?.ok_or(AutomergeError::Fail)?
        };

        let list_len = self.doc.length(&list_obj);
        let mut tx = self.doc.transaction();
        tx.insert(&list_obj, list_len, value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Append a double value to a list at the specified path.
    pub fn append_double(&mut self, path: &str, value: f64) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.append_double_with_change(path, value).map(|_| ())
    }

    /// Append a double value to a list and return the raw change bytes.
    pub fn append_double_with_change(
        &mut self,
        path: &str,
        value: f64,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        // Navigate before creating transaction
        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            navigate_path_read(&self.doc, &segments)?.ok_or(AutomergeError::Fail)?
        };

        let list_len = self.doc.length(&list_obj);
        let mut tx = self.doc.transaction();
        tx.insert(&list_obj, list_len, value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Append a boolean value to a list at the specified path.
    pub fn append_bool(&mut self, path: &str, value: bool) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.append_bool_with_change(path, value).map(|_| ())
    }

    /// Append a boolean value to a list and return the raw change bytes.
    pub fn append_bool_with_change(
        &mut self,
        path: &str,
        value: bool,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        // Navigate before creating transaction
        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            navigate_path_read(&self.doc, &segments)?.ok_or(AutomergeError::Fail)?
        };

        let list_len = self.doc.length(&list_obj);
        let mut tx = self.doc.transaction();
        tx.insert(&list_obj, list_len, value)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Returns the length of a list at the specified path.
    ///
    /// Returns `None` if the path doesn't exist or doesn't point to a list.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the list
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.create_list("items").unwrap();
    /// client.append_text("items", "first").unwrap();
    /// client.append_text("items", "second").unwrap();
    ///
    /// assert_eq!(client.list_len("items").unwrap(), Some(2));
    /// assert_eq!(client.list_len("missing").unwrap(), None);
    /// ```
    pub fn list_len(&self, path: &str) -> Result<Option<usize>, AutomergeError> {
        let segments = parse_path(path)?;

        let list_obj = if segments.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, &segments)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        Ok(Some(self.doc.length(&list_obj)))
    }

    /// Returns the number of keys in a map at the specified path.
    ///
    /// Returns `None` if the path doesn't exist or doesn't point to a map.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the map
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("name", "Alice").unwrap();
    /// client.put_text("age", "30").unwrap();
    ///
    /// assert_eq!(client.map_len("").unwrap(), Some(2));
    /// assert_eq!(client.map_len("missing").unwrap(), None);
    /// ```
    pub fn map_len(&self, path: &str) -> Result<Option<usize>, AutomergeError> {
        let segments = parse_path(path)?;

        let map_obj = if segments.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, &segments)? {
                Some(obj) => obj,
                None => return Ok(None),
            }
        };

        Ok(Some(self.doc.keys(&map_obj).count()))
    }

    /// Get changes from the document that are not in the provided have_deps list.
    ///
    /// This exposes the Automerge `get_changes` API, which returns all changes
    /// that are not in the provided list of change hashes. If an empty list is
    /// provided, all changes in the document are returned.
    ///
    /// # Arguments
    ///
    /// * `have_deps` - Slice of ChangeHash values representing changes already known
    ///
    /// # Returns
    ///
    /// A vector of Change references for changes not in have_deps
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("field", "value").unwrap();
    ///
    /// // Get all changes
    /// let all_changes = client.get_changes(&[]);
    /// assert_eq!(all_changes.len(), 1);
    ///
    /// // Get changes we don't have
    /// let hash = all_changes[0].hash();
    /// let new_changes = client.get_changes(&[hash]);
    /// assert_eq!(new_changes.len(), 0);
    /// ```
    pub fn get_changes(&self, have_deps: &[ChangeHash]) -> Vec<Change> {
        self.doc.get_changes(have_deps)
    }

    /// Get the diff between two document states.
    ///
    /// This uses Automerge's `diff` function to compare two document states identified by
    /// their change hashes (heads). It returns a vector of patches describing what changed
    /// between the two states.
    ///
    /// # Arguments
    ///
    /// * `before_heads` - Change hashes representing the "before" state
    /// * `after_heads` - Change hashes representing the "after" state
    ///
    /// # Returns
    ///
    /// A vector of `Patch` objects describing the differences. Each patch indicates:
    /// - The path to the changed object
    /// - The type of change (PutMap, PutSeq, DeleteMap, DeleteSeq, Insert, Increment, etc.)
    /// - The old and new values
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("name", "Alice").unwrap();
    /// let before_heads = client.get_changes(&[]).iter().map(|c| c.hash()).collect::<Vec<_>>();
    ///
    /// client.put_text("name", "Bob").unwrap();
    /// let after_heads = client.get_changes(&[]).iter().map(|c| c.hash()).collect::<Vec<_>>();
    ///
    /// let patches = client.get_diff(&before_heads, &after_heads);
    /// // patches will show name changing from "Alice" to "Bob"
    /// ```
    pub fn get_diff(&self, before_heads: &[ChangeHash], after_heads: &[ChangeHash]) -> Vec<Patch> {
        self.doc.diff(before_heads, after_heads)
    }

    /// Resolve `path` to a Text object, converting an existing string
    /// scalar into a Text object in-place if needed.
    ///
    /// Splice and mark operations require an Automerge `Text` object to
    /// operate on, but the same field may already hold a plain string
    /// scalar (the natural result of an earlier `put_text` that did not
    /// need CRDT-aware text semantics). When that happens this helper
    /// commits a one-off transaction that replaces the scalar with a
    /// `Text` object seeded with the prior contents, then returns the
    /// new object's id.
    ///
    /// Audit #32: this body used to be copy-pasted across six call
    /// sites (`splice_text`, `splice_text_with_change`, `create_mark`,
    /// `create_mark_with_change`, `clear_mark`, `clear_mark_with_change`).
    /// Extracting it here keeps those six in lockstep.
    fn ensure_text_object_at(&mut self, path: &str) -> Result<ObjId, AutomergeError> {
        let segments = parse_path(path)?;
        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }
        let (parent_path, field_name) = segments.split_at(segments.len() - 1);

        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Err(AutomergeError::Fail),
            }
        };

        match get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            Some((Value::Object(automerge::ObjType::Text), obj_id)) => Ok(obj_id),
            Some((Value::Scalar(s), _)) => {
                if let ScalarValue::Str(existing_text) = s.as_ref() {
                    let existing_text_owned = existing_text.to_string();
                    let mut tx = self.doc.transaction();
                    let parent_for_put = navigate_or_create_path(&mut tx, parent_path)?;
                    let text_obj = match &field_name[0] {
                        PathSegment::Key(key) => tx.put_object(
                            &parent_for_put,
                            key.as_str(),
                            automerge::ObjType::Text,
                        )?,
                        PathSegment::Index(idx) => {
                            tx.put_object(&parent_for_put, *idx, automerge::ObjType::Text)?
                        }
                    };
                    tx.splice_text(&text_obj, 0, 0, &existing_text_owned)?;
                    let (_hash, _patch) = tx.commit();
                    Ok(text_obj)
                } else {
                    Err(AutomergeError::Fail)
                }
            }
            _ => Err(AutomergeError::Fail),
        }
    }

    /// Splice text at the specified path.
    ///
    /// This performs an in-place text splice operation using Automerge's `splice_text` method,
    /// which is more efficient than replacing the entire text value. The splice operation
    /// deletes `del` characters starting at position `pos` and inserts `text` at that position.
    ///
    /// If the field contains a string scalar, it will be converted to a Text object first.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text field
    /// * `pos` - Character position where the splice begins (0-indexed)
    /// * `del` - Number of characters to delete (can be negative to delete backwards)
    /// * `text` - Text to insert at the position
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("greeting", "Hello World").unwrap();
    ///
    /// // Replace "World" with "Rust" - delete 5 chars at position 6 and insert "Rust"
    /// client.splice_text("greeting", 6, 5, "Rust").unwrap();
    ///
    /// assert_eq!(client.get_text("greeting").unwrap(), Some("Hello Rust".to_string()));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is invalid or doesn't exist
    /// - The value at path is not text
    /// - The position or deletion count is invalid
    pub fn splice_text(
        &mut self,
        path: &str,
        pos: usize,
        del: isize,
        text: &str,
    ) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.splice_text_with_change(path, pos, del, text).map(|_| ())
    }

    /// Splice text and return the raw change bytes.
    ///
    /// Like `splice_text()` but returns Automerge change bytes that can
    /// be published to other clients for real-time synchronization.
    ///
    /// If the field contains a string scalar, it will be converted to a Text object first.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text field
    /// * `pos` - Character position where the splice begins (0-indexed)
    /// * `del` - Number of characters to delete (can be negative to delete backwards)
    /// * `text` - Text to insert at the position
    ///
    /// # Returns
    ///
    /// - `Some(Vec<u8>)` - Raw change bytes if a change was generated
    /// - `None` - If no change was needed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("doc", "Hello World").unwrap();
    ///
    /// let change = client.splice_text_with_change("doc", 6, 5, "Rust").unwrap();
    ///
    /// if let Some(change_bytes) = change {
    ///     // Publish to other clients
    /// }
    /// ```
    pub fn splice_text_with_change(
        &mut self,
        path: &str,
        pos: usize,
        del: isize,
        text: &str,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        // Audit #32: text-object resolution + scalar->Text conversion
        // lives in `ensure_text_object_at` so this body stays focused
        // on the splice itself.
        let text_obj = self.ensure_text_object_at(path)?;

        let mut tx = self.doc.transaction();
        tx.splice_text(&text_obj, pos, del, text)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Convert the entire Automerge document to JSON.
    ///
    /// Recursively traverses the document starting from ROOT and converts all
    /// values to JSON format. Supports both compact and pretty-printed output.
    ///
    /// # Arguments
    ///
    /// * `pretty` - If true, output formatted JSON with indentation. If false, compact JSON.
    ///
    /// # Returns
    ///
    /// A JSON string representation of the document.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("name", "Alice").unwrap();
    /// client.put_int("age", 30).unwrap();
    ///
    /// // Compact JSON
    /// let json = client.to_json(false).unwrap();
    /// // Returns: {"name":"Alice","age":30}
    ///
    /// // Pretty JSON
    /// let json = client.to_json(true).unwrap();
    /// // Returns:
    /// // {
    /// //   "name": "Alice",
    /// //   "age": 30
    /// // }
    /// ```
    pub fn to_json(&self, pretty: bool) -> Result<String, AutomergeError> {
        use serde_json::{Map, Value as JsonValue};

        // Helper function to recursively convert an Automerge object to JSON
        fn obj_to_json(doc: &Automerge, obj_id: &ObjId) -> Result<JsonValue, AutomergeError> {
            // Check the object type
            let obj_type = doc.object_type(obj_id)?;

            match obj_type {
                automerge::ObjType::Map => {
                    let mut map = Map::new();
                    // Iterate over all keys in the map
                    for key in doc.keys(obj_id) {
                        if let Some((value, value_obj_id)) = doc.get(obj_id, &key)? {
                            let json_value = value_to_json(doc, &value, &value_obj_id)?;
                            map.insert(key.to_string(), json_value);
                        }
                    }
                    Ok(JsonValue::Object(map))
                }
                automerge::ObjType::List => {
                    let mut arr = Vec::new();
                    let len = doc.length(obj_id);
                    for i in 0..len {
                        if let Some((value, value_obj_id)) = doc.get(obj_id, i)? {
                            let json_value = value_to_json(doc, &value, &value_obj_id)?;
                            arr.push(json_value);
                        }
                    }
                    Ok(JsonValue::Array(arr))
                }
                automerge::ObjType::Text => {
                    // Text objects are converted to strings
                    let text = doc.text(obj_id)?;
                    Ok(JsonValue::String(text))
                }
                _ => {
                    // Unknown object type, treat as null
                    Ok(JsonValue::Null)
                }
            }
        }

        // Helper function to convert an Automerge value to JSON
        fn value_to_json(
            doc: &Automerge,
            value: &Value,
            obj_id: &ObjId,
        ) -> Result<JsonValue, AutomergeError> {
            match value {
                Value::Object(_) => {
                    // Recursively convert nested objects
                    obj_to_json(doc, obj_id)
                }
                Value::Scalar(scalar) => {
                    let s = scalar.as_ref();
                    match s {
                        ScalarValue::Str(s) => Ok(JsonValue::String(s.to_string())),
                        ScalarValue::Int(i) => Ok(JsonValue::Number((*i).into())),
                        ScalarValue::F64(f) => {
                            if let Some(num) = serde_json::Number::from_f64(*f) {
                                Ok(JsonValue::Number(num))
                            } else {
                                Ok(JsonValue::Null)
                            }
                        }
                        ScalarValue::Counter(c) => Ok(JsonValue::Number(i64::from(c).into())),
                        ScalarValue::Timestamp(ts) => {
                            // Convert Unix timestamp (milliseconds) to ISO 8601 string
                            let dt = DateTime::from_timestamp_millis(*ts)
                                .unwrap_or_else(|| DateTime::<Utc>::UNIX_EPOCH);
                            Ok(JsonValue::String(dt.to_rfc3339()))
                        }
                        ScalarValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
                        ScalarValue::Null => Ok(JsonValue::Null),
                        _ => Ok(JsonValue::Null),
                    }
                }
            }
        }

        // Start conversion from ROOT
        let json_value = obj_to_json(&self.doc, &ROOT)?;

        // Serialize to string
        if pretty {
            serde_json::to_string_pretty(&json_value).map_err(|_| AutomergeError::Fail)
        } else {
            serde_json::to_string(&json_value).map_err(|_| AutomergeError::Fail)
        }
    }

    /// Create a new Automerge document from a JSON string.
    ///
    /// Parses the JSON string and recursively converts it to Automerge document structure:
    /// - JSON objects become Automerge Maps
    /// - JSON arrays become Automerge Lists
    /// - JSON strings become text values
    /// - JSON numbers become integers (if no decimal) or doubles
    /// - JSON booleans become boolean values
    /// - JSON null becomes null
    ///
    /// This replaces the entire document with the structure from the JSON.
    ///
    /// # Arguments
    ///
    /// * `json` - JSON string to parse and convert
    ///
    /// # Returns
    ///
    /// A new `RedisAutomergeClient` with the document initialized from JSON.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    ///
    /// let json = r#"{"name":"Alice","age":30,"active":true}"#;
    /// let client = RedisAutomergeClient::from_json(json).unwrap();
    ///
    /// assert_eq!(client.get_text("name").unwrap(), Some("Alice".to_string()));
    /// assert_eq!(client.get_int("age").unwrap(), Some(30));
    /// assert_eq!(client.get_bool("active").unwrap(), Some(true));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON string cannot be parsed or converted to Automerge format.
    pub fn from_json(json: &str) -> Result<Self, AutomergeError> {
        use serde_json::Value as JsonValue;

        // Parse JSON string
        let json_value: JsonValue = serde_json::from_str(json).map_err(|_| AutomergeError::Fail)?;

        // Create new document
        let mut client = Self::new();
        let mut tx = client.doc.transaction();

        // Helper function to recursively populate an Automerge object from JSON
        fn populate_from_json<T: Transactable>(
            tx: &mut T,
            parent: &ObjId,
            key_or_index: KeyOrIndex,
            value: &JsonValue,
            depth: usize,
        ) -> Result<(), AutomergeError> {
            // Defense in depth against stack overflow from deeply nested JSON.
            // serde_json caps parse-time recursion at 128 by default, but we
            // enforce our own limit on the walker so future serde_json
            // configuration changes cannot reach unbounded recursion here.
            if depth > MAX_JSON_DEPTH {
                return Err(AutomergeError::Fail);
            }
            match value {
                JsonValue::Object(map) => {
                    // Create a Map object
                    let obj_id = match key_or_index {
                        KeyOrIndex::Key(key) => {
                            tx.put_object(parent, key.as_str(), automerge::ObjType::Map)?
                        }
                        KeyOrIndex::Index(idx) => {
                            tx.put_object(parent, idx, automerge::ObjType::Map)?
                        }
                    };
                    // Recursively populate the map
                    for (k, v) in map {
                        populate_from_json(tx, &obj_id, KeyOrIndex::Key(k.clone()), v, depth + 1)?;
                    }
                }
                JsonValue::Array(arr) => {
                    // Create a List object
                    let obj_id = match key_or_index {
                        KeyOrIndex::Key(key) => {
                            tx.put_object(parent, key.as_str(), automerge::ObjType::List)?
                        }
                        KeyOrIndex::Index(idx) => {
                            tx.put_object(parent, idx, automerge::ObjType::List)?
                        }
                    };
                    // Append elements to the list
                    for (i, v) in arr.iter().enumerate() {
                        populate_from_json(tx, &obj_id, KeyOrIndex::Index(i), v, depth + 1)?;
                    }
                }
                JsonValue::String(s) => {
                    // Insert as text value
                    match key_or_index {
                        KeyOrIndex::Key(key) => {
                            tx.put(parent, key.as_str(), s.as_str())?;
                        }
                        KeyOrIndex::Index(idx) => {
                            tx.insert(parent, idx, s.as_str())?;
                        }
                    }
                }
                JsonValue::Number(n) => {
                    // Convert to int or double
                    match key_or_index {
                        KeyOrIndex::Key(key) => {
                            if let Some(i) = n.as_i64() {
                                tx.put(parent, key.as_str(), i)?;
                            } else if let Some(f) = n.as_f64() {
                                tx.put(parent, key.as_str(), f)?;
                            }
                        }
                        KeyOrIndex::Index(idx) => {
                            if let Some(i) = n.as_i64() {
                                tx.insert(parent, idx, i)?;
                            } else if let Some(f) = n.as_f64() {
                                tx.insert(parent, idx, f)?;
                            }
                        }
                    }
                }
                JsonValue::Bool(b) => {
                    // Insert as boolean
                    match key_or_index {
                        KeyOrIndex::Key(key) => {
                            tx.put(parent, key.as_str(), *b)?;
                        }
                        KeyOrIndex::Index(idx) => {
                            tx.insert(parent, idx, *b)?;
                        }
                    }
                }
                JsonValue::Null => {
                    // Insert as null
                    match key_or_index {
                        KeyOrIndex::Key(key) => {
                            tx.put(parent, key.as_str(), ScalarValue::Null)?;
                        }
                        KeyOrIndex::Index(idx) => {
                            tx.insert(parent, idx, ScalarValue::Null)?;
                        }
                    }
                }
            }
            Ok(())
        }

        // Helper enum to handle both keys and indices
        enum KeyOrIndex {
            Key(String),
            Index(usize),
        }

        // Start populating from root
        if let JsonValue::Object(map) = &json_value {
            for (k, v) in map {
                populate_from_json(&mut tx, &ROOT, KeyOrIndex::Key(k.clone()), v, 1)?;
            }
        } else {
            // If root is not an object, we can't convert it directly
            return Err(AutomergeError::Fail);
        }

        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = client.doc.get_change_by_hash(&h) {
                client.aof.push(change.raw_bytes().to_vec());
            }
        }

        Ok(client)
    }

    /// Create a mark on a text object at the specified path.
    ///
    /// Marks allow attaching metadata to ranges of text, useful for rich text formatting
    /// (bold, italic, comments, etc.). Only one mark of the same name can affect a position.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text object
    /// * `name` - Name of the mark (e.g., "bold", "comment")
    /// * `value` - Scalar value for the mark
    /// * `start` - Start position (0-indexed)
    /// * `end` - End position (exclusive)
    /// * `expand` - How the mark expands when text is inserted at boundaries
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    /// use automerge::marks::ExpandMark;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("doc", "Hello World").unwrap();
    ///
    /// // Mark "World" as bold
    /// client.create_mark("doc", "bold", true.into(), 6, 11, ExpandMark::None).unwrap();
    /// ```
    pub fn create_mark(
        &mut self,
        path: &str,
        name: &str,
        value: ScalarValue,
        start: usize,
        end: usize,
        expand: ExpandMark,
    ) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.create_mark_with_change(path, name, value, start, end, expand)
            .map(|_| ())
    }

    /// Create a mark on a text object and return the raw change bytes.
    pub fn create_mark_with_change(
        &mut self,
        path: &str,
        name: &str,
        value: ScalarValue,
        start: usize,
        end: usize,
        expand: ExpandMark,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        // Audit #32: text-object resolution + scalar->Text conversion
        // lives in `ensure_text_object_at`.
        let text_obj = self.ensure_text_object_at(path)?;

        let mut tx = self.doc.transaction();
        let mark = Mark::new(name.to_string(), value, start, end);
        tx.mark(&text_obj, mark, expand)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Remove a mark from a text object at the specified path.
    ///
    /// Removes a mark with the given name from the specified range of text.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text object
    /// * `name` - Name of the mark to remove
    /// * `start` - Start position (0-indexed)
    /// * `end` - End position (exclusive)
    /// * `expand` - How the mark expands when text is inserted at boundaries
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    /// use automerge::marks::ExpandMark;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("doc", "Hello World").unwrap();
    /// client.create_mark("doc", "bold", true.into(), 6, 11, ExpandMark::None).unwrap();
    ///
    /// // Remove the bold mark
    /// client.clear_mark("doc", "bold", 6, 11, ExpandMark::None).unwrap();
    /// ```
    pub fn clear_mark(
        &mut self,
        path: &str,
        name: &str,
        start: usize,
        end: usize,
        expand: ExpandMark,
    ) -> Result<(), AutomergeError> {
        // Audit #32: delegate to the `_with_change` variant.
        self.clear_mark_with_change(path, name, start, end, expand)
            .map(|_| ())
    }

    /// Remove a mark from a text object and return the raw change bytes.
    pub fn clear_mark_with_change(
        &mut self,
        path: &str,
        name: &str,
        start: usize,
        end: usize,
        expand: ExpandMark,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        // Audit #32: text-object resolution + scalar->Text conversion
        // lives in `ensure_text_object_at`.
        let text_obj = self.ensure_text_object_at(path)?;

        let mut tx = self.doc.transaction();
        tx.unmark(&text_obj, name, start, end, expand)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                let change_bytes = change.raw_bytes().to_vec();
                self.aof.push(change_bytes.clone());
                return Ok(Some(change_bytes));
            }
        }

        Ok(None)
    }

    /// Get all marks on a text object at the specified path.
    ///
    /// Returns a vector of marks containing their name, value, start, and end positions.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the text object
    ///
    /// # Returns
    ///
    /// A vector of tuples `(name, value, start, end)` for each mark.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use redis_automerge::ext::RedisAutomergeClient;
    /// use automerge::marks::ExpandMark;
    ///
    /// let mut client = RedisAutomergeClient::new();
    /// client.put_text("doc", "Hello World").unwrap();
    /// client.create_mark("doc", "bold", true.into(), 6, 11, ExpandMark::None).unwrap();
    ///
    /// let marks = client.get_marks("doc").unwrap();
    /// // Returns: vec![("bold", ScalarValue::Boolean(true), 6, 11)]
    /// ```
    pub fn get_marks(
        &self,
        path: &str,
    ) -> Result<Vec<(String, ScalarValue, usize, usize)>, AutomergeError> {
        let segments = parse_path(path)?;

        let text_obj = if segments.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, &segments)? {
                Some(obj) => obj,
                None => return Ok(Vec::new()),
            }
        };

        let marks = self.doc.marks(&text_obj)?;
        let result = marks
            .into_iter()
            .map(|m| {
                (
                    m.name().to_string(),
                    m.value().clone(),
                    m.start,
                    m.end,
                )
            })
            .collect();
        Ok(result)
    }
}

impl Default for RedisAutomergeClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RedisAutomergeExt for RedisAutomergeClient {
    fn load(bytes: &[u8]) -> Result<Self, AutomergeError> {
        let doc = Automerge::load(bytes)?;
        Ok(Self {
            doc,
            aof: Vec::new(),
        })
    }

    fn save(&self) -> Vec<u8> {
        self.doc.save()
    }

    fn apply(&mut self, changes: Vec<Change>) -> Result<(), AutomergeError> {
        for change in &changes {
            self.aof.push(change.raw_bytes().to_vec());
        }
        self.doc.apply_changes(changes)?;
        Ok(())
    }

    fn commands(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.aof)
    }
}
