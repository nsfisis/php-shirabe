//! ref: composer/tests/Composer/Test/Advisory/AuditorTest.php

use crate::io_mock::{Expectation, get_io_mock};
use crate::test_case::get_complete_package;
use crate::test_case::get_package;
use chrono::Utc;
use indexmap::IndexMap;
use shirabe::advisory::AnySecurityAdvisory;
use shirabe::advisory::Auditor;
use shirabe::advisory::PartialSecurityAdvisory;
use shirabe::advisory::SecurityAdvisory;
use shirabe::downloader::TransportException;
use shirabe::io::BufferIO;
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::package::BasePackageHandle;
use shirabe::package::PackageInterfaceHandle;
use shirabe::repository::AdvisoryProviderInterface;
use shirabe::repository::FindPackageConstraint;
use shirabe::repository::LoadPackagesResult;
use shirabe::repository::ProviderInfo;
use shirabe::repository::RepositoryInterface;
use shirabe::repository::RepositoryInterfaceHandle;
use shirabe::repository::RepositorySet;
use shirabe::repository::SearchResult;
use shirabe::repository::SecurityAdvisoryResult;
use shirabe_external_packages::symfony::console::output::output_interface;
use shirabe_php_shim::date_create;
use shirabe_semver::VersionParser;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

fn constraint(operator: &str, version: &str) -> shirabe_semver::constraint::AnyConstraint {
    SimpleConstraint::new(operator.to_string(), version.to_string(), None).into()
}

fn full_advisory() -> AnySecurityAdvisory {
    let mut source: IndexMap<String, String> = IndexMap::new();
    source.insert("name".to_string(), "foo".to_string());
    source.insert("remoteId".to_string(), "remoteID".to_string());
    AnySecurityAdvisory::Full(SecurityAdvisory::new(
        "foo/bar".to_string(),
        "123".to_string(),
        constraint("=", "1.0.0.0"),
        "test".to_string(),
        vec![source],
        Utc::now(),
        None,
        None,
        None,
    ))
}

fn full_advisory_with_id(advisory_id: &str) -> AnySecurityAdvisory {
    let mut source: IndexMap<String, String> = IndexMap::new();
    source.insert("name".to_string(), "foo".to_string());
    source.insert("remoteId".to_string(), "remoteID".to_string());
    AnySecurityAdvisory::Full(SecurityAdvisory::new(
        "foo/bar".to_string(),
        advisory_id.to_string(),
        constraint("=", "1.0.0.0"),
        "test".to_string(),
        vec![source],
        Utc::now(),
        None,
        None,
        None,
    ))
}

fn partial_advisory(advisory_id: &str) -> AnySecurityAdvisory {
    AnySecurityAdvisory::Partial(PartialSecurityAdvisory::new(
        "foo/bar".to_string(),
        advisory_id.to_string(),
        constraint("=", "1.0.0.0"),
    ))
}

fn ignore_list(pairs: Vec<(&str, Option<&str>)>) -> IndexMap<String, Option<String>> {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(String::from)))
        .collect()
}

/// Behavior of the mocked repository's advisory provider. Replaces PHP's
/// `getMockBuilder(ComposerRepository::class)` partial mock; the real `RepositorySet` still drives
/// `getMatchingSecurityAdvisories`, only the per-repo advisory lookup is stubbed.
#[derive(Debug)]
enum AdvisorySource {
    /// Mirrors `AuditorTest::getMockAdvisories()` filtered by the package constraint map.
    Table,
    /// Returns a fixed advisory map regardless of the request (reachable repo).
    Fixed(IndexMap<String, Vec<AnySecurityAdvisory>>),
    /// Throws a `TransportException`, simulating an unreachable repository.
    Unreachable(String),
}

#[derive(Debug)]
struct MockAdvisoryRepository {
    source: AdvisorySource,
}

impl AdvisoryProviderInterface for MockAdvisoryRepository {
    fn has_security_advisories(&mut self) -> anyhow::Result<bool> {
        Ok(true)
    }

