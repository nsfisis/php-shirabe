//! ref: composer/src/Composer/DependencyResolver/RuleWatchGraph.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::dependency_resolver::Decisions;
use crate::dependency_resolver::Rule;
use crate::dependency_resolver::RuleWatchChain;
use crate::dependency_resolver::RuleWatchNode;

#[derive(Debug)]
pub struct RuleWatchGraph {
    pub(crate) watch_chains: IndexMap<i64, RuleWatchChain>,
}

impl Default for RuleWatchGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleWatchGraph {
    pub fn new() -> Self {
        Self {
            watch_chains: IndexMap::new(),
        }
    }

    pub fn insert(&mut self, node: Rc<RefCell<RuleWatchNode>>) {
        if node.borrow().get_rule().borrow().is_assertion() {
            return;
        }

        let is_multi_conflict = node.borrow().get_rule().borrow().is_multi_conflict_rule();

        if !is_multi_conflict {
            let watch1 = node.borrow().watch1;
            let watch2 = node.borrow().watch2;
            for literal in [watch1, watch2] {
                if !self.watch_chains.contains_key(&literal) {
                    self.watch_chains.insert(literal, RuleWatchChain::new());
                }
                self.watch_chains
                    .get_mut(&literal)
                    .unwrap()
                    .unshift(node.clone());
            }
        } else {
            let literals: Vec<i64> = node.borrow().get_rule().borrow().get_literals();
            for literal in literals {
                if !self.watch_chains.contains_key(&literal) {
                    self.watch_chains.insert(literal, RuleWatchChain::new());
                }
                self.watch_chains
                    .get_mut(&literal)
                    .unwrap()
                    .unshift(node.clone());
            }
        }
    }

    pub fn propagate_literal(
        &mut self,
        decided_literal: i64,
        level: i64,
        decisions: &mut Decisions,
    ) -> Option<Rc<RefCell<Rule>>> {
        let literal = -decided_literal;

        if !self.watch_chains.contains_key(&literal) {
            return None;
        }

        self.watch_chains.get_mut(&literal).unwrap().rewind();
        while self.watch_chains.get(&literal).unwrap().valid() {
            let node = self.watch_chains.get(&literal).unwrap().current().clone();
            let is_multi_conflict = node.borrow().get_rule().borrow().is_multi_conflict_rule();
            if !is_multi_conflict {
                let other_watch = node.borrow().get_other_watch(literal);

                if !node.borrow().get_rule().borrow().is_disabled()
                    && !decisions.satisfy(other_watch)
                {
                    let rule_literals: Vec<i64> = node.borrow().get_rule().borrow().get_literals();

                    let alternative_literals: Vec<i64> = rule_literals
                        .into_iter()
                        .filter(|&rule_literal| {
                            literal != rule_literal
                                && other_watch != rule_literal
                                && !decisions.conflict(rule_literal)
                        })
                        .collect();

                    if !alternative_literals.is_empty() {
                        let first_alternative = alternative_literals[0];
                        self.move_watch(literal, first_alternative, node);
                        continue;
                    }

                    if decisions.conflict(other_watch) {
                        return Some(node.borrow().get_rule());
                    }

                    decisions.decide(other_watch, level, node.borrow().get_rule());
                }
            } else {
                let literals: Vec<i64> = node.borrow().get_rule().borrow().get_literals();
                for other_literal in literals {
                    if literal != other_literal && !decisions.satisfy(other_literal) {
                        if decisions.conflict(other_literal) {
                            return Some(node.borrow().get_rule());
                        }

                        decisions.decide(other_literal, level, node.borrow().get_rule());
                    }
                }
            }

            self.watch_chains.get_mut(&literal).unwrap().next();
        }

        None
    }

    pub(crate) fn move_watch(
        &mut self,
        from_literal: i64,
        to_literal: i64,
        node: Rc<RefCell<RuleWatchNode>>,
    ) {
        if !self.watch_chains.contains_key(&to_literal) {
            self.watch_chains.insert(to_literal, RuleWatchChain::new());
        }

        node.borrow_mut().move_watch(from_literal, to_literal);
        self.watch_chains.get_mut(&from_literal).unwrap().remove();
        self.watch_chains
            .get_mut(&to_literal)
            .unwrap()
            .unshift(node);
    }
}
