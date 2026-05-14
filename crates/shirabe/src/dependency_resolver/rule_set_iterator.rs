//! ref: composer/src/Composer/DependencyResolver/RuleSetIterator.php

use indexmap::IndexMap;
use crate::dependency_resolver::rule::Rule;

/// Implements PHP \Iterator over a grouped rule set.
#[derive(Debug)]
pub struct RuleSetIterator {
    pub(crate) rules: IndexMap<i64, Vec<Rule>>,
    pub(crate) types: Vec<i64>,
    pub(crate) current_offset: i64,
    pub(crate) current_type: i64,
    pub(crate) current_type_offset: i64,
}

impl RuleSetIterator {
    pub fn new(rules: IndexMap<i64, Vec<Rule>>) -> Self {
        let mut types: Vec<i64> = rules.keys().copied().collect();
        types.sort();
        let mut iter = Self {
            rules,
            types,
            current_offset: 0,
            current_type: -1,
            current_type_offset: 0,
        };
        iter.rewind();
        iter
    }

    pub fn current(&self) -> &Rule {
        &self.rules[&self.current_type][self.current_offset as usize]
    }

    pub fn key(&self) -> i64 {
        self.current_type
    }

    pub fn next(&mut self) {
        self.current_offset += 1;

        if !self.rules.contains_key(&self.current_type) {
            return;
        }

        if self.current_offset >= self.rules[&self.current_type].len() as i64 {
            self.current_offset = 0;

            loop {
                self.current_type_offset += 1;

                if self.types.get(self.current_type_offset as usize).is_none() {
                    self.current_type = -1;
                    break;
                }

                self.current_type = self.types[self.current_type_offset as usize];

                if self.rules[&self.current_type].len() != 0 {
                    break;
                }
            }
        }
    }

    pub fn rewind(&mut self) {
        self.current_offset = 0;
        self.current_type_offset = -1;
        self.current_type = -1;

        loop {
            self.current_type_offset += 1;

            if self.types.get(self.current_type_offset as usize).is_none() {
                self.current_type = -1;
                break;
            }

            self.current_type = self.types[self.current_type_offset as usize];

            if self.rules[&self.current_type].len() != 0 {
                break;
            }
        }
    }

    pub fn valid(&self) -> bool {
        if let Some(rules) = self.rules.get(&self.current_type) {
            rules.get(self.current_offset as usize).is_some()
        } else {
            false
        }
    }
}
