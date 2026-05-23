//! ref: composer/src/Composer/DependencyResolver/RuleSet.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe_php_shim::OutOfBoundsException;

use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::Rule;
use crate::dependency_resolver::RuleSetIterator;
use crate::repository::RepositorySet;

#[derive(Debug)]
pub struct RuleSet {
    pub rule_by_id: IndexMap<i64, Rc<RefCell<Rule>>>,
    pub(crate) rules: IndexMap<i64, Vec<Rc<RefCell<Rule>>>>,
    pub(crate) next_rule_id: i64,
    pub(crate) rules_by_hash: IndexMap<String, Vec<Rc<RefCell<Rule>>>>,
}

impl RuleSet {
    pub const TYPE_PACKAGE: i64 = 0;
    pub const TYPE_REQUEST: i64 = 1;
    pub const TYPE_LEARNED: i64 = 4;

    pub fn types() -> IndexMap<i64, &'static str> {
        let mut map = IndexMap::new();
        map.insert(Self::TYPE_PACKAGE, "PACKAGE");
        map.insert(Self::TYPE_REQUEST, "REQUEST");
        map.insert(Self::TYPE_LEARNED, "LEARNED");
        map
    }

    pub fn new() -> Self {
        let mut rules = IndexMap::new();
        for type_ in Self::get_types_static() {
            rules.insert(type_, vec![]);
        }
        Self {
            rule_by_id: IndexMap::new(),
            rules,
            next_rule_id: 0,
            rules_by_hash: IndexMap::new(),
        }
    }

    fn get_types_static() -> Vec<i64> {
        Self::types().into_keys().collect()
    }

    pub fn add(&mut self, rule: Rc<RefCell<Rule>>, r#type: i64) -> anyhow::Result<()> {
        let types = Self::types();
        if !types.contains_key(&r#type) {
            return Err(OutOfBoundsException {
                message: format!("Unknown rule type: {}", r#type),
                code: 0,
            }
            .into());
        }

        let hash = rule.borrow().get_hash()?.to_string();

        // Skip exact duplicates that share the same hash.
        if let Some(potential_duplicates) = self.rules_by_hash.get(&hash) {
            for potential_duplicate in potential_duplicates {
                if rule.borrow().equals(&potential_duplicate.borrow()) {
                    return Ok(());
                }
            }
        }

        // The same rule instance is referenced from `rules`, `rule_by_id`, and
        // `rules_by_hash` (PHP shares one object across all three).
        self.rules
            .entry(r#type)
            .or_insert_with(Vec::new)
            .push(rule.clone());
        self.rule_by_id.insert(self.next_rule_id, rule.clone());
        rule.borrow_mut().set_type(r#type);

        self.next_rule_id += 1;

        self.rules_by_hash
            .entry(hash)
            .or_insert_with(Vec::new)
            .push(rule);

        Ok(())
    }

    pub fn count(&self) -> i64 {
        self.next_rule_id
    }

    pub fn rule_by_id(&self, id: i64) -> Rc<RefCell<Rule>> {
        self.rule_by_id[&id].clone()
    }

    pub fn get_rules(&self) -> &IndexMap<i64, Vec<Rc<RefCell<Rule>>>> {
        &self.rules
    }

    pub fn get_iterator(&self) -> RuleSetIterator {
        RuleSetIterator::new(self.rules.clone())
    }

    pub fn get_iterator_for(&self, types: Vec<i64>) -> RuleSetIterator {
        let mut rules: IndexMap<i64, Vec<Rc<RefCell<Rule>>>> = IndexMap::new();
        for r#type in types {
            if let Some(rules_for_type) = self.rules.get(&r#type) {
                rules.insert(r#type, rules_for_type.clone());
            }
        }
        RuleSetIterator::new(rules)
    }

    pub fn get_iterator_without(&self, types: Vec<i64>) -> RuleSetIterator {
        let mut rules: IndexMap<i64, Vec<Rc<RefCell<Rule>>>> = IndexMap::new();
        for (r#type, rules_for_type) in &self.rules {
            if types.contains(r#type) {
                continue;
            }
            rules.insert(*r#type, rules_for_type.clone());
        }
        RuleSetIterator::new(rules)
    }

    pub fn get_types(&self) -> Vec<i64> {
        Self::get_types_static()
    }

    pub fn get_pretty_string(
        &self,
        repository_set: Option<&RepositorySet>,
        request: Option<&Request>,
        pool: Option<&Pool>,
        is_verbose: bool,
    ) -> String {
        let types = Self::types();
        let mut string = "\n".to_string();
        for (r#type, rules) in &self.rules {
            let type_name = types.get(r#type).copied().unwrap_or("");
            string.push_str(&format!("{:<8}: ", type_name));
            for rule in rules {
                if repository_set.is_some() && request.is_some() && pool.is_some() {
                    // TODO(phase-b): get_pretty_string needs &mut Pool plus installed_map and learned_pool.
                    let _ = (repository_set, request, pool, is_verbose);
                    string.push_str(&rule.borrow().to_string());
                } else {
                    string.push_str(&rule.borrow().to_string());
                }
                string.push('\n');
            }
            string.push_str("\n\n");
        }
        string
    }
}

impl std::fmt::Display for RuleSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_pretty_string(None, None, None, false))
    }
}