    fn get_security_advisories(
        &mut self,
        package_constraint_map: IndexMap<String, AnyConstraint>,
        _allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        match &self.source {
            AdvisorySource::Unreachable(message) => {
                Err(TransportException::new(message.clone(), 404).into())
            }
            AdvisorySource::Fixed(advisories) => Ok(SecurityAdvisoryResult {
                names_found: package_constraint_map.keys().cloned().collect(),
                advisories: advisories.clone(),
            }),
            AdvisorySource::Table => {
                let mut advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                for (package, list) in mock_advisories() {
                    let Some(constraint) = package_constraint_map.get(&package) else {
                        continue;
                    };
                    let filtered: Vec<AnySecurityAdvisory> = list
                        .into_iter()
                        .filter(|advisory| advisory.affected_versions().matches(constraint))
                        .collect();
                    if !filtered.is_empty() {
                        advisories.insert(package, filtered);
                    }
                }
                Ok(SecurityAdvisoryResult {
                    names_found: package_constraint_map.keys().cloned().collect(),
                    advisories,
                })
            }
        }
    }
}

impl RepositoryInterface for MockAdvisoryRepository {
    fn count(&self) -> anyhow::Result<usize> {
        unimplemented!("not used by Auditor")
    }
    fn has_package(&self, _package: PackageInterfaceHandle) -> bool {
        unimplemented!("not used by Auditor")
    }
    fn find_package(
        &mut self,
        _name: &str,
        _constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn find_packages(
        &mut self,
        _name: &str,
        _constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn load_packages(
        &mut self,
        _package_name_map: IndexMap<String, Option<AnyConstraint>>,
        _acceptable_stabilities: IndexMap<String, i64>,
        _stability_flags: IndexMap<String, i64>,
        _already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        unimplemented!("not used by Auditor")
    }
    fn search(
        &mut self,
        _query: String,
        _mode: i64,
        _type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        unimplemented!("not used by Auditor")
    }
    fn get_providers(
        &mut self,
        _package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        unimplemented!("not used by Auditor")
    }
    fn get_repo_name(&self) -> String {
        "mock advisory repo".to_string()
    }
    fn as_advisory_provider_mut(&mut self) -> Option<&mut dyn AdvisoryProviderInterface> {
        Some(self)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// ref: AuditorTest::getMockAdvisories. All entries carry full data so they load as
/// `SecurityAdvisory` (never partial) regardless of `allowPartialAdvisories`.
fn mock_advisories() -> IndexMap<String, Vec<AnySecurityAdvisory>> {
    let mut advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
    advisories.insert(
        "vendor1/package1".to_string(),
        vec![
            mock_advisory(
                "vendor1/package1",
                "ID1",
                "advisory1",
                "https://advisory.example.com/advisory1",
                "CVE1",
                ">=3,<3.4.3|>=1,<2.5.6",
                "source1",
                "RemoteID1",
                "2022-05-25 13:21:00",
                "medium",
            ),
            mock_advisory(
                "vendor1/package1",
                "ID4",
                "advisory4",
                "https://advisory.example.com/advisory4",
                "CVE3",
                ">=8,<8.2.2|>=1,<2.5.6",
                "source2",
                "RemoteID4",
                "2022-05-25 13:21:00",
                "high",
            ),
            mock_advisory(
                "vendor1/package1",
                "ID5",
                "advisory5",
                "https://advisory.example.com/advisory5",
                "",
                ">=8,<8.2.2|>=1,<2.5.6",
                "source1",
                "RemoteID3",
                "2022-05-25 13:21:00",
                "medium",
            ),
        ],
    );
    advisories.insert(
        "vendor1/package2".to_string(),
        vec![mock_advisory(
            "vendor1/package2",
            "ID2",
            "advisory2",
            "https://advisory.example.com/advisory2",
            "",
            ">=3,<3.4.3|>=1,<2.5.6",
            "source1",
            "RemoteID2",
            "2022-05-25 13:21:00",
            "medium",
        )],
    );
    advisories.insert(
        "vendorx/packagex".to_string(),
        vec![mock_advisory(
            "vendorx/packagex",
            "IDx",
            "advisory17",
            "https://advisory.example.com/advisory17",
            "CVE5",
            ">=3,<3.4.3|>=1,<2.5.6",
            "source2",
            "RemoteIDx",
            "2015-05-25 13:21:00",
            "medium",
        )],
    );
    advisories.insert(
        "vendor2/package1".to_string(),
        vec![
            mock_advisory(
                "vendor2/package1",
                "ID3",
                "advisory3",
                "https://advisory.example.com/advisory3",
                "CVE2",
                ">=3,<3.4.3|>=1,<2.5.6",
                "source2",
                "RemoteID1",
                "2022-05-25 13:21:00",
                "medium",
            ),
            mock_advisory(
                "vendor2/package1",
                "ID6",
                "advisory6",
                "https://advisory.example.com/advisory6",
                "CVE4",
                ">=3,<3.4.3|>=1,<2.5.6",
                "source2",
                "RemoteID3",
                "2015-05-25 13:21:00",
                "medium",
            ),
        ],
    );
    advisories.insert(
        "vendory/packagey".to_string(),
        vec![mock_advisory(
            "vendory/packagey",
            "IDy",
            "advisory7",
            "https://advisory.example.com/advisory7",
            "CVE5",
            ">=3,<3.4.3|>=1,<2.5.6",
            "source2",
            "RemoteID4",
            "2015-05-25 13:21:00",
            "medium",
        )],
    );
    advisories.insert(
        "vendor3/package1".to_string(),
        vec![mock_advisory(
            "vendor3/package1",
            "ID7",
            "advisory7",
            "https://advisory.example.com/advisory7",
            "CVE5",
            ">=3,<3.4.3|>=1,<2.5.6",
            "source2",
            "RemoteID4",
            "2015-05-25 13:21:00",
            "medium",
        )],
    );
    advisories
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the PHP advisory data shape"
)]
fn mock_advisory(
    package_name: &str,
    advisory_id: &str,
    title: &str,
    link: &str,
    cve: &str,
    affected_versions: &str,
    source_name: &str,
    remote_id: &str,
    reported_at: &str,
    severity: &str,
) -> AnySecurityAdvisory {
    let mut source: IndexMap<String, String> = IndexMap::new();
    source.insert("name".to_string(), source_name.to_string());
    source.insert("remoteId".to_string(), remote_id.to_string());
    AnySecurityAdvisory::Full(SecurityAdvisory::new(
        package_name.to_string(),
        advisory_id.to_string(),
        VersionParser.parse_constraints(affected_versions).unwrap(),
        title.to_string(),
        vec![source],
        date_create::<Utc>(reported_at).unwrap(),
        Some(cve.to_string()),
        Some(link.to_string()),
        Some(severity.to_string()),
    ))
}

/// ref: AuditorTest::getRepoSet. Real `RepositorySet` holding a single repository whose advisory
/// provider serves `getMockAdvisories()`.
fn get_repo_set() -> RepositorySet {
    let mut repo_set = RepositorySet::new(
        "stable",
        IndexMap::new(),
        vec![],
        IndexMap::new(),
        IndexMap::new(),
        IndexMap::new(),
    );
    repo_set
        .add_repository(RepositoryInterfaceHandle::new(MockAdvisoryRepository {
            source: AdvisorySource::Table,
        }))
        .unwrap();
    repo_set
}

/// ref: AuditorTest::testAudit (auditProvider).
#[test]
fn test_audit() {
    #[derive(Clone)]
    struct Case {
        packages: Vec<PackageInterfaceHandle>,
        warning_only: bool,
        abandoned: &'static str,
        format: &'static str,
        ignore_abandoned: IndexMap<String, Option<String>>,
        expected: i64,
        output: &'static str,
    }

    let abandoned_with_replacement = || {
        let p = get_complete_package("vendor/abandoned", "1.0.0");
        p.set_abandoned(shirabe_php_shim::PhpMixed::String("foo/bar".to_string()));
        let handle: PackageInterfaceHandle = p.into();
        handle
    };
    let abandoned_no_replacement = || {
        let p = get_complete_package("vendor/abandoned2", "1.0.0");
        p.set_abandoned(shirabe_php_shim::PhpMixed::Bool(true));
        let handle: PackageInterfaceHandle = p.into();
        handle
    };

    let cases: Vec<Case> = vec![
        Case {
            packages: vec![
                get_package("vendor1/package2", "9.0.0"),
                get_package("vendor1/package1", "9.0.0"),
                get_package("vendor3/package1", "9.0.0"),
            ],
            warning_only: true,
            abandoned: Auditor::ABANDONED_IGNORE,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_OK,
            output: "No security vulnerability advisories found.",
        },
        Case {
            packages: vec![
                get_package("vendor1/package2", "9.0.0"),
                get_package("vendor1/package1", "8.2.1"),
                get_package("vendor3/package1", "9.0.0"),
            ],
            warning_only: true,
            abandoned: Auditor::ABANDONED_IGNORE,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_VULNERABLE,
            output:
                "<warning>Found 2 security vulnerability advisories affecting 1 package:</warning>
Package: vendor1/package1
Severity: high
Advisory ID: ID4
CVE: CVE3
Title: advisory4
URL: https://advisory.example.com/advisory4
Affected versions: >=8,<8.2.2|>=1,<2.5.6
Reported at: 2022-05-25T13:21:00+00:00
--------
Package: vendor1/package1
Severity: medium
Advisory ID: ID5
CVE: \nTitle: advisory5
URL: https://advisory.example.com/advisory5
Affected versions: >=8,<8.2.2|>=1,<2.5.6
Reported at: 2022-05-25T13:21:00+00:00",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_IGNORE,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_OK,
            output: "No security vulnerability advisories found.",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![("vendor/*", None)]),
            expected: Auditor::STATUS_OK,
            output: "No security vulnerability advisories found.",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![
                ("vendor/abandoned", None),
                ("vendor/abandoned2", None),
            ]),
            expected: Auditor::STATUS_OK,
            output: "No security vulnerability advisories found.",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![("acme/test", Some("ignoring because yolo"))]),
            expected: Auditor::STATUS_ABANDONED,
            output: "No security vulnerability advisories found.
Found 2 abandoned packages:
vendor/abandoned is abandoned. Use foo/bar instead.
vendor/abandoned2 is abandoned. No replacement was suggested.",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: true,
            abandoned: Auditor::ABANDONED_REPORT,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_OK,
            output: "No security vulnerability advisories found.
Found 2 abandoned packages:
vendor/abandoned is abandoned. Use foo/bar instead.
vendor/abandoned2 is abandoned. No replacement was suggested.",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_TABLE,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_ABANDONED,
            output: "No security vulnerability advisories found.
Found 2 abandoned packages:
+-------------------+----------------------------------------------------------------------------------+
| Abandoned Package | Suggested Replacement                                                            |
+-------------------+----------------------------------------------------------------------------------+
| vendor/abandoned  | foo/bar                                                                          |
| vendor/abandoned2 | none                                                                             |
+-------------------+----------------------------------------------------------------------------------+",
        },
        Case {
            packages: vec![
                get_package("vendor1/package1", "8.2.1"),
                abandoned_with_replacement(),
                abandoned_no_replacement(),
            ],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_TABLE,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_VULNERABLE | Auditor::STATUS_ABANDONED,
            output: "Found 2 security vulnerability advisories affecting 1 package:
+-------------------+----------------------------------------------------------------------------------+
| Package           | vendor1/package1                                                                 |
| Severity          | high                                                                             |
| Advisory ID       | ID4                                                                              |
| CVE               | CVE3                                                                             |
| Title             | advisory4                                                                        |
| URL               | https://advisory.example.com/advisory4                                           |
| Affected versions | >=8,<8.2.2|>=1,<2.5.6                                                            |
| Reported at       | 2022-05-25T13:21:00+00:00                                                        |
+-------------------+----------------------------------------------------------------------------------+
+-------------------+----------------------------------------------------------------------------------+
| Package           | vendor1/package1                                                                 |
| Severity          | medium                                                                           |
| Advisory ID       | ID5                                                                              |
| CVE               |                                                                                  |
| Title             | advisory5                                                                        |
| URL               | https://advisory.example.com/advisory5                                           |
| Affected versions | >=8,<8.2.2|>=1,<2.5.6                                                            |
| Reported at       | 2022-05-25T13:21:00+00:00                                                        |
+-------------------+----------------------------------------------------------------------------------+
Found 2 abandoned packages:
+-------------------+----------------------------------------------------------------------------------+
| Abandoned Package | Suggested Replacement                                                            |
+-------------------+----------------------------------------------------------------------------------+
| vendor/abandoned  | foo/bar                                                                          |
| vendor/abandoned2 | none                                                                             |
+-------------------+----------------------------------------------------------------------------------+",
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_JSON,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_ABANDONED,
            output: "{
    \"advisories\": [],
    \"abandoned\": {
        \"vendor/abandoned\": \"foo/bar\",
        \"vendor/abandoned2\": null
    }
}",
        },
    ];

    for case in cases {
        let repo_set = get_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
            ));
        let auditor = Auditor;
        let result = auditor
            .audit(
                &io,
                &repo_set,
                case.packages.clone(),
                case.format,
                case.warning_only,
                IndexMap::new(),
                case.abandoned,
                IndexMap::new(),
                false,
                case.ignore_abandoned.clone(),
            )
            .unwrap();
        assert_eq!(case.expected, result);
        assert_eq!(
            case.output,
            io.borrow()
                .as_any()
                .downcast_ref::<BufferIO>()
                .unwrap()
                .get_output()
                .replace('\r', "")
                .trim()
        );
    }
}

