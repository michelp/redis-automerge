//! Extension trait and minimal implementation for working with Redis Automerge.

use automerge::{
    transaction::Transactable, Automerge, AutomergeError, Change, ObjId, ReadDoc, ScalarValue, Value, ROOT,
};

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
                let index = bracket_content.parse::<usize>()
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
            PathSegment::Key(key) => {
                match doc.get(&current, key.as_str())? {
                    Some((Value::Object(_obj_type), obj_id)) => {
                        current = obj_id;
                    }
                    Some(_) => return Ok(None),
                    None => return Ok(None),
                }
            }
            PathSegment::Index(idx) => {
                match doc.get(&current, *idx)? {
                    Some((Value::Object(_obj_type), obj_id)) => {
                        current = obj_id;
                    }
                    Some(_) => return Ok(None),
                    None => return Ok(None),
                }
            }
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

/// Basic client holding an Automerge document and any changes which need to be
/// persisted to the AOF stream.
pub struct RedisAutomergeClient {
    doc: Automerge,
    aof: Vec<Vec<u8>>,
}

impl RedisAutomergeClient {
    /// Create a new client with an empty Automerge document.
    pub fn new() -> Self {
        Self {
            doc: Automerge::new(),
            aof: Vec::new(),
        }
    }

    /// Insert a text value using a path (e.g., "user.profile.name", "users[0].name", or "$.users[0].name").
    /// Creates intermediate maps as needed. Array indices must already exist.
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

    /// Retrieve a text value using a path (e.g., "user.profile.name", "users[0].name", or "$.users[0].name").
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

        if let Some((Value::Scalar(s), _)) = get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            if let ScalarValue::Str(t) = s.as_ref() {
                return Ok(Some(t.to_string()));
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

        if let Some((Value::Scalar(s), _)) = get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            if let ScalarValue::Int(i) = s.as_ref() {
                return Ok(Some(*i));
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

        if let Some((Value::Scalar(s), _)) = get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            if let ScalarValue::F64(f) = s.as_ref() {
                return Ok(Some(*f));
            }
        }
        Ok(None)
    }

    /// Insert a boolean value using a path (e.g., "flags.active", "flags[0]", or "$.flags.active").
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

    /// Retrieve a boolean value using a path (e.g., "flags.active", "flags[0]", or "$.flags.active").
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

        if let Some((Value::Scalar(s), _)) = get_value_from_parent(&self.doc, &parent_obj, &field_name[0])? {
            if let ScalarValue::Boolean(b) = s.as_ref() {
                return Ok(Some(*b));
            }
        }
        Ok(None)
    }

    /// Create a new list at the specified path.
    /// Example: create_list("users") creates an empty list at the root.
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

    /// Append a text value to a list at the specified path.
    /// Example: append_text("users", "Alice") appends "Alice" to the users list.
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

    /// Get the length of a list at the specified path.
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
