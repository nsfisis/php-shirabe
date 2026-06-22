//! ref: composer/tests/Composer/Test/DependencyResolver/TransactionTest.php

use indexmap::IndexMap;
use shirabe::dependency_resolver::transaction::Transaction;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

use crate::test_case::{get_alias_package, get_package, get_version_constraint};

/// PHP `new Link($source, $target, $constraint, $type)`: prettyConstraint defaults to
/// `(string) $constraint`.
fn mk_link(
    source: &str,
    target: &str,
    constraint: shirabe_semver::constraint::AnyConstraint,
    r#type: &str,
) -> Link {
    let pretty = constraint.get_pretty_string();
    Link::new(
        source.to_string(),
        target.to_string(),
        constraint,
        Some(r#type.to_string()),
        pretty,
    )
}

/// Mirrors a PHP expected/actual operation entry.
#[derive(Debug)]
enum OperationEntry {
    Job {
        job: String,
        package: PackageInterfaceHandle,
    },
    Update {
        from: PackageInterfaceHandle,
        to: PackageInterfaceHandle,
    },
}

impl PartialEq for OperationEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Job {
                    job: j1,
                    package: p1,
                },
                Self::Job {
                    job: j2,
                    package: p2,
                },
            ) => j1 == j2 && p1.ptr_eq(p2),
            (Self::Update { from: f1, to: t1 }, Self::Update { from: f2, to: t2 }) => {
                f1.ptr_eq(f2) && t1.ptr_eq(t2)
            }
            _ => false,
        }
    }
}

fn check_transaction_operations(transaction: &Transaction, expected: Vec<OperationEntry>) {
    let mut result: Vec<OperationEntry> = vec![];
    for operation in transaction.get_operations() {
        if let Some(update) = operation.as_update_operation() {
            result.push(OperationEntry::Update {
                from: update.get_initial_package(),
                to: update.get_target_package(),
            });
        } else {
            result.push(OperationEntry::Job {
                job: operation.get_operation_type(),
                package: operation.get_package(),
            });
        }
    }

    assert_eq!(expected, result);
}

#[test]
#[ignore]
fn test_transaction_generation_and_sorting() {
    let package_a = get_package("a/a", "dev-master");
    let package_a_alias = get_alias_package(&package_a, "1.0.x-dev");
    let package_b = get_package("b/b", "1.0.0");
    let package_e = get_package("e/e", "dev-foo");
    let package_e_alias = get_alias_package(&package_e, "1.0.x-dev");
    let package_c = get_package("c/c", "1.0.0");
    let present_packages = vec![
        package_a.clone(),
        package_a_alias.clone(),
        package_b.clone(),
        package_e.clone(),
        package_e_alias.clone(),
        package_c.clone(),
    ];

    let package_b_new = get_package("b/b", "2.1.3");
    let package_d = get_package("d/d", "1.2.3");
    let package_f = get_package("f/f", "1.0.0");
    let package_f_alias1 = get_alias_package(&package_f, "dev-foo");
    let package_g = get_package("g/g", "1.0.0");
    let package_a0_first = get_package("a0/first", "1.2.3");
    let package_f_alias2 = get_alias_package(&package_f, "dev-bar");
    let plugin = get_package("x/plugin", "1.0.0");
    let plugin2_dep = get_package("x/plugin2-dep", "1.0.0");
    let plugin2 = get_package("x/plugin2", "1.0.0");
    let dl_modifying_plugin = get_package("x/downloads-modifying", "1.0.0");
    let dl_modifying_plugin2_dep = get_package("x/downloads-modifying2-dep", "1.0.0");
    let dl_modifying_plugin2 = get_package("x/downloads-modifying2", "1.0.0");
    let result_packages = vec![
        package_a.clone(),
        package_a_alias.clone(),
        package_b_new.clone(),
        package_d.clone(),
        package_f.clone(),
        package_f_alias1.clone(),
        package_g.clone(),
        package_a0_first.clone(),
        package_f_alias2.clone(),
        plugin.clone(),
        plugin2_dep.clone(),
        plugin2.clone(),
        dl_modifying_plugin.clone(),
        dl_modifying_plugin2_dep.clone(),
        dl_modifying_plugin2.clone(),
    ];

    plugin
        .as_complete_package()
        .unwrap()
        .__set_type("composer-installer".to_string());
    for plugin_package in [&plugin2, &dl_modifying_plugin, &dl_modifying_plugin2] {
        plugin_package
            .as_complete_package()
            .unwrap()
            .__set_type("composer-plugin".to_string());
    }

    plugin2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "x/plugin2-dep".to_string(),
            mk_link(
                "x/plugin2",
                "x/plugin2-dep",
                get_version_constraint("=", "1.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    dl_modifying_plugin2
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([(
            "x/downloads-modifying2-dep".to_string(),
            mk_link(
                "x/downloads-modifying2",
                "x/downloads-modifying2-dep",
                get_version_constraint("=", "1.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )]));
    dl_modifying_plugin
        .as_complete_package()
        .unwrap()
        .__set_extra(IndexMap::from([(
            "plugin-modifies-downloads".to_string(),
            PhpMixed::Bool(true),
        )]));
    dl_modifying_plugin2
        .as_complete_package()
        .unwrap()
        .__set_extra(IndexMap::from([(
            "plugin-modifies-downloads".to_string(),
            PhpMixed::Bool(true),
        )]));

    package_d
        .as_complete_package()
        .unwrap()
        .__set_requires(IndexMap::from([
            (
                "f/f".to_string(),
                mk_link(
                    "d/d",
                    "f/f",
                    get_version_constraint(">", "0.2"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                "g/provider".to_string(),
                mk_link(
                    "d/d",
                    "g/provider",
                    get_version_constraint(">", "0.2"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ]));
    package_g
        .as_complete_package()
        .unwrap()
        .__set_provides(IndexMap::from([(
            "g/provider".to_string(),
            mk_link(
                "g/g",
                "g/provider",
                get_version_constraint("==", "1.0.0"),
                Link::TYPE_PROVIDE,
            ),
        )]));

    let expected_operations = vec![
        OperationEntry::Job {
            job: "uninstall".to_string(),
            package: package_c.clone(),
        },
        OperationEntry::Job {
            job: "uninstall".to_string(),
            package: package_e.clone(),
        },
        OperationEntry::Job {
            job: "markAliasUninstalled".to_string(),
            package: package_e_alias.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: dl_modifying_plugin.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: dl_modifying_plugin2_dep.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: dl_modifying_plugin2.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: plugin.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: plugin2_dep.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: plugin2.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: package_a0_first.clone(),
        },
        OperationEntry::Update {
            from: package_b.clone(),
            to: package_b_new.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: package_g.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: package_f.clone(),
        },
        OperationEntry::Job {
            job: "markAliasInstalled".to_string(),
            package: package_f_alias2.clone(),
        },
        OperationEntry::Job {
            job: "markAliasInstalled".to_string(),
            package: package_f_alias1.clone(),
        },
        OperationEntry::Job {
            job: "install".to_string(),
            package: package_d.clone(),
        },
    ];

    let transaction = Transaction::new(present_packages, result_packages);
    check_transaction_operations(&transaction, expected_operations);
}