/// ref: AuditorTest::testAuditWithIgnore (ignoredIdsProvider).
#[test]
fn test_audit_with_ignore() {
    struct Case {
        packages: Vec<PackageInterfaceHandle>,
        ignored_ids: IndexMap<String, Option<String>>,
        exit_code: i64,
        expected_output: Vec<Expectation>,
    }

    let cases: Vec<Case> = vec![
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: ignore_list(vec![("CVE1", None)]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendor1/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID1"),
                Expectation::text("CVE: CVE1"),
                Expectation::text("Title: advisory1"),
                Expectation::text("URL: https://advisory.example.com/advisory1"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
            ],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: ignore_list(vec![("CVE1", Some("A good reason"))]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendor1/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID1"),
                Expectation::text("CVE: CVE1"),
                Expectation::text("Title: advisory1"),
                Expectation::text("URL: https://advisory.example.com/advisory1"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
                Expectation::text("Ignore reason: A good reason"),
            ],
        },
        Case {
            packages: vec![get_package("vendor1/package2", "3.0.0.0")],
            ignored_ids: ignore_list(vec![("ID2", None)]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendor1/package2"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID2"),
                Expectation::text("CVE: "),
                Expectation::text("Title: advisory2"),
                Expectation::text("URL: https://advisory.example.com/advisory2"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
            ],
        },
        Case {
            packages: vec![get_package("vendorx/packagex", "3.0.0.0")],
            ignored_ids: ignore_list(vec![("RemoteIDx", None)]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendorx/packagex"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: IDx"),
                Expectation::text("CVE: CVE5"),
                Expectation::text("Title: advisory17"),
                Expectation::text("URL: https://advisory.example.com/advisory17"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2015-05-25T13:21:00+00:00"),
            ],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: ignore_list(vec![("vendor1/package1", None)]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendor1/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID1"),
                Expectation::text("CVE: CVE1"),
                Expectation::text("Title: advisory1"),
                Expectation::text("URL: https://advisory.example.com/advisory1"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
            ],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: ignore_list(vec![(
                "vendor1/package1",
                Some("Package has known safe usage"),
            )]),
            exit_code: 0,
            expected_output: vec![
                Expectation::text(
                    "Found 1 ignored security vulnerability advisory affecting 1 package:",
                ),
                Expectation::text("Package: vendor1/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID1"),
                Expectation::text("CVE: CVE1"),
                Expectation::text("Title: advisory1"),
                Expectation::text("URL: https://advisory.example.com/advisory1"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
                Expectation::text("Ignore reason: Package has known safe usage"),
            ],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: IndexMap::new(),
            exit_code: 1,
            expected_output: vec![
                Expectation::text("Found 1 security vulnerability advisory affecting 1 package:"),
                Expectation::text("Package: vendor1/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID1"),
                Expectation::text("CVE: CVE1"),
                Expectation::text("Title: advisory1"),
                Expectation::text("URL: https://advisory.example.com/advisory1"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
            ],
        },
        Case {
            packages: vec![
                get_package("vendor3/package1", "3.0.0.0"),
                get_package("vendorx/packagex", "3.0.0.0"),
                get_package("vendor2/package1", "3.0.0.0"),
            ],
            ignored_ids: ignore_list(vec![("RemoteIDx", None), ("ID3", None), ("ID6", None)]),
            exit_code: 1,
            expected_output: vec![
                Expectation::text(
                    "Found 3 ignored security vulnerability advisories affecting 2 packages:",
                ),
                Expectation::text("Package: vendor2/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID3"),
                Expectation::text("CVE: CVE2"),
                Expectation::text("Title: advisory3"),
                Expectation::text("URL: https://advisory.example.com/advisory3"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2022-05-25T13:21:00+00:00"),
                Expectation::text("Ignore reason: None specified"),
                Expectation::text("--------"),
                Expectation::text("Package: vendor2/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID6"),
                Expectation::text("CVE: CVE4"),
                Expectation::text("Title: advisory6"),
                Expectation::text("URL: https://advisory.example.com/advisory6"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2015-05-25T13:21:00+00:00"),
                Expectation::text("Ignore reason: None specified"),
                Expectation::text("--------"),
                Expectation::text("Package: vendorx/packagex"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: IDx"),
                Expectation::text("CVE: CVE5"),
                Expectation::text("Title: advisory17"),
                Expectation::text("URL: https://advisory.example.com/advisory17"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2015-05-25T13:21:00+00:00"),
                Expectation::text("Ignore reason: None specified"),
                Expectation::text("Found 1 security vulnerability advisory affecting 1 package:"),
                Expectation::text("Package: vendor3/package1"),
                Expectation::text("Severity: medium"),
                Expectation::text("Advisory ID: ID7"),
                Expectation::text("CVE: CVE5"),
                Expectation::text("Title: advisory7"),
                Expectation::text("URL: https://advisory.example.com/advisory7"),
                Expectation::text("Affected versions: >=3,<3.4.3|>=1,<2.5.6"),
                Expectation::text("Reported at: 2015-05-25T13:21:00+00:00"),
            ],
        },
    ];

    for case in cases {
        let repo_set = get_repo_set();
        let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
        let auditor = Auditor;
        let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
        let result = auditor
            .audit(
                &io_dyn,
                &repo_set,
                case.packages.clone(),
                Auditor::FORMAT_PLAIN,
                false,
                case.ignored_ids.clone(),
                Auditor::ABANDONED_FAIL,
                IndexMap::new(),
                false,
                IndexMap::new(),
            )
            .unwrap();
        io_mock
            .borrow_mut()
            .expects(case.expected_output.clone(), true)
            .unwrap();
        assert_eq!(case.exit_code, result);
    }
}

/// ref: AuditorTest::testAuditWithIgnoreUnreachable. The whole `RepositorySet` is mocked in PHP;
/// here a real `RepositorySet` holds a reachable repo (fixed advisories) followed by an
/// unreachable repo (throws `TransportException`), reproducing the same merged result.
#[test]
fn test_audit_with_ignore_unreachable() {
    let packages = vec![get_package("vendor1/package1", "3.0.0.0")];

    let error_message =
        "The \"https://example.org/packages.json\" file could not be downloaded: HTTP/1.1 404 Not Found"
            .to_string();

    let make_repo_set = || {
        let mut fixed: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
        fixed.insert(
            "vendor1/package1".to_string(),
            vec![
                AnySecurityAdvisory::Full(SecurityAdvisory::new(
                    "vendor1/package1".to_string(),
                    "CVE-2023-12345".to_string(),
                    constraint("=", "3.0.0.0"),
                    "First repo advisory".to_string(),
                    vec![{
                        let mut s: IndexMap<String, String> = IndexMap::new();
                        s.insert("name".to_string(), "test".to_string());
                        s.insert("remoteId".to_string(), "1".to_string());
                        s
                    }],
                    date_create::<Utc>("2023-01-01").unwrap(),
                    Some("CVE-2023-12345".to_string()),
                    Some("https://example.com/advisory/1".to_string()),
                    Some("medium".to_string()),
                )),
                AnySecurityAdvisory::Full(SecurityAdvisory::new(
                    "vendor1/package1".to_string(),
                    "CVE-2023-67890".to_string(),
                    constraint("=", "3.0.0.0"),
                    "Third repo advisory".to_string(),
                    vec![{
                        let mut s: IndexMap<String, String> = IndexMap::new();
                        s.insert("name".to_string(), "test".to_string());
                        s.insert("remoteId".to_string(), "3".to_string());
                        s
                    }],
                    date_create::<Utc>("2023-01-01").unwrap(),
                    Some("CVE-2023-67890".to_string()),
                    Some("https://example.com/advisory/3".to_string()),
                    Some("high".to_string()),
                )),
            ],
        );

        let mut repo_set = RepositorySet::new(
            "stable",
            IndexMap::new(),
            vec![],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        repo_set
            .add_repository(RepositoryInterfaceHandle::new(MockAdvisoryRepository {
                source: AdvisorySource::Fixed(fixed),
            }))
            .unwrap();
        repo_set
            .add_repository(RepositoryInterfaceHandle::new(MockAdvisoryRepository {
                source: AdvisorySource::Unreachable(error_message.clone()),
            }))
            .unwrap();
        repo_set
    };

    let auditor = Auditor;

    // Without the ignoreUnreachable flag the TransportException propagates.
    {
        let repo_set = make_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
            ));
        let err = auditor
            .audit(
                &io,
                &repo_set,
                packages.clone(),
                Auditor::FORMAT_PLAIN,
                false,
                IndexMap::new(),
                Auditor::ABANDONED_IGNORE,
                IndexMap::new(),
                false,
                IndexMap::new(),
            )
            .expect_err("Expected TransportException was not thrown");
        assert!(err.to_string().contains("HTTP/1.1 404 Not Found"));
    }

    // With the ignoreUnreachable flag the advisories from reachable repos are reported.
    {
        let repo_set = make_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
            ));
        let result = auditor
            .audit(
                &io,
                &repo_set,
                packages.clone(),
                Auditor::FORMAT_PLAIN,
                false,
                IndexMap::new(),
                Auditor::ABANDONED_IGNORE,
                IndexMap::new(),
                true,
                IndexMap::new(),
            )
            .unwrap();
        assert_eq!(Auditor::STATUS_VULNERABLE, result);

        let output = io
            .borrow()
            .as_any()
            .downcast_ref::<BufferIO>()
            .unwrap()
            .get_output();
        assert!(output.contains("The following repositories were unreachable:"));
        assert!(output.contains("HTTP/1.1 404 Not Found"));
        assert!(output.contains("First repo advisory"));
        assert!(output.contains("Third repo advisory"));
        assert!(output.contains("CVE-2023-12345"));
        assert!(output.contains("CVE-2023-67890"));
    }

    // With JSON format the unreachable repositories and advisories are both included.
    {
        let repo_set = make_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(String::new(), output_interface::VERBOSITY_NORMAL, None).unwrap(),
            ));
        let result = auditor
            .audit(
                &io,
                &repo_set,
                packages.clone(),
                Auditor::FORMAT_JSON,
                false,
                IndexMap::new(),
                Auditor::ABANDONED_IGNORE,
                IndexMap::new(),
                true,
                IndexMap::new(),
            )
            .unwrap();
        assert_eq!(Auditor::STATUS_VULNERABLE, result);

        let json: serde_json::Value = serde_json::from_str(
            &io.borrow()
                .as_any()
                .downcast_ref::<BufferIO>()
                .unwrap()
                .get_output(),
        )
        .unwrap();
        let unreachable = json
            .get("unreachable-repositories")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(1, unreachable.len());
        assert!(
            unreachable[0]
                .as_str()
                .unwrap()
                .contains("HTTP/1.1 404 Not Found")
        );

        let pkg_advisories = json
            .get("advisories")
            .and_then(|v| v.get("vendor1/package1"))
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(2, pkg_advisories.len());
        assert_eq!("CVE-2023-12345", pkg_advisories[0]["cve"].as_str().unwrap());
        assert_eq!(
            "First repo advisory",
            pkg_advisories[0]["title"].as_str().unwrap()
        );
        assert_eq!("CVE-2023-67890", pkg_advisories[1]["cve"].as_str().unwrap());
        assert_eq!(
            "Third repo advisory",
            pkg_advisories[1]["title"].as_str().unwrap()
        );
    }
}

/// ref: AuditorTest::testAuditWithIgnoreSeverity (ignoreSeverityProvider).
#[test]
fn test_audit_with_ignore_severity() {
    struct Case {
        packages: Vec<PackageInterfaceHandle>,
        ignored_severities: IndexMap<String, Option<String>>,
        exit_code: i64,
        expected_output: Vec<Expectation>,
    }

    let cases: Vec<Case> = vec![
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![("medium", None)]),
            exit_code: 1,
            expected_output: vec![Expectation::text(
                "Found 2 ignored security vulnerability advisories affecting 1 package:",
            )],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![("high", None)]),
            exit_code: 1,
            expected_output: vec![Expectation::text(
                "Found 1 ignored security vulnerability advisory affecting 1 package:",
            )],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![("high", None), ("medium", None)]),
            exit_code: 0,
            expected_output: vec![Expectation::text(
                "Found 3 ignored security vulnerability advisories affecting 1 package:",
            )],
        },
    ];

    for case in cases {
        let repo_set = get_repo_set();
        let (io_mock, _io_guard) = get_io_mock(io_interface::NORMAL).unwrap();
        let auditor = Auditor;
        let io_dyn: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock.clone();
        let result = auditor
            .audit(
                &io_dyn,
                &repo_set,
                case.packages.clone(),
                Auditor::FORMAT_PLAIN,
                false,
                IndexMap::new(),
                Auditor::ABANDONED_IGNORE,
                case.ignored_severities.clone(),
                false,
                IndexMap::new(),
            )
            .unwrap();
        io_mock
            .borrow_mut()
            .expects(case.expected_output.clone(), true)
            .unwrap();
        assert_eq!(case.exit_code, result);
    }
}

