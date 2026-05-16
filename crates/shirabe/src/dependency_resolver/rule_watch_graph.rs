//! ref: composer/src/Composer/DependencyResolver/RuleWatchGraph.php

use std::any::Any;

use indexmap::IndexMap;

use crate::dependency_resolver::decisions::Decisions;
use crate::dependency_resolver::multi_conflict_rule::MultiConflictRule;
use crate::dependency_resolver::rule::Rule;
use crate::dependency_resolver::rule_watch_chain::RuleWatchChain;
use crate::dependency_resolver::rule_watch_node::RuleWatchNode;

#[derive(Debug)]
pub struct RuleWatchGraph {
    pub(crate) watch_chains: IndexMap<i64, RuleWatchChain>,
}

impl RuleWatchGraph {
    pub fn new() -> Self {
        Self {
            watch_chains: IndexMap::new(),
        }
    }

    pub fn insert(&mut self, node: RuleWatchNode) {
        if node.get_rule().is_assertion() {
            return;
        }

        if (node.get_rule().as_any() as &dyn Any)
            .downcast_ref::<MultiConflictRule>()
            .is_none()
        {
            for literal in [node.watch1, node.watch2] {
                if !self.watch_chains.contains_key(&literal) {
                    self.watch_chains.insert(literal, RuleWatchChain::new());
                }
                self.watch_chains
                    .get_mut(&literal)
                    .unwrap()
                    .unshift(node.clone());
            }
        } else {
            for literal in node.get_rule().get_literals() {
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
    ) -> Option<Box<dyn Rule>> {
        let literal = -decided_literal;

        if !self.watch_chains.contains_key(&literal) {
            return None;
        }

        let chain = self.watch_chains.get_mut(&literal).unwrap();

        chain.rewind();
        while chain.valid() {
            let node = chain.current();
            if (node.get_rule().as_any() as &dyn Any)
                .downcast_ref::<MultiConflictRule>()
                .is_none()
            {
                let other_watch = node.get_other_watch(literal);

                if !node.get_rule().is_disabled() && !decisions.satisfy(other_watch) {
                    let rule_literals = node.get_rule().get_literals();

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
                        return Some(chain.current().get_rule_boxed());
                    }

                    decisions.decide(other_watch, level, chain.current().get_rule_boxed());
                }
            } else {
                for other_literal in node.get_rule().get_literals() {
                    if literal != other_literal && !decisions.satisfy(other_literal) {
                        if decisions.conflict(other_literal) {
                            return Some(node.get_rule_boxed());
                        }

                        decisions.decide(other_literal, level, node.get_rule_boxed());
                    }
                }
            }

            chain.next();
        }

        None
    }

    pub(crate) fn move_watch(&mut self, from_literal: i64, to_literal: i64, node: RuleWatchNode) {
        if !self.watch_chains.contains_key(&to_literal) {
            self.watch_chains.insert(to_literal, RuleWatchChain::new());
        }

        node.move_watch(from_literal, to_literal);
        self.watch_chains.get_mut(&from_literal).unwrap().remove();
        self.watch_chains
            .get_mut(&to_literal)
            .unwrap()
            .unshift(node);
    }
}
