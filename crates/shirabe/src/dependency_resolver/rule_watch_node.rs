//! ref: composer/src/Composer/DependencyResolver/RuleWatchNode.php

use std::cell::RefCell;
use std::rc::Rc;

use crate::dependency_resolver::Decisions;
use crate::dependency_resolver::Rule;

pub struct RuleWatchNode {
    pub watch1: i64,
    pub watch2: i64,
    pub(crate) rule: Rc<RefCell<Rule>>,
}

impl std::fmt::Debug for RuleWatchNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuleWatchNode")
            .field("watch1", &self.watch1)
            .field("watch2", &self.watch2)
            .finish()
    }
}

impl RuleWatchNode {
    pub fn new(rule: Rc<RefCell<Rule>>) -> Self {
        let literals = rule.borrow().get_literals();
        let literal_count = literals.len();
        let watch1 = if literal_count > 0 { literals[0] } else { 0 };
        let watch2 = if literal_count > 1 { literals[1] } else { 0 };

        Self {
            watch1,
            watch2,
            rule,
        }
    }

    pub fn watch2_on_highest(&mut self, decisions: &Decisions) {
        let literals = self.rule.borrow().get_literals();

        // if there are only 2 elements, both are being watched anyway
        if literals.len() < 3 || self.rule.borrow().is_multi_conflict_rule() {
            return;
        }

        let mut watch_level: i64 = 0;

        for literal in &literals {
            let level = decisions.decision_level(*literal);

            if level > watch_level {
                self.watch2 = *literal;
                watch_level = level;
            }
        }
    }

    pub fn get_rule(&self) -> Rc<RefCell<Rule>> {
        self.rule.clone()
    }

    pub fn get_other_watch(&self, literal: i64) -> i64 {
        if self.watch1 == literal {
            return self.watch2;
        }

        self.watch1
    }

    pub fn move_watch(&mut self, from: i64, to: i64) {
        if self.watch1 == from {
            self.watch1 = to;
        } else {
            self.watch2 = to;
        }
    }
}