#[test]
fn test_needs_complete_advisory_load() {
    let cases: Vec<(
        IndexMap<String, Vec<AnySecurityAdvisory>>,
        IndexMap<String, Option<String>>,
        bool,
    )> = vec![
        // no filter or advisories
        (IndexMap::new(), ignore_list(vec![]), false),
        // packagist filters are IDs so work fine with partial advisories
        (
            IndexMap::new(),
            ignore_list(vec![("PKSA-foo-bar", None)]),
            false,
        ),
        // packagist filters are IDs so work fine with partial advisories/2
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert(
                    "vendor1/package1".to_string(),
                    vec![full_advisory(), partial_advisory("1234")],
                );
                m
            },
            ignore_list(vec![("PKSA-foo-bar", Some("this is fine 🔥"))]),
            false,
        ),
        // no advisories no need to load any further
        (
            IndexMap::new(),
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
        // no advisories no need to load any further/2
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert("vendor1/package1".to_string(), vec![]);
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
        // CVE filter or other non-packagist ones might need to fully load for safety if partial advisories are present
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert(
                    "vendor1/package1".to_string(),
                    vec![full_advisory(), partial_advisory("1234")],
                );
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            true,
        ),
        // filter does not trigger load if all advisories are fully loaded
        (
            {
                let mut m: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
                m.insert("vendor1/package1".to_string(), vec![full_advisory()]);
                m.insert(
                    "vendor1/package2".to_string(),
                    vec![full_advisory_with_id("1234")],
                );
                m
            },
            ignore_list(vec![("CVE-2025-1234", None)]),
            false,
        ),
    ];

    let auditor = Auditor;
    for (advisories, ignore_list, expected) in cases {
        assert_eq!(
            expected,
            auditor.needs_complete_advisory_load(&advisories, &ignore_list)
        );
    }
}
