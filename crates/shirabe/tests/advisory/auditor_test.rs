//! ref: composer/tests/Composer/Test/Advisory/AuditorTest.php sha1:ac30dd8d1e88a98370b4c9297cab377a5a6eae8e

use crate::io_mock::{Expectation, get_io_mock};
use crate::test_case::{get_complete_package, get_package};
use bstr::{BStr, BString, ByteSlice};
use chrono::Utc;
use indexmap::IndexMap;
use shirabe::advisory::{AnySecurityAdvisory, Auditor, PartialSecurityAdvisory, SecurityAdvisory};
use shirabe::downloader::TransportException;
use shirabe::io::{BufferIO, IOInterface, io_interface};
use shirabe::package::{BasePackageHandle, PackageInterfaceHandle};
use shirabe::repository::{
    AdvisoryProviderInterface, FindPackageConstraint, LoadPackagesResult, ProviderInfo,
    RepositoryInterface, RepositoryInterfaceHandle, RepositorySet, SearchResult,
    SecurityAdvisoryResult,
};
use shirabe_php_shim::{PhpMixed, bconcat, date_create, error_message, str_replace, trim};
use shirabe_semver::VersionParser;
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};
use shirabe_symfony_console::output::output_interface;

fn constraint(operator: &BStr, version: &BStr) -> shirabe_semver::constraint::AnyConstraint {
    SimpleConstraint::new(operator, bconcat!(version), None).into()
}

fn full_advisory() -> std::rc::Rc<AnySecurityAdvisory> {
    let mut source: IndexMap<BString, BString> = IndexMap::new();
    source.insert(BString::from("name"), BString::from("foo"));
    source.insert(BString::from("remoteId"), BString::from("remoteID"));
    std::rc::Rc::new(AnySecurityAdvisory::Full(SecurityAdvisory::new(
        BString::from("foo/bar"),
        BString::from("123"),
        constraint(BStr::new("="), BStr::new("1.0.0.0")),
        BString::from("test"),
        vec![source.into_iter().collect::<indexmap::IndexMap<_, _>>()],
        Utc::now(),
        None,
        None,
        None,
    )))
}

fn full_advisory_with_id(advisory_id: &BStr) -> std::rc::Rc<AnySecurityAdvisory> {
    let mut source: IndexMap<BString, BString> = IndexMap::new();
    source.insert(BString::from("name"), BString::from("foo"));
    source.insert(BString::from("remoteId"), BString::from("remoteID"));
    std::rc::Rc::new(AnySecurityAdvisory::Full(SecurityAdvisory::new(
        BString::from("foo/bar"),
        bconcat!(advisory_id),
        constraint(BStr::new("="), BStr::new("1.0.0.0")),
        BString::from("test"),
        vec![source.into_iter().collect::<indexmap::IndexMap<_, _>>()],
        Utc::now(),
        None,
        None,
        None,
    )))
}

fn partial_advisory(advisory_id: &BStr) -> std::rc::Rc<AnySecurityAdvisory> {
    std::rc::Rc::new(AnySecurityAdvisory::Partial(PartialSecurityAdvisory::new(
        BString::from("foo/bar"),
        bconcat!(advisory_id),
        constraint(BStr::new("="), BStr::new("1.0.0.0")),
    )))
}

