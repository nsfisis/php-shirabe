//! ref: composer/src/Composer/DependencyResolver/RuleWatchChain.php

use crate::dependency_resolver::rule_watch_node::RuleWatchNode;

/// An extension of SplDoublyLinkedList with seek and removal of current element.
pub struct RuleWatchChain {
    data: Vec<RuleWatchNode>,
    current_offset: usize,
}

impl RuleWatchChain {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            current_offset: 0,
        }
    }

    fn rewind(&mut self) {
        self.current_offset = 0;
    }

    fn next(&mut self) {
        self.current_offset += 1;
    }

    fn key(&self) -> usize {
        self.current_offset
    }

    fn offset_unset(&mut self, offset: usize) {
        self.data.remove(offset);
    }

    /// Moves the internal iterator to the specified offset.
    pub fn seek(&mut self, offset: usize) {
        self.rewind();
        for _ in 0..offset {
            self.next();
        }
    }

    /// Removes the current element from the list.
    pub fn remove(&mut self) {
        let offset = self.key();
        self.offset_unset(offset);
        self.seek(offset);
    }
}
