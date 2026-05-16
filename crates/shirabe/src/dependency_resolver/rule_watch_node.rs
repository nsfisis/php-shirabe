//! ref: composer/src/Composer/DependencyResolver/RuleWatchNode.php

use crate::dependency_resolver::decisions::Decisions;
use crate::dependency_resolver::generic_rule::RuleLiterals;

pub struct RuleWatchNode {
    pub watch1: i64,
    pub watch2: i64,
    pub(crate) rule: Box<dyn RuleLiterals>,
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
    pub fn new(rule: Box<dyn RuleLiterals>) -> Self {
        let literals = rule.get_literals();
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
        let literals = self.rule.get_literals();

        // if there are only 2 elements, both are being watched anyway
        if literals.len() < 3 || self.rule.is_multi_conflict_rule() {
            return;
        }

        let literals: Vec<i64> = literals.clone();
        let mut watch_level: i64 = 0;

        for literal in &literals {
            let level = decisions.decision_level(*literal);

            if level > watch_level {
                self.watch2 = *literal;
                watch_level = level;
            }
        }
    }

    pub fn get_rule(&self) -> &dyn RuleLiterals {
        self.rule.as_ref()
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