fn ignore_list(pairs: Vec<(&BStr, Option<&BStr>)>) -> IndexMap<BString, Option<BString>> {
    pairs
        .into_iter()
        .map(|(k, v)| (BString::from(k), v.map(BString::from)))
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
    Fixed(IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>),
    /// Throws a `TransportException`, simulating an unreachable repository.
    Unreachable(BString),
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
        package_constraint_map: IndexMap<BString, AnyConstraint>,
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
                let mut advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
                    IndexMap::new();
                for (package, list) in mock_advisories() {
                    let Some(constraint) = package_constraint_map.get(bstr::B(&package)) else {
                        continue;
                    };
                    let filtered: Vec<std::rc::Rc<AnySecurityAdvisory>> = list
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
    fn count(&mut self) -> anyhow::Result<usize> {
        unimplemented!("not used by Auditor")
    }
    fn has_package(&mut self, _package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        unimplemented!("not used by Auditor")
    }
    fn find_package(
        &mut self,
        _name: &BStr,
        _constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn find_packages(
        &mut self,
        _name: &BStr,
        _constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        unimplemented!("not used by Auditor")
    }
    fn load_packages(
        &mut self,
        _package_name_map: IndexMap<BString, Option<AnyConstraint>>,
        _acceptable_stabilities: IndexMap<BString, i64>,
        _stability_flags: IndexMap<BString, i64>,
        _already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        unimplemented!("not used by Auditor")
    }
    fn search(
        &mut self,
        _query: BString,
        _mode: i64,
        _type: Option<BString>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        unimplemented!("not used by Auditor")
    }
    fn get_providers(
        &mut self,
        _package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
        unimplemented!("not used by Auditor")
    }
    fn get_repo_name(&self) -> anyhow::Result<BString> {
        Ok("mock advisory repo".into())
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
fn mock_advisories() -> IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> {
    let mut advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> = IndexMap::new();
    advisories.insert(
        BString::from("vendor1/package1"),
        vec![
            mock_advisory(
                BStr::new("vendor1/package1"),
                BStr::new("ID1"),
                BStr::new("advisory1"),
                BStr::new("https://advisory.example.com/advisory1"),
                BStr::new("CVE1"),
                BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
                BStr::new("source1"),
                BStr::new("RemoteID1"),
                BStr::new("2022-05-25 13:21:00"),
                BStr::new("medium"),
            ),
            mock_advisory(
                BStr::new("vendor1/package1"),
                BStr::new("ID4"),
                BStr::new("advisory4"),
                BStr::new("https://advisory.example.com/advisory4"),
                BStr::new("CVE3"),
                BStr::new(">=8,<8.2.2|>=1,<2.5.6"),
                BStr::new("source2"),
                BStr::new("RemoteID4"),
                BStr::new("2022-05-25 13:21:00"),
                BStr::new("high"),
            ),
            mock_advisory(
                BStr::new("vendor1/package1"),
                BStr::new("ID5"),
                BStr::new("advisory5"),
                BStr::new("https://advisory.example.com/advisory5"),
                BStr::new(""),
                BStr::new(">=8,<8.2.2|>=1,<2.5.6"),
                BStr::new("source1"),
                BStr::new("RemoteID3"),
                BStr::new("2022-05-25 13:21:00"),
                BStr::new("medium"),
            ),
        ],
    );
    advisories.insert(
        BString::from("vendor1/package2"),
        vec![mock_advisory(
            BStr::new("vendor1/package2"),
            BStr::new("ID2"),
            BStr::new("advisory2"),
            BStr::new("https://advisory.example.com/advisory2"),
            BStr::new(""),
            BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
            BStr::new("source1"),
            BStr::new("RemoteID2"),
            BStr::new("2022-05-25 13:21:00"),
            BStr::new("medium"),
        )],
    );
    advisories.insert(
        BString::from("vendorx/packagex"),
        vec![mock_advisory(
            BStr::new("vendorx/packagex"),
            BStr::new("IDx"),
            BStr::new("advisory17"),
            BStr::new("https://advisory.example.com/advisory17"),
            BStr::new("CVE5"),
            BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
            BStr::new("source2"),
            BStr::new("RemoteIDx"),
            BStr::new("2015-05-25 13:21:00"),
            BStr::new("medium"),
        )],
    );
    advisories.insert(
        BString::from("vendor2/package1"),
        vec![
            mock_advisory(
                BStr::new("vendor2/package1"),
                BStr::new("ID3"),
                BStr::new("advisory3"),
                BStr::new("https://advisory.example.com/advisory3"),
                BStr::new("CVE2"),
                BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
                BStr::new("source2"),
                BStr::new("RemoteID1"),
                BStr::new("2022-05-25 13:21:00"),
                BStr::new("medium"),
            ),
            mock_advisory(
                BStr::new("vendor2/package1"),
                BStr::new("ID6"),
                BStr::new("advisory6"),
                BStr::new("https://advisory.example.com/advisory6"),
                BStr::new("CVE4"),
                BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
                BStr::new("source2"),
                BStr::new("RemoteID3"),
                BStr::new("2015-05-25 13:21:00"),
                BStr::new("medium"),
            ),
        ],
    );
    advisories.insert(
        BString::from("vendory/packagey"),
        vec![mock_advisory(
            BStr::new("vendory/packagey"),
            BStr::new("IDy"),
            BStr::new("advisory7"),
            BStr::new("https://advisory.example.com/advisory7"),
            BStr::new("CVE5"),
            BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
            BStr::new("source2"),
            BStr::new("RemoteID4"),
            BStr::new("2015-05-25 13:21:00"),
            BStr::new("medium"),
        )],
    );
    advisories.insert(
        BString::from("vendor3/package1"),
        vec![mock_advisory(
            BStr::new("vendor3/package1"),
            BStr::new("ID7"),
            BStr::new("advisory7"),
            BStr::new("https://advisory.example.com/advisory7"),
            BStr::new("CVE5"),
            BStr::new(">=3,<3.4.3|>=1,<2.5.6"),
            BStr::new("source2"),
            BStr::new("RemoteID4"),
            BStr::new("2015-05-25 13:21:00"),
            BStr::new("medium"),
        )],
    );
    advisories
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors the PHP advisory data shape"
)]
fn mock_advisory(
    package_name: &BStr,
    advisory_id: &BStr,
    title: &BStr,
    link: &BStr,
    cve: &BStr,
    affected_versions: &BStr,
    source_name: &BStr,
    remote_id: &BStr,
    reported_at: &BStr,
    severity: &BStr,
) -> std::rc::Rc<AnySecurityAdvisory> {
    let mut source: IndexMap<BString, BString> = IndexMap::new();
    source.insert(BString::from("name"), bconcat!(source_name));
    source.insert(BString::from("remoteId"), bconcat!(remote_id));
    std::rc::Rc::new(AnySecurityAdvisory::Full(SecurityAdvisory::new(
        bconcat!(package_name),
        bconcat!(advisory_id),
        VersionParser.parse_constraints(affected_versions).unwrap(),
        bconcat!(title),
        vec![source.into_iter().collect::<indexmap::IndexMap<_, _>>()],
        date_create::<Utc>(reported_at).unwrap(),
        Some(bconcat!(cve)),
        Some(bconcat!(link)),
        Some(bconcat!(severity)),
    )))
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
        abandoned: &'static BStr,
        format: &'static BStr,
        ignore_abandoned: IndexMap<BString, Option<BString>>,
        expected: i64,
        output: &'static BStr,
    }

    let abandoned_with_replacement = || {
        let p = get_complete_package("vendor/abandoned", "1.0.0");
        p.set_abandoned(PhpMixed::string(BString::from("foo/bar")));
        let handle: PackageInterfaceHandle = p.into();
        handle
    };
    let abandoned_no_replacement = || {
        let p = get_complete_package("vendor/abandoned2", "1.0.0");
        p.set_abandoned(PhpMixed::Bool(true));
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
            output: BStr::new("No security vulnerability advisories found."),
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
                BStr::new("<warning>Found 2 security vulnerability advisories affecting 1 package:</warning>
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
Reported at: 2022-05-25T13:21:00+00:00"),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_IGNORE,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_OK,
            output: BStr::new("No security vulnerability advisories found."),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![(BStr::new("vendor/*"), None)]),
            expected: Auditor::STATUS_OK,
            output: BStr::new("No security vulnerability advisories found."),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![
                (BStr::new("vendor/abandoned"), None),
                (BStr::new("vendor/abandoned2"), None),
            ]),
            expected: Auditor::STATUS_OK,
            output: BStr::new("No security vulnerability advisories found."),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: ignore_list(vec![(BStr::new("acme/test"), Some(BStr::new("ignoring because yolo")))]),
            expected: Auditor::STATUS_ABANDONED,
            output: BStr::new("No security vulnerability advisories found.
Found 2 abandoned packages:
vendor/abandoned is abandoned. Use foo/bar instead.
vendor/abandoned2 is abandoned. No replacement was suggested."),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: true,
            abandoned: Auditor::ABANDONED_REPORT,
            format: Auditor::FORMAT_PLAIN,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_OK,
            output: BStr::new("No security vulnerability advisories found.
Found 2 abandoned packages:
vendor/abandoned is abandoned. Use foo/bar instead.
vendor/abandoned2 is abandoned. No replacement was suggested."),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_TABLE,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_ABANDONED,
            output: BStr::new("No security vulnerability advisories found.
Found 2 abandoned packages:
+-------------------+----------------------------------------------------------------------------------+
| Abandoned Package | Suggested Replacement                                                            |
+-------------------+----------------------------------------------------------------------------------+
| vendor/abandoned  | foo/bar                                                                          |
| vendor/abandoned2 | none                                                                             |
+-------------------+----------------------------------------------------------------------------------+"),
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
            output: BStr::new("Found 2 security vulnerability advisories affecting 1 package:
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
+-------------------+----------------------------------------------------------------------------------+"),
        },
        Case {
            packages: vec![abandoned_with_replacement(), abandoned_no_replacement()],
            warning_only: false,
            abandoned: Auditor::ABANDONED_FAIL,
            format: Auditor::FORMAT_JSON,
            ignore_abandoned: IndexMap::new(),
            expected: Auditor::STATUS_ABANDONED,
            output: BStr::new("{
    \"advisories\": [],
    \"abandoned\": {
        \"vendor/abandoned\": \"foo/bar\",
        \"vendor/abandoned2\": null
    }
}"),
        },
    ];

    for case in cases {
        let repo_set = get_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None)
                    .unwrap(),
            ));
        let auditor = Auditor;
        let result = auditor
            .audit(
                &io,
                &repo_set,
                case.packages.clone(),
                bstr::BStr::new(case.format),
                case.warning_only,
                IndexMap::new(),
                bstr::BStr::new(case.abandoned),
                IndexMap::new(),
                false,
                case.ignore_abandoned.clone(),
            )
            .unwrap();
        assert_eq!(case.expected, result);
        assert_eq!(
            case.output,
            trim(str_replace(
                "\r",
                "",
                io.borrow()
                    .as_any()
                    .downcast_ref::<BufferIO>()
                    .unwrap()
                    .get_output()
            ))
        );
    }
}

