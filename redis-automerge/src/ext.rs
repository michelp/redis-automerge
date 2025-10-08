//! Extension trait and minimal implementation for working with Redis Automerge.

use automerge::{
    transaction::Transactable, Automerge, AutomergeError, Change, ObjId, ReadDoc, ScalarValue, Value, ROOT,
};

/// Parse a JSON-like path into components.
/// Supports both "$.foo.bar" and "foo.bar" syntax.
/// Returns a vector of path segments.
fn parse_path(path: &str) -> Vec<&str> {
    let trimmed = path.strip_prefix("$.").unwrap_or(path);
    if trimmed.is_empty() {
        vec![]
    } else {
        trimmed.split('.').collect()
    }
}

/// Navigate to a nested object in the document, creating intermediate maps as needed.
/// Returns the ObjId of the target object where the final value should be set.
fn navigate_or_create_path<T: Transactable>(
    tx: &mut T,
    path: &[&str],
) -> Result<ObjId, AutomergeError> {
    let mut current = ROOT;

    for segment in path {
        // Check if the key exists and is a map
        match tx.get(&current, *segment)? {
            Some((Value::Object(obj_type), obj_id)) => {
                if obj_type == automerge::ObjType::Map {
                    current = obj_id;
                } else {
                    // Path segment exists but is not a map
                    return Err(AutomergeError::Fail);
                }
            }
            Some(_) => {
                // Path segment exists but is not a map
                return Err(AutomergeError::Fail);
            }
            None => {
                // Create a new map at this location
                current = tx.put_object(&current, *segment, automerge::ObjType::Map)?;
            }
        }
    }

    Ok(current)
}

/// Navigate to a nested object in the document for reading.
/// Returns None if any part of the path doesn't exist.
fn navigate_path_read(
    doc: &Automerge,
    path: &[&str],
) -> Result<Option<ObjId>, AutomergeError> {
    let mut current = ROOT;

    for segment in path {
        match doc.get(&current, *segment)? {
            Some((Value::Object(obj_type), obj_id)) => {
                if obj_type == automerge::ObjType::Map {
                    current = obj_id;
                } else {
                    return Ok(None);
                }
            }
            Some(_) => return Ok(None),
            None => return Ok(None),
        }
    }

    Ok(Some(current))
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

    /// Insert a text value using a path (e.g., "user.profile.name" or "$.user.profile.name").
    /// Creates intermediate maps as needed.
    pub fn put_text(&mut self, path: &str, value: &str) -> Result<(), AutomergeError> {
        let segments = parse_path(path);
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        tx.put(&parent_obj, field_name[0], value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a text value using a path (e.g., "user.profile.name" or "$.user.profile.name").
    pub fn get_text(&self, path: &str) -> Result<Option<String>, AutomergeError> {
        let segments = parse_path(path);

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

        if let Some((Value::Scalar(s), _)) = self.doc.get(&parent_obj, field_name[0])? {
            if let ScalarValue::Str(t) = s.as_ref() {
                return Ok(Some(t.to_string()));
            }
        }
        Ok(None)
    }

    /// Insert an integer value using a path (e.g., "user.age" or "$.user.age").
    /// Creates intermediate maps as needed.
    pub fn put_int(&mut self, path: &str, value: i64) -> Result<(), AutomergeError> {
        let segments = parse_path(path);
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        tx.put(&parent_obj, field_name[0], value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve an integer value using a path (e.g., "user.age" or "$.user.age").
    pub fn get_int(&self, path: &str) -> Result<Option<i64>, AutomergeError> {
        let segments = parse_path(path);

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

        if let Some((Value::Scalar(s), _)) = self.doc.get(&parent_obj, field_name[0])? {
            if let ScalarValue::Int(i) = s.as_ref() {
                return Ok(Some(*i));
            }
        }
        Ok(None)
    }

    /// Insert a double value using a path (e.g., "metrics.temperature" or "$.metrics.temperature").
    /// Creates intermediate maps as needed.
    pub fn put_double(&mut self, path: &str, value: f64) -> Result<(), AutomergeError> {
        let segments = parse_path(path);
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        tx.put(&parent_obj, field_name[0], value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a double value using a path (e.g., "metrics.temperature" or "$.metrics.temperature").
    pub fn get_double(&self, path: &str) -> Result<Option<f64>, AutomergeError> {
        let segments = parse_path(path);

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

        if let Some((Value::Scalar(s), _)) = self.doc.get(&parent_obj, field_name[0])? {
            if let ScalarValue::F64(f) = s.as_ref() {
                return Ok(Some(*f));
            }
        }
        Ok(None)
    }

    /// Insert a boolean value using a path (e.g., "flags.active" or "$.flags.active").
    /// Creates intermediate maps as needed.
    pub fn put_bool(&mut self, path: &str, value: bool) -> Result<(), AutomergeError> {
        let segments = parse_path(path);
        let mut tx = self.doc.transaction();

        if segments.is_empty() {
            return Err(AutomergeError::Fail);
        }

        let (parent_path, field_name) = segments.split_at(segments.len() - 1);
        let parent_obj = navigate_or_create_path(&mut tx, parent_path)?;

        tx.put(&parent_obj, field_name[0], value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a boolean value using a path (e.g., "flags.active" or "$.flags.active").
    pub fn get_bool(&self, path: &str) -> Result<Option<bool>, AutomergeError> {
        let segments = parse_path(path);

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

        if let Some((Value::Scalar(s), _)) = self.doc.get(&parent_obj, field_name[0])? {
            if let ScalarValue::Boolean(b) = s.as_ref() {
                return Ok(Some(*b));
            }
        }
        Ok(None)
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
