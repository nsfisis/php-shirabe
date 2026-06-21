//! ref: composer/tests/Composer/Test/DependencyResolver/RuleTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::{
    GenericRule, Pool, RULE_PACKAGE_REQUIRES, RULE_ROOT_REQUIRE, ReasonData, Request, Rule, RuleSet,
};
use shirabe::package::Link;
use shirabe::repository::RepositorySet;
use shirabe_php_shim::{PHP_VERSION_ID, hash_raw, unpack};
use shirabe_semver::constraint::MatchAllConstraint;

use crate::test_case::get_package;

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
fn test_get_hash() {
    let rule = generic_rule(vec![123]);

    let algo = if PHP_VERSION_ID > 80100 {
        "xxh3"
    } else {
        "sha1"
    };
    let binary = hash_raw(algo, "123");
    let hash = unpack("ihash", &binary).unwrap();

    assert_eq!(
        hash.get("hash").unwrap().as_int(),
        rule.get_hash().unwrap().as_int()
    );
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

// PHP mocks RepositorySet and Request with the constructor disabled; the RULE_PACKAGE_REQUIRES
// branch with a non-empty requires list never consults them, so real minimal instances suffice.
#[test]
fn test_pretty_string() {
    let p1 = get_package("foo", "2.1");
    let p2 = get_package("baz", "1.1");
    let mut pool = Pool::new(
        vec![p1.clone(), p2.clone()],
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

    let empty_constraint = MatchAllConstraint::new(Some("*".to_string()));

    let rule = Rule::Generic(GenericRule::new(
        vec![p1.get_id(), -p2.get_id()],
        RULE_PACKAGE_REQUIRES,
        ReasonData::Link(Link::new(
            "baz".to_string(),
            "foo".to_string(),
            empty_constraint.into(),
            None,
            "*".to_string(),
        )),
    ));

    assert_eq!(
        "baz 1.1 relates to foo * -> satisfiable by foo[2.1].",
        rule.get_pretty_string(
            &repository_set,
            &request,
            &mut pool,
            false,
            &IndexMap::new(),
            &vec![],
        )
        .unwrap()
    );
}