/// ref: AuditorTest::testAuditWithIgnore (ignoredIdsProvider).
#[test]
fn test_audit_with_ignore() {
    struct Case {
        packages: Vec<PackageInterfaceHandle>,
        ignored_ids: IndexMap<BString, Option<BString>>,
        exit_code: i64,
        expected_output: Vec<Expectation>,
    }

    let cases: Vec<Case> = vec![
        Case {
            packages: vec![get_package("vendor1/package1", "3.0.0.0")],
            ignored_ids: ignore_list(vec![(BStr::new("CVE1"), None)]),
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
            ignored_ids: ignore_list(vec![(BStr::new("CVE1"), Some(BStr::new("A good reason")))]),
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
            ignored_ids: ignore_list(vec![(BStr::new("ID2"), None)]),
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
            ignored_ids: ignore_list(vec![(BStr::new("RemoteIDx"), None)]),
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
            ignored_ids: ignore_list(vec![(BStr::new("vendor1/package1"), None)]),
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
                BStr::new("vendor1/package1"),
                Some(BStr::new("Package has known safe usage")),
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
            ignored_ids: ignore_list(vec![
                (BStr::new("RemoteIDx"), None),
                (BStr::new("ID3"), None),
                (BStr::new("ID6"), None),
            ]),
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
                bstr::BStr::new(Auditor::FORMAT_PLAIN),
                false,
                case.ignored_ids.clone(),
                bstr::BStr::new(Auditor::ABANDONED_FAIL),
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

    let unreachable_message = BString::from(
        "The \"https://example.org/packages.json\" file could not be downloaded: HTTP/1.1 404 Not Found",
    );

    let make_repo_set = || {
        let mut fixed: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> = IndexMap::new();
        fixed.insert(
            BString::from("vendor1/package1"),
            vec![
                std::rc::Rc::new(AnySecurityAdvisory::Full(SecurityAdvisory::new(
                    BString::from("vendor1/package1"),
                    BString::from("CVE-2023-12345"),
                    constraint(BStr::new("="), BStr::new("3.0.0.0")),
                    BString::from("First repo advisory"),
                    vec![{
                        let mut s: IndexMap<BString, BString> = IndexMap::new();
                        s.insert(BString::from("name"), BString::from("test"));
                        s.insert(BString::from("remoteId"), BString::from("1"));
                        s.into_iter().collect::<indexmap::IndexMap<_, _>>()
                    }],
                    date_create::<Utc>("2023-01-01").unwrap(),
                    Some(BString::from("CVE-2023-12345")),
                    Some(BString::from("https://example.com/advisory/1")),
                    Some(BString::from("medium")),
                ))),
                std::rc::Rc::new(AnySecurityAdvisory::Full(SecurityAdvisory::new(
                    BString::from("vendor1/package1"),
                    BString::from("CVE-2023-67890"),
                    constraint(BStr::new("="), BStr::new("3.0.0.0")),
                    BString::from("Third repo advisory"),
                    vec![{
                        let mut s: IndexMap<BString, BString> = IndexMap::new();
                        s.insert(BString::from("name"), BString::from("test"));
                        s.insert(BString::from("remoteId"), BString::from("3"));
                        s.into_iter().collect::<indexmap::IndexMap<_, _>>()
                    }],
                    date_create::<Utc>("2023-01-01").unwrap(),
                    Some(BString::from("CVE-2023-67890")),
                    Some(BString::from("https://example.com/advisory/3")),
                    Some(BString::from("high")),
                ))),
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
                source: AdvisorySource::Unreachable(unreachable_message.clone()),
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
                BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None)
                    .unwrap(),
            ));
        let err = auditor
            .audit(
                &io,
                &repo_set,
                packages.clone(),
                bstr::BStr::new(Auditor::FORMAT_PLAIN),
                false,
                IndexMap::new(),
                bstr::BStr::new(Auditor::ABANDONED_IGNORE),
                IndexMap::new(),
                false,
                IndexMap::new(),
            )
            .expect_err("Expected TransportException was not thrown");
        assert!(error_message(&err).contains_str("HTTP/1.1 404 Not Found"));
    }

    // With the ignoreUnreachable flag the advisories from reachable repos are reported.
    {
        let repo_set = make_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None)
                    .unwrap(),
            ));
        let result = auditor
            .audit(
                &io,
                &repo_set,
                packages.clone(),
                bstr::BStr::new(Auditor::FORMAT_PLAIN),
                false,
                IndexMap::new(),
                bstr::BStr::new(Auditor::ABANDONED_IGNORE),
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
        assert!(output.contains_str("The following repositories were unreachable:"));
        assert!(output.contains_str("HTTP/1.1 404 Not Found"));
        assert!(output.contains_str("First repo advisory"));
        assert!(output.contains_str("Third repo advisory"));
        assert!(output.contains_str("CVE-2023-12345"));
        assert!(output.contains_str("CVE-2023-67890"));
    }

    // With JSON format the unreachable repositories and advisories are both included.
    {
        let repo_set = make_repo_set();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(
                BufferIO::new(BString::default(), output_interface::VERBOSITY_NORMAL, None)
                    .unwrap(),
            ));
        let result = auditor
            .audit(
                &io,
                &repo_set,
                packages,
                bstr::BStr::new(Auditor::FORMAT_JSON),
                false,
                IndexMap::new(),
                bstr::BStr::new(Auditor::ABANDONED_IGNORE),
                IndexMap::new(),
                true,
                IndexMap::new(),
            )
            .unwrap();
        assert_eq!(Auditor::STATUS_VULNERABLE, result);

        let json: serde_json::Value = serde_json::from_slice(
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
        ignored_severities: IndexMap<BString, Option<BString>>,
        exit_code: i64,
        expected_output: Vec<Expectation>,
    }

    let cases: Vec<Case> = vec![
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![(BStr::new("medium"), None)]),
            exit_code: 1,
            expected_output: vec![Expectation::text(
                "Found 2 ignored security vulnerability advisories affecting 1 package:",
            )],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![(BStr::new("high"), None)]),
            exit_code: 1,
            expected_output: vec![Expectation::text(
                "Found 1 ignored security vulnerability advisory affecting 1 package:",
            )],
        },
        Case {
            packages: vec![get_package("vendor1/package1", "2.0.0.0")],
            ignored_severities: ignore_list(vec![
                (BStr::new("high"), None),
                (BStr::new("medium"), None),
            ]),
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
                bstr::BStr::new(Auditor::FORMAT_PLAIN),
                false,
                IndexMap::new(),
                bstr::BStr::new(Auditor::ABANDONED_IGNORE),
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
        IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
        IndexMap<BString, Option<BString>>,
        bool,
    )> = vec![
        // no filter or advisories
        (IndexMap::new(), ignore_list(vec![]), false),
        // packagist filters are IDs so work fine with partial advisories
        (
            IndexMap::new(),
            ignore_list(vec![(BStr::new("PKSA-foo-bar"), None)]),
            false,
        ),
        // packagist filters are IDs so work fine with partial advisories/2
        (
            {
                let mut m: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
                    IndexMap::new();
                m.insert(
                    BString::from("vendor1/package1"),
                    vec![full_advisory(), partial_advisory(BStr::new("1234"))],
                );
                m
            },
            ignore_list(vec![(
                BStr::new("PKSA-foo-bar"),
                Some(BStr::new("this is fine 🔥")),
            )]),
            false,
        ),
        // no advisories no need to load any further
        (
            IndexMap::new(),
            ignore_list(vec![(BStr::new("CVE-2025-1234"), None)]),
            false,
        ),
        // no advisories no need to load any further/2
        (
            {
                let mut m: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
                    IndexMap::new();
                m.insert(BString::from("vendor1/package1"), vec![]);
                m
            },
            ignore_list(vec![(BStr::new("CVE-2025-1234"), None)]),
            false,
        ),
        // CVE filter or other non-packagist ones might need to fully load for safety if partial advisories are present
        (
            {
                let mut m: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
                    IndexMap::new();
                m.insert(
                    BString::from("vendor1/package1"),
                    vec![full_advisory(), partial_advisory(BStr::new("1234"))],
                );
                m
            },
            ignore_list(vec![(BStr::new("CVE-2025-1234"), None)]),
            true,
        ),
        // filter does not trigger load if all advisories are fully loaded
        (
            {
                let mut m: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
                    IndexMap::new();
                m.insert(BString::from("vendor1/package1"), vec![full_advisory()]);
                m.insert(
                    BString::from("vendor1/package2"),
                    vec![full_advisory_with_id(BStr::new("1234"))],
                );
                m
            },
            ignore_list(vec![(BStr::new("CVE-2025-1234"), None)]),
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
