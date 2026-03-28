// Sample Rust source file for testing code parsing

use std::collections::HashMap;

/// A simple key-value store implementation.
pub struct KeyValueStore {
    data: HashMap<String, String>,
}

impl KeyValueStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        KeyValueStore {
            data: HashMap::new(),
        }
    }

    /// Insert a key-value pair.
    pub fn insert(&mut self, key: String, value: String) {
        self.data.insert(key, value); // store the pair
    }

    /// Retrieve a value by key.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

/* This is a block comment explaining the module.
   It should be stripped during indexing. */

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut store = KeyValueStore::new();
        store.insert("hello".to_string(), "world".to_string());
        assert_eq!(store.get("hello"), Some(&"world".to_string()));
    }
}
