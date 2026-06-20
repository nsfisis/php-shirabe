//! ref: composer/tests/Composer/Test/DependencyResolver/RuleTest.php

use shirabe::dependency_resolver::{GenericRule, RULE_ROOT_REQUIRE, ReasonData, Rule, RuleSet};
use shirabe_semver::constraint::MatchAllConstraint;

fn root_require_reason() -> ReasonData {
    ReasonData::RootRequire {
        package_name: String::new(),
        constraint: MatchAllConstraint::new(None).into(),
    }
}

fn generic_rule(literals: Vec<i64>) -> Rule {
    Rule::Generic(GenericRule::new(
        literals,
        RULE_ROOT_REQUIRE,
        root_require_reason(),
    ))
}

#[test]
#[ignore = "Rule::get_hash reaches shirabe_php_shim::hash_raw, which is todo!()"]
fn test_get_hash() {
    todo!()
}

#[test]
fn test_equals_for_rules_with_different_hashes() {
    let rule = generic_rule(vec![1, 2]);
    let rule2 = generic_rule(vec![1, 3]);

    assert!(!rule.equals(&rule2));
}

#[test]
fn test_equals_for_rules_with_differ_literals_quantity() {
    let rule = generic_rule(vec![1, 12]);
    let rule2 = generic_rule(vec![1]);

    assert!(!rule.equals(&rule2));
}

#[test]
fn test_equals_for_rules_with_same_literals() {
    let rule = generic_rule(vec![1, 12]);
    let rule2 = generic_rule(vec![1, 12]);

    assert!(rule.equals(&rule2));
}

#[test]
fn test_set_and_get_type() {
    let mut rule = generic_rule(vec![]);
    rule.set_type(RuleSet::TYPE_REQUEST);

    assert_eq!(RuleSet::TYPE_REQUEST, rule.get_type());
}

#[test]
fn test_enable() {
    let mut rule = generic_rule(vec![]);
    rule.disable().unwrap();
    rule.enable();

    assert!(rule.is_enabled());
    assert!(!rule.is_disabled());
}

#[test]
fn test_disable() {
    let mut rule = generic_rule(vec![]);
    rule.enable();
    rule.disable().unwrap();

    assert!(rule.is_disabled());
    assert!(!rule.is_enabled());
}

#[test]
fn test_is_assertions() {
    let rule = generic_rule(vec![1, 12]);
    let rule2 = generic_rule(vec![1]);

    assert!(!rule.is_assertion());
    assert!(rule2.is_assertion());
}

// In PHP this mocks RepositorySet and Request and passes a Link reason to build the
// pretty string. The mocked collaborators cannot be reproduced here.
#[test]
#[ignore = "getPrettyString needs mocked RepositorySet and Request; mocking is not available"]
fn test_pretty_string() {
    todo!()
}
