//! Extension trait and minimal implementation for working with Redis Automerge.

use automerge::{
    transaction::Transactable, Automerge, AutomergeError, Change, ReadDoc, ScalarValue, Value, ROOT,
};

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

    /// Insert a text value at the root object under the given key.
    pub fn put_text(&mut self, key: &str, value: &str) -> Result<(), AutomergeError> {
        let mut tx = self.doc.transaction();
        tx.put(ROOT, key, value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a text value from the root object by key.
    pub fn get_text(&self, key: &str) -> Result<Option<String>, AutomergeError> {
        if let Some((Value::Scalar(s), _)) = self.doc.get(ROOT, key)? {
            if let ScalarValue::Str(t) = s.as_ref() {
                return Ok(Some(t.to_string()));
            }
        }
        Ok(None)
    }

    /// Insert an integer value at the root object under the given key.
    pub fn put_int(&mut self, key: &str, value: i64) -> Result<(), AutomergeError> {
        let mut tx = self.doc.transaction();
        tx.put(ROOT, key, value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve an integer value from the root object by key.
    pub fn get_int(&self, key: &str) -> Result<Option<i64>, AutomergeError> {
        if let Some((Value::Scalar(s), _)) = self.doc.get(ROOT, key)? {
            if let ScalarValue::Int(i) = s.as_ref() {
                return Ok(Some(*i));
            }
        }
        Ok(None)
    }

    /// Insert a double value at the root object under the given key.
    pub fn put_double(&mut self, key: &str, value: f64) -> Result<(), AutomergeError> {
        let mut tx = self.doc.transaction();
        tx.put(ROOT, key, value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a double value from the root object by key.
    pub fn get_double(&self, key: &str) -> Result<Option<f64>, AutomergeError> {
        if let Some((Value::Scalar(s), _)) = self.doc.get(ROOT, key)? {
            if let ScalarValue::F64(f) = s.as_ref() {
                return Ok(Some(*f));
            }
        }
        Ok(None)
    }

    /// Insert a boolean value at the root object under the given key.
    pub fn put_bool(&mut self, key: &str, value: bool) -> Result<(), AutomergeError> {
        let mut tx = self.doc.transaction();
        tx.put(ROOT, key, value)?;
        let (hash, _patch) = tx.commit();
        if let Some(h) = hash {
            if let Some(change) = self.doc.get_change_by_hash(&h) {
                self.aof.push(change.raw_bytes().to_vec());
            }
        }
        Ok(())
    }

    /// Retrieve a boolean value from the root object by key.
    pub fn get_bool(&self, key: &str) -> Result<Option<bool>, AutomergeError> {
        if let Some((Value::Scalar(s), _)) = self.doc.get(ROOT, key)? {
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
