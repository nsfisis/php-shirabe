//! ref: composer/tests/Composer/Test/DependencyResolver/RuleSetIteratorTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::generic_rule::GenericRule;
use shirabe::dependency_resolver::pool::Pool;
use shirabe::dependency_resolver::rule::{RULE_LEARNED, RULE_ROOT_REQUIRE, ReasonData, Rule};
use shirabe::dependency_resolver::rule_set::RuleSet;
use shirabe::dependency_resolver::rule_set_iterator::RuleSetIterator;
use shirabe_semver::constraint::MatchAllConstraint;

type Rules = IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>;

fn root_require_rule() -> std::rc::Rc<std::cell::RefCell<Rule>> {
    std::rc::Rc::new(std::cell::RefCell::new(Rule::Generic(GenericRule::new(
        vec![],
        RULE_ROOT_REQUIRE,
        ReasonData::RootRequire {
            package_name: String::new(),
            constraint: MatchAllConstraint::new(None).into(),
        },
    ))))
}

fn set_up() -> (Pool, Rules) {
    let pool = Pool::new(
        vec![],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );

    let mut rules: Rules = IndexMap::new();
    rules.insert(
        RuleSet::TYPE_REQUEST,
        vec![root_require_rule(), root_require_rule()],
    );
    rules.insert(
        RuleSet::TYPE_LEARNED,
        vec![std::rc::Rc::new(std::cell::RefCell::new(Rule::Generic(
            GenericRule::new(vec![], RULE_LEARNED, ReasonData::Int(1)),
        )))],
    );
    rules.insert(RuleSet::TYPE_PACKAGE, vec![]);

    (pool, rules)
}

#[test]
fn test_foreach() {
    let (_pool, rules) = set_up();
    let mut rule_set_iterator = RuleSetIterator::new(rules.clone());

    let mut result: Vec<std::rc::Rc<std::cell::RefCell<Rule>>> = Vec::new();
    while rule_set_iterator.valid() {
        result.push(rule_set_iterator.current());
        rule_set_iterator.next();
    }

    let expected = [
        rules[&RuleSet::TYPE_REQUEST][0].clone(),
        rules[&RuleSet::TYPE_REQUEST][1].clone(),
        rules[&RuleSet::TYPE_LEARNED][0].clone(),
    ];

    assert_eq!(expected.len(), result.len());
    for (e, r) in expected.iter().zip(result.iter()) {
        assert!(std::rc::Rc::ptr_eq(e, r));
    }
}

#[test]
fn test_keys() {
    let (_pool, rules) = set_up();
    let mut rule_set_iterator = RuleSetIterator::new(rules);

    let mut result: Vec<i64> = Vec::new();
    while rule_set_iterator.valid() {
        result.push(rule_set_iterator.key());
        rule_set_iterator.next();
    }

    let expected = vec![
        RuleSet::TYPE_REQUEST,
        RuleSet::TYPE_REQUEST,
        RuleSet::TYPE_LEARNED,
    ];

    assert_eq!(expected, result);
}
