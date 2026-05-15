//! ref: composer/src/Composer/DependencyResolver/Decisions.php

use std::fmt;
use indexmap::IndexMap;
use shirabe_php_shim::LogicException;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::rule::Rule;
use crate::dependency_resolver::solver_bug_exception::SolverBugException;

#[derive(Debug)]
pub struct Decisions {
    pub(crate) pool: Pool,
    pub(crate) decision_map: IndexMap<i64, i64>,
    pub(crate) decision_queue: Vec<(i64, Rule)>,
    iterator_cursor: Option<usize>,
}

impl Decisions {
    pub const DECISION_LITERAL: usize = 0;
    pub const DECISION_REASON: usize = 1;

    pub fn new(pool: Pool) -> Self {
        Self {
            pool,
            decision_map: IndexMap::new(),
            decision_queue: Vec::new(),
            iterator_cursor: None,
        }
    }

    pub fn decide(&mut self, literal: i64, level: i64, why: Rule) {
        self.add_decision(literal, level);
        self.decision_queue.push((literal, why));
    }

    pub fn satisfy(&self, literal: i64) -> bool {
        let package_id = literal.abs();

        (literal > 0 && self.decision_map.get(&package_id).copied().unwrap_or(0) > 0)
            || (literal < 0 && self.decision_map.get(&package_id).copied().unwrap_or(0) < 0)
    }

    pub fn conflict(&self, literal: i64) -> bool {
        let package_id = literal.abs();

        (self.decision_map.contains_key(&package_id) && self.decision_map[&package_id] > 0 && literal < 0)
            || (self.decision_map.contains_key(&package_id) && self.decision_map[&package_id] < 0 && literal > 0)
    }

    pub fn decided(&self, literal_or_package_id: i64) -> bool {
        self.decision_map.get(&literal_or_package_id.abs()).copied().unwrap_or(0) != 0
    }

    pub fn undecided(&self, literal_or_package_id: i64) -> bool {
        self.decision_map.get(&literal_or_package_id.abs()).copied().unwrap_or(0) == 0
    }

    pub fn decided_install(&self, literal_or_package_id: i64) -> bool {
        let package_id = literal_or_package_id.abs();

        self.decision_map.contains_key(&package_id) && self.decision_map[&package_id] > 0
    }

    pub fn decision_level(&self, literal_or_package_id: i64) -> i64 {
        let package_id = literal_or_package_id.abs();
        if self.decision_map.contains_key(&package_id) {
            return self.decision_map[&package_id].abs();
        }

        0
    }

    pub fn decision_rule(&self, literal_or_package_id: i64) -> &Rule {
        let package_id = literal_or_package_id.abs();

        for decision in &self.decision_queue {
            if package_id == decision.0.abs() {
                return &decision.1;
            }
        }

        panic!("{}", LogicException {
            message: format!("Did not find a decision rule using {}", literal_or_package_id),
            code: 0,
        }.message);
    }

    pub fn at_offset(&self, queue_offset: usize) -> &(i64, Rule) {
        &self.decision_queue[queue_offset]
    }

    pub fn valid_offset(&self, queue_offset: i64) -> bool {
        queue_offset >= 0 && queue_offset < self.decision_queue.len() as i64
    }

    pub fn last_reason(&self) -> &Rule {
        &self.decision_queue[self.decision_queue.len() - 1].1
    }

    pub fn last_literal(&self) -> i64 {
        self.decision_queue[self.decision_queue.len() - 1].0
    }

    pub fn reset(&mut self) {
        while let Some(decision) = self.decision_queue.pop() {
            self.decision_map.insert(decision.0.abs(), 0);
        }
    }

    pub fn reset_to_offset(&mut self, offset: i64) {
        while self.decision_queue.len() as i64 > offset + 1 {
            let decision = self.decision_queue.pop().unwrap();
            self.decision_map.insert(decision.0.abs(), 0);
        }
    }

    pub fn revert_last(&mut self) {
        let last_literal = self.last_literal();
        self.decision_map.insert(last_literal.abs(), 0);
        self.decision_queue.pop();
    }

    pub fn count(&self) -> usize {
        self.decision_queue.len()
    }

    pub fn rewind(&mut self) {
        if self.decision_queue.is_empty() {
            self.iterator_cursor = None;
        } else {
            self.iterator_cursor = Some(self.decision_queue.len() - 1);
        }
    }

    pub fn current(&self) -> Option<&(i64, Rule)> {
        self.iterator_cursor.and_then(|cursor| self.decision_queue.get(cursor))
    }

    pub fn key(&self) -> Option<usize> {
        self.iterator_cursor
    }

    pub fn next(&mut self) {
        self.iterator_cursor = self.iterator_cursor.and_then(|cursor| {
            if cursor > 0 {
                Some(cursor - 1)
            } else {
                None
            }
        });
    }

    pub fn valid(&self) -> bool {
        self.iterator_cursor.is_some() && self.current().is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.decision_queue.is_empty()
    }

    pub(crate) fn add_decision(&mut self, literal: i64, level: i64) {
        let package_id = literal.abs();

        let previous_decision = self.decision_map.get(&package_id).copied().unwrap_or(0);
        if previous_decision != 0 {
            let literal_string = self.pool.literal_to_pretty_string(literal, &IndexMap::new());
            let package = self.pool.literal_to_package(literal);
            panic!("{}", SolverBugException::new(
                format!("Trying to decide {} on level {}, even though {} was previously decided as {}.", literal_string, level, package, previous_decision)
            ).0.message);
        }

        if literal > 0 {
            self.decision_map.insert(package_id, level);
        } else {
            self.decision_map.insert(package_id, -level);
        }
    }

    pub fn to_string_with_pool(&self, pool: Option<&Pool>) -> String {
        let mut decision_map = self.decision_map.clone();
        decision_map.sort_keys();
        let mut str = "[".to_string();
        for (package_id, level) in &decision_map {
            let key = if let Some(pool) = pool {
                pool.literal_to_package(*package_id).to_string()
            } else {
                package_id.to_string()
            };
            str.push_str(&format!("{}:{},", key, level));
        }
        str.push(']');

        str
    }
}

impl fmt::Display for Decisions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_with_pool(None))
    }
}
