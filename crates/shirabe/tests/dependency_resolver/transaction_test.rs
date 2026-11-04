//! ref: composer/tests/Composer/Test/DependencyResolver/TransactionTest.php

use crate::test_case::{get_alias_package, get_package, get_version_constraint};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::dependency_resolver::operation::AnyOperation;
use shirabe::dependency_resolver::transaction::Transaction;
use shirabe::package::Link;
use shirabe::package::handle::PackageInterfaceHandle;
use shirabe_php_shim::{PhpMixed, bconcat};

/// PHP `new Link($source, $target, $constraint, $type)`: prettyConstraint defaults to
/// `(string) $constraint`.
fn mk_link(
    source: &BStr,
    target: &BStr,
    constraint: shirabe_semver::constraint::AnyConstraint,
    r#type: &bstr::BStr,
) -> Link {
    let pretty = constraint.get_pretty_string();
    Link::new(
        bconcat!(source),
        bconcat!(target),
        constraint,
        Some(r#type.to_owned()),
        Some(pretty),
    )
}

/// Mirrors a PHP expected/actual operation entry.
#[derive(Debug)]
enum OperationEntry {
    Job {
        job: BString,
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
        if let AnyOperation::Update(update) = &**operation {
            result.push(OperationEntry::Update {
                from: update.get_initial_package(),
                to: update.get_target_package(),
            });
        } else {
            result.push(OperationEntry::Job {
                job: operation.get_operation_type().to_owned(),
                package: operation.get_target_package(),
            });
        }
    }

    assert_eq!(expected, result);
}

#[test]
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
        package_a,
        package_a_alias,
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
        .__set_type(BString::from("composer-installer"));
    for plugin_package in [&plugin2, &dl_modifying_plugin, &dl_modifying_plugin2] {
        plugin_package
            .as_complete_package()
            .unwrap()
            .__set_type(BString::from("composer-plugin"));
    }

    plugin2.as_complete_package().unwrap().__set_requires(
        IndexMap::from([(
            BString::from("x/plugin2-dep"),
            mk_link(
                BStr::new("x/plugin2"),
                BStr::new("x/plugin2-dep"),
                get_version_constraint("=", "1.0.0"),
                Link::TYPE_REQUIRE,
            ),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    dl_modifying_plugin2
        .as_complete_package()
        .unwrap()
        .__set_requires(
            IndexMap::from([(
                BString::from("x/downloads-modifying2-dep"),
                mk_link(
                    BStr::new("x/downloads-modifying2"),
                    BStr::new("x/downloads-modifying2-dep"),
                    get_version_constraint("=", "1.0.0"),
                    Link::TYPE_REQUIRE,
                ),
            )])
            .into_iter()
            .collect::<indexmap::IndexMap<_, _>>(),
        );
    dl_modifying_plugin
        .as_complete_package()
        .unwrap()
        .__set_extra(IndexMap::from([(
            "plugin-modifies-downloads".into(),
            PhpMixed::Bool(true),
        )]));
    dl_modifying_plugin2
        .as_complete_package()
        .unwrap()
        .__set_extra(IndexMap::from([(
            "plugin-modifies-downloads".into(),
            PhpMixed::Bool(true),
        )]));

    package_d.as_complete_package().unwrap().__set_requires(
        IndexMap::from([
            (
                BString::from("f/f"),
                mk_link(
                    BStr::new("d/d"),
                    BStr::new("f/f"),
                    get_version_constraint(">", "0.2"),
                    Link::TYPE_REQUIRE,
                ),
            ),
            (
                BString::from("g/provider"),
                mk_link(
                    BStr::new("d/d"),
                    BStr::new("g/provider"),
                    get_version_constraint(">", "0.2"),
                    Link::TYPE_REQUIRE,
                ),
            ),
        ])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );
    package_g.as_complete_package().unwrap().__set_provides(
        IndexMap::from([(
            BString::from("g/provider"),
            mk_link(
                BStr::new("g/g"),
                BStr::new("g/provider"),
                get_version_constraint("==", "1.0.0"),
                Link::TYPE_PROVIDE,
            ),
        )])
        .into_iter()
        .collect::<indexmap::IndexMap<_, _>>(),
    );

    let expected_operations = vec![
        OperationEntry::Job {
            job: BString::from("uninstall"),
            package: package_c,
        },
        OperationEntry::Job {
            job: BString::from("uninstall"),
            package: package_e,
        },
        OperationEntry::Job {
            job: BString::from("markAliasUninstalled"),
            package: package_e_alias,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: dl_modifying_plugin,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: dl_modifying_plugin2_dep,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: dl_modifying_plugin2,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: plugin,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: plugin2_dep,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: plugin2,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: package_a0_first,
        },
        OperationEntry::Update {
            from: package_b,
            to: package_b_new,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: package_g,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: package_f,
        },
        OperationEntry::Job {
            job: BString::from("markAliasInstalled"),
            package: package_f_alias2,
        },
        OperationEntry::Job {
            job: BString::from("markAliasInstalled"),
            package: package_f_alias1,
        },
        OperationEntry::Job {
            job: BString::from("install"),
            package: package_d,
        },
    ];

    let transaction = Transaction::new(present_packages, result_packages);
    check_transaction_operations(&transaction, expected_operations);
}
