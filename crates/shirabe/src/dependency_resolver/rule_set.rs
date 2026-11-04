//! ref: composer/src/Composer/DependencyResolver/RuleSet.php sha1:1f7d32d4254b98a41c9c533cee9e0ad6abe506af

use crate::dependency_resolver::{Pool, Request, Rule, RuleSetIterator};
use crate::repository::RepositorySet;
use bstr::{BStr, BString, ByteVec};
use indexmap::IndexMap;
use shirabe_php_shim::{
    OutOfBoundsException, STR_PAD_RIGHT, bconcat, display_to_bytes, php_to_string, str_pad,
};

#[derive(Debug)]
pub struct RuleSet {
    pub rule_by_id: IndexMap<i64, std::rc::Rc<std::cell::RefCell<Rule>>>,
    rules: IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
    next_rule_id: i64,
    rules_by_hash: IndexMap<BString, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleSet {
    pub const TYPE_PACKAGE: i64 = 0;
    pub const TYPE_REQUEST: i64 = 1;
    pub const TYPE_LEARNED: i64 = 4;

    pub fn types() -> IndexMap<i64, &'static BStr> {
        let mut map = IndexMap::new();
        map.insert(Self::TYPE_PACKAGE, BStr::new("PACKAGE"));
        map.insert(Self::TYPE_REQUEST, BStr::new("REQUEST"));
        map.insert(Self::TYPE_LEARNED, BStr::new("LEARNED"));
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

    pub fn add(
        &mut self,
        rule: std::rc::Rc<std::cell::RefCell<Rule>>,
        r#type: i64,
    ) -> anyhow::Result<()> {
        let types = Self::types();
        if !types.contains_key(&r#type) {
            return Err(OutOfBoundsException::new(bconcat!("Unknown rule type: ", r#type)).into());
        }

        let hash = php_to_string(&rule.borrow().get_hash()?);

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
        self.rules.entry(r#type).or_default().push(rule.clone());
        self.rule_by_id.insert(self.next_rule_id, rule.clone());
        rule.borrow_mut().set_type(r#type);

        self.next_rule_id += 1;

        self.rules_by_hash.entry(hash).or_default().push(rule);

        Ok(())
    }

    pub fn count(&self) -> i64 {
        self.next_rule_id
    }

    pub fn rule_by_id(&self, id: i64) -> std::rc::Rc<std::cell::RefCell<Rule>> {
        self.rule_by_id[&id].clone()
    }

    pub fn get_rules(&self) -> &IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>> {
        &self.rules
    }

    pub fn get_iterator(&self) -> RuleSetIterator {
        RuleSetIterator::new(self.rules.clone())
    }

    pub fn get_iterator_for(&self, types: Vec<i64>) -> RuleSetIterator {
        let mut rules: IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>> = IndexMap::new();
        for r#type in types {
            if let Some(rules_for_type) = self.rules.get(&r#type) {
                rules.insert(r#type, rules_for_type.clone());
            }
        }
        RuleSetIterator::new(rules)
    }

    pub fn get_iterator_without(&self, types: Vec<i64>) -> RuleSetIterator {
        let mut rules: IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>> = IndexMap::new();
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
        mut pool: Option<&mut Pool>,
        is_verbose: bool,
    ) -> anyhow::Result<BString> {
        let mut string = BString::from("\n");
        let types = Self::types();
        let dummy_installed_map = IndexMap::new();
        let dummy_learned_pool = Vec::new();
        for (r#type, rules) in &self.rules {
            string.push_str(bconcat!(
                str_pad(
                    types.get(r#type).expect("unknown type"),
                    8,
                    " ",
                    STR_PAD_RIGHT
                ),
                ": "
            ));
            for rule in rules {
                if let Some(repository_set) = repository_set
                    && let Some(request) = request
                    && let Some(pool) = pool.as_deref_mut()
                {
                    string.push_str(rule.borrow().get_pretty_string(
                        repository_set,
                        request,
                        pool,
                        is_verbose,
                        &dummy_installed_map,
                        &dummy_learned_pool,
                    )?);
                } else {
                    string.push_str(display_to_bytes(&*rule.borrow()));
                }
                string.push(b'\n');
            }
            string.push_str("\n\n");
        }

        Ok(string)
    }
}

impl std::fmt::Display for RuleSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pretty = self
            .get_pretty_string(None, None, None, false)
            .expect("RuleSet::get_pretty_string() is infallible without a repository set, a request and a pool");
        write!(f, "{}", pretty.display_lossy())
    }
}
