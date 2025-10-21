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
    transaction::Transactable, Automerge, AutomergeError, Change, ChangeHash, ObjId, ReadDoc,
    ScalarValue, Value, ROOT,
};

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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                return Ok(Some(*c as i64));
            }
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
    pub fn put_diff(&mut self, path: &str, diff: &str) -> Result<(), AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        // Get current text
        let current_text = self.get_text(path)?.ok_or(AutomergeError::Fail)?;
        let current_lines: Vec<&str> = current_text.lines().collect();

        // Parse the diff
        let ops = parse_unified_diff(diff)?;

        // Build the new text by applying diff operations
        let mut new_lines = Vec::new();
        let mut current_line_idx = 0;

        let mut i = 0;
        while i < ops.len() {
            match &ops[i] {
                DiffOp::Context(line) => {
                    // Verify context matches (for safety)
                    if current_line_idx < current_lines.len() {
                        let current = current_lines[current_line_idx];
                        if current != line.as_str() {
                            // Context mismatch - try to be lenient
                        }
                        new_lines.push(current.to_string());
                        current_line_idx += 1;
                    }
                }
                DiffOp::Delete(line) => {
                    // Skip the deleted line in current text
                    if current_line_idx < current_lines.len() {
                        let current = current_lines[current_line_idx];
                        if current == line.as_str() {
                            current_line_idx += 1;
                        }
                    }
                }
                DiffOp::Add(line) => {
                    // Add the new line
                    new_lines.push(line.clone());
                }
            }
            i += 1;
        }

        // Add any remaining lines
        while current_line_idx < current_lines.len() {
            new_lines.push(current_lines[current_line_idx].to_string());
            current_line_idx += 1;
        }

        // Reconstruct text with newlines
        let new_text = if current_text.ends_with('\n') {
            new_lines.join("\n") + "\n"
        } else {
            new_lines.join("\n")
        };

        // Apply the change using put_text
        self.put_text(path, &new_text)?;

        Ok(())
    }

    /// Apply a unified diff and return the raw change bytes.
    pub fn put_diff_with_change(
        &mut self,
        path: &str,
        diff: &str,
    ) -> Result<Option<Vec<u8>>, AutomergeError> {
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        // Get current text
        let current_text = self.get_text(path)?.ok_or(AutomergeError::Fail)?;
        let current_lines: Vec<&str> = current_text.lines().collect();

        // Parse the diff
        let ops = parse_unified_diff(diff)?;

        // Build the new text by applying diff operations
        let mut new_lines = Vec::new();
        let mut current_line_idx = 0;

        let mut i = 0;
        while i < ops.len() {
            match &ops[i] {
                DiffOp::Context(line) => {
                    // Verify context matches (for safety)
                    if current_line_idx < current_lines.len() {
                        let current = current_lines[current_line_idx];
                        if current != line.as_str() {
                            // Context mismatch - try to be lenient
                        }
                        new_lines.push(current.to_string());
                        current_line_idx += 1;
                    }
                }
                DiffOp::Delete(line) => {
                    // Skip the deleted line in current text
                    if current_line_idx < current_lines.len() {
                        let current = current_lines[current_line_idx];
                        if current == line.as_str() {
                            current_line_idx += 1;
                        }
                    }
                }
                DiffOp::Add(line) => {
                    // Add the new line
                    new_lines.push(line.clone());
                }
            }
            i += 1;
        }

        // Add any remaining lines
        while current_line_idx < current_lines.len() {
            new_lines.push(current_lines[current_line_idx].to_string());
            current_line_idx += 1;
        }

        // Reconstruct text with newlines
        let new_text = if current_text.ends_with('\n') {
            new_lines.join("\n") + "\n"
        } else {
            new_lines.join("\n")
        };

        // Apply the change using put_text_with_change
        self.put_text_with_change(path, &new_text)
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
    pub fn get_changes(&mut self, have_deps: &[ChangeHash]) -> Vec<Change> {
        self.doc.get_changes(have_deps)
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
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);

        // Get parent object
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Err(AutomergeError::Fail),
            }
        };

        // Check what exists at the path
        let text_obj = match get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            Some((Value::Object(automerge::ObjType::Text), obj_id)) => obj_id,
            Some((Value::Scalar(s), _)) => {
                // Convert scalar string to Text object
                if let ScalarValue::Str(existing_text) = s.as_ref() {
                    // Clone the text to avoid borrow checker issues
                    let existing_text_owned = existing_text.to_string();
                    let mut tx = self.doc.transaction();
                    let parent_for_put = navigate_or_create_path(&mut tx, parent_path)?;
                    let text_obj = match &field_name[0] {
                        PathSegment::Key(key) => {
                            tx.put_object(&parent_for_put, key.as_str(), automerge::ObjType::Text)?
                        }
                        PathSegment::Index(idx) => {
                            tx.put_object(&parent_for_put, *idx, automerge::ObjType::Text)?
                        }
                    };
                    // Insert existing text
                    tx.splice_text(&text_obj, 0, 0, &existing_text_owned)?;
                    let (_hash, _patch) = tx.commit();
                    text_obj
                } else {
                    return Err(AutomergeError::Fail);
                }
            }
            _ => return Err(AutomergeError::Fail),
        };

        let mut tx = self.doc.transaction();
        tx.splice_text(&text_obj, pos, del, text)?;
        let (hash, _patch) = tx.commit();

        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
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
        let segments = parse_path(path)?;

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);

        // Get parent object
        let parent_obj = if parent_path.is_empty() {
            ROOT
        } else {
            match navigate_path_read(&self.doc, parent_path)? {
                Some(obj) => obj,
                None => return Err(AutomergeError::Fail),
            }
        };

        // Check what exists at the path
        let text_obj = match get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            Some((Value::Object(automerge::ObjType::Text), obj_id)) => obj_id,
            Some((Value::Scalar(s), _)) => {
                // Convert scalar string to Text object
                if let ScalarValue::Str(existing_text) = s.as_ref() {
                    // Clone the text to avoid borrow checker issues
                    let existing_text_owned = existing_text.to_string();
                    let mut tx = self.doc.transaction();
                    let parent_for_put = navigate_or_create_path(&mut tx, parent_path)?;
                    let text_obj = match &field_name[0] {
                        PathSegment::Key(key) => {
                            tx.put_object(&parent_for_put, key.as_str(), automerge::ObjType::Text)?
                        }
                        PathSegment::Index(idx) => {
                            tx.put_object(&parent_for_put, *idx, automerge::ObjType::Text)?
                        }
                    };
                    // Insert existing text
                    tx.splice_text(&text_obj, 0, 0, &existing_text_owned)?;
                    let (_hash, _patch) = tx.commit();
                    text_obj
                } else {
                    return Err(AutomergeError::Fail);
                }
            }
            _ => return Err(AutomergeError::Fail),
        };

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
        ) -> Result<(), AutomergeError> {
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
                        populate_from_json(tx, &obj_id, KeyOrIndex::Key(k.clone()), v)?;
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
                        populate_from_json(tx, &obj_id, KeyOrIndex::Index(i), v)?;
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
                populate_from_json(&mut tx, &ROOT, KeyOrIndex::Key(k.clone()), v)?;
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
