//! Bake-on-change scheduling: expensive bakes run once per key change, never
//! per frame. The caller owns what a "key" is (a settings snapshot, a body
//! set, a map version); identical keys are free.

/// Tracks the last completed bake key. [`maintain`](Self::maintain) reports a
/// bake as owed exactly when the presented key differs from it, so repeated
/// calls cost one comparison.
#[derive(Debug)]
pub struct AmortizedBake<K: PartialEq> {
    completed: Option<K>,
}

impl<K: PartialEq + Clone> AmortizedBake<K> {
    /// Creates an empty tracker that owes one bake immediately.
    pub const fn new() -> Self {
        Self { completed: None }
    }

    /// Records `key` and reports whether the caller owes a bake for it.
    pub fn maintain(&mut self, key: &K) -> bool {
        if self.completed.as_ref() == Some(key) {
            return false;
        }
        self.completed = Some(key.clone());
        true
    }
}

impl<K: PartialEq + Clone> Default for AmortizedBake<K> {
    fn default() -> Self {
        Self::new()
    }
}
