//! ref: composer/tests/Composer/Test/DependencyResolver/RuleSetTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::dependency_resolver::{
    GenericRule, Pool, RULE_LEARNED, RULE_ROOT_REQUIRE, ReasonData, Request, Rule, RuleSet,
};
use shirabe::repository::RepositorySet;
use shirabe_semver::constraint::{MatchAllConstraint, MatchNoneConstraint};

use crate::test_case::get_package;

fn root_require_reason() -> ReasonData {
    ReasonData::RootRequire {
        package_name: String::new(),
        constraint: MatchAllConstraint::new(None).into(),
    }
}

fn rule(literals: Vec<i64>) -> Rc<RefCell<Rule>> {
    Rc::new(RefCell::new(Rule::Generic(GenericRule::new(
        literals,
        RULE_ROOT_REQUIRE,
        root_require_reason(),
    ))))
}

fn learned_rule() -> Rc<RefCell<Rule>> {
    Rc::new(RefCell::new(Rule::Generic(GenericRule::new(
        vec![],
        RULE_LEARNED,
        ReasonData::Int(1),
    ))))
}

fn iterator_count(mut iterator: shirabe::dependency_resolver::RuleSetIterator) -> i64 {
    let mut count = 0;
    while iterator.valid() {
        count += 1;
        iterator.next();
    }
    count
}

#[test]
fn test_add() {
    let request0 = rule(vec![1]);
    let request1 = rule(vec![2]);
    let learned0 = learned_rule();

    let mut rule_set = RuleSet::new();
    rule_set
        .add(request0.clone(), RuleSet::TYPE_REQUEST)
        .unwrap();
    rule_set
        .add(learned0.clone(), RuleSet::TYPE_LEARNED)
        .unwrap();
    rule_set
        .add(request1.clone(), RuleSet::TYPE_REQUEST)
        .unwrap();

    let rules = rule_set.get_rules();
    assert!(
        rules
            .get(&RuleSet::TYPE_PACKAGE)
            .is_none_or(|v| v.is_empty())
    );
    let request = &rules[&RuleSet::TYPE_REQUEST];
    assert_eq!(2, request.len());
    assert!(Rc::ptr_eq(&request[0], &request0));
    assert!(Rc::ptr_eq(&request[1], &request1));
    let learned = &rules[&RuleSet::TYPE_LEARNED];
    assert_eq!(1, learned.len());
    assert!(Rc::ptr_eq(&learned[0], &learned0));
}

#[test]
fn test_add_ignores_duplicates() {
    let mut rule_set = RuleSet::new();
    rule_set.add(rule(vec![]), RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule(vec![]), RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule(vec![]), RuleSet::TYPE_REQUEST).unwrap();

    assert_eq!(
        1,
        iterator_count(rule_set.get_iterator_for(vec![RuleSet::TYPE_REQUEST]))
    );
}

#[test]
fn test_add_when_type_is_not_recognized() {
    let mut rule_set = RuleSet::new();

    // The unknown-type check happens before the hash is computed.
    assert!(rule_set.add(rule(vec![]), 7).is_err());
}

#[test]
fn test_count() {
    let mut rule_set = RuleSet::new();
    rule_set.add(rule(vec![1]), RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule(vec![2]), RuleSet::TYPE_REQUEST).unwrap();

    assert_eq!(2, rule_set.count());
}

#[test]
fn test_rule_by_id() {
    let mut rule_set = RuleSet::new();
    let rule = rule(vec![]);
    rule_set.add(rule.clone(), RuleSet::TYPE_REQUEST).unwrap();

    assert!(Rc::ptr_eq(&rule_set.rule_by_id[&0], &rule));
}

#[test]
fn test_get_iterator() {
    let mut rule_set = RuleSet::new();
    let rule1 = rule(vec![1]);
    let rule2 = rule(vec![2]);
    rule_set.add(rule1.clone(), RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule2.clone(), RuleSet::TYPE_LEARNED).unwrap();

    let mut iterator = rule_set.get_iterator();
    assert!(Rc::ptr_eq(&iterator.current(), &rule1));
    iterator.next();
    assert!(Rc::ptr_eq(&iterator.current(), &rule2));
}

#[test]
fn test_get_iterator_for() {
    let mut rule_set = RuleSet::new();
    let rule1 = rule(vec![1]);
    let rule2 = rule(vec![2]);
    rule_set.add(rule1, RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule2.clone(), RuleSet::TYPE_LEARNED).unwrap();

    let iterator = rule_set.get_iterator_for(vec![RuleSet::TYPE_LEARNED]);
    assert!(Rc::ptr_eq(&iterator.current(), &rule2));
}

#[test]
fn test_get_iterator_without() {
    let mut rule_set = RuleSet::new();
    let rule1 = rule(vec![1]);
    let rule2 = rule(vec![2]);
    rule_set.add(rule1, RuleSet::TYPE_REQUEST).unwrap();
    rule_set.add(rule2.clone(), RuleSet::TYPE_LEARNED).unwrap();

    let iterator = rule_set.get_iterator_without(vec![RuleSet::TYPE_REQUEST]);
    assert!(Rc::ptr_eq(&iterator.current(), &rule2));
}

// The constraint is MatchNoneConstraint, so what_provides returns no packages and the
// "No package found" branch is taken; the RepositorySet/Request collaborators are never
// actually consulted (PHP mocks them with the constructor disabled).
#[test]
fn test_pretty_string() {
    let p = get_package("foo", "2.1");
    let mut pool = Pool::new(
        vec![p.clone()],
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );

    let repository_set = RepositorySet::new(
        "stable",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    let request = Request::new(None);

    let mut rule_set = RuleSet::new();
    let literal = p.get_id();
    let rule = Rc::new(RefCell::new(Rule::Generic(GenericRule::new(
        vec![literal],
        RULE_ROOT_REQUIRE,
        ReasonData::RootRequire {
            package_name: "foo/bar".to_string(),
            constraint: MatchNoneConstraint::new(None).into(),
        },
    ))));

    rule_set.add(rule, RuleSet::TYPE_REQUEST).unwrap();

    let pretty = rule_set
        .get_pretty_string(
            Some(&repository_set),
            Some(&request),
            Some(&mut pool),
            false,
        )
        .unwrap();
    assert!(
        pretty.contains("REQUEST : No package found to satisfy root composer.json require foo/bar")
    );
}
