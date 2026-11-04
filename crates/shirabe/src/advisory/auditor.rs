//! ref: composer/src/Composer/Advisory/Auditor.php sha1:e2575416d104b436981565df0389ffe67cad9f4f

use crate::advisory::{AnySecurityAdvisory, SecurityAdvisory};
use crate::io::{ConsoleIO, IOInterface, IOInterfaceWrite};
use crate::json::JsonFile;
use crate::package::{CompletePackageInterfaceHandle, PackageInterfaceHandle, base_package};
use crate::repository::RepositorySet;
use crate::util::PackageInfo;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    DATE_ATOM, InvalidArgumentException, PhpMixed, array_all, array_any, array_key_exists,
    array_keys, array_reduce, bconcat, display_to_bytes, get_class, json_bytes, php_str,
    preg_is_match,
};
use shirabe_symfony_console::formatter::OutputFormatter;
use shirabe_symfony_console::helper::Cell;

/// Shape of the `--format=json` audit output.
#[derive(serde::Serialize)]
struct AuditJsonReport<'a> {
    #[serde(serialize_with = "serialize_advisories_field")]
    advisories: &'a IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    #[serde(
        rename = "ignored-advisories",
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_ignored_advisories_field"
    )]
    ignored_advisories: Option<&'a IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>>,
    #[serde(
        rename = "unreachable-repositories",
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_unreachable_repositories_field"
    )]
    unreachable_repositories: Option<&'a Vec<BString>>,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::keyed_option_str")]
    abandoned: IndexMap<BString, Option<BString>>,
}

fn serialize_advisories_field<S>(
    map: &&IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    serializer: S,
) -> anyhow::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    json_bytes::keyed(*map, serializer)
}

fn serialize_ignored_advisories_field<S>(
    map: &Option<&IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>>,
    serializer: S,
) -> anyhow::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match map {
        Some(map) => json_bytes::keyed(map, serializer),
        None => serializer.serialize_none(),
    }
}

fn serialize_unreachable_repositories_field<S>(
    repositories: &Option<&Vec<BString>>,
    serializer: S,
) -> anyhow::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match repositories {
        Some(repositories) => json_bytes::str_vec(repositories, serializer),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug)]
pub struct Auditor;

impl Auditor {
    pub const FORMAT_TABLE: &'static BStr = php_str(b"table");

    pub const FORMAT_PLAIN: &'static BStr = php_str(b"plain");

    pub const FORMAT_JSON: &'static BStr = php_str(b"json");

    pub const FORMAT_SUMMARY: &'static BStr = php_str(b"summary");

    pub const FORMATS: [&'static BStr; 4] = [
        Self::FORMAT_TABLE,
        Self::FORMAT_PLAIN,
        Self::FORMAT_JSON,
        Self::FORMAT_SUMMARY,
    ];

    pub const ABANDONED_IGNORE: &'static BStr = php_str(b"ignore");
    pub const ABANDONED_REPORT: &'static BStr = php_str(b"report");
    pub const ABANDONED_FAIL: &'static BStr = php_str(b"fail");

    pub const ABANDONEDS: [&'static BStr; 3] = [
        Self::ABANDONED_IGNORE,
        Self::ABANDONED_REPORT,
        Self::ABANDONED_FAIL,
    ];

    /// Values to determine the audit result.
    pub const STATUS_OK: i64 = 0;
    pub const STATUS_VULNERABLE: i64 = 1;
    pub const STATUS_ABANDONED: i64 = 2;

    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn audit(
        &self,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        repo_set: &RepositorySet,
        packages: Vec<PackageInterfaceHandle>,
        format: &BStr,
        warning_only: bool,
        ignore_list: IndexMap<BString, Option<BString>>,
        abandoned: &BStr,
        ignored_severities: IndexMap<BString, Option<BString>>,
        ignore_unreachable: bool,
        ignore_abandoned: IndexMap<BString, Option<BString>>,
    ) -> anyhow::Result<i64> {
        let result = repo_set.get_matching_security_advisories(
            packages.clone(),
            format == Self::FORMAT_SUMMARY,
            ignore_unreachable,
        )?;
        let mut all_advisories = result.advisories;
        let mut unreachable_repos = result.unreachable_repos;

        // we need the CVE & remote IDs set to filter ignores correctly so if we have any matches using the optimized codepath above
        // and ignores are set then we need to query again the full data to make sure it can be filtered
        if format == Self::FORMAT_SUMMARY
            && self.needs_complete_advisory_load(&all_advisories, &ignore_list)
        {
            let result = repo_set.get_matching_security_advisories(
                packages.clone(),
                false,
                ignore_unreachable,
            )?;
            all_advisories = result.advisories;
            unreachable_repos.extend(result.unreachable_repos);
        }
        let processed = self.process_advisories(all_advisories, &ignore_list, &ignored_severities);
        let advisories = processed.advisories;
        let ignored_advisories = processed.ignored_advisories;

        let mut abandoned_count: i64 = 0;
        let affected_packages_count = advisories.len() as i64;
        let abandoned_packages: Vec<CompletePackageInterfaceHandle>;
        if abandoned == Self::ABANDONED_IGNORE {
            abandoned_packages = vec![];
        } else {
            abandoned_packages = self.filter_abandoned_packages(&packages, &ignore_abandoned)?;
            if abandoned == Self::ABANDONED_FAIL {
                abandoned_count = abandoned_packages.len() as i64;
            }
        }

        let audit_bitmask =
            self.calculate_bitmask(0 < affected_packages_count, 0 < abandoned_count);

        if Self::FORMAT_JSON == format {
            let abandoned = array_reduce(
                &abandoned_packages,
                |mut carry: IndexMap<BString, Option<BString>>,
                 package: &CompletePackageInterfaceHandle| {
                    carry.insert(
                        BString::from(&package.get_pretty_name()[..]),
                        package
                            .get_replacement_package()
                            .map(|s| BString::from(&s[..])),
                    );
                    carry
                },
                IndexMap::new(),
            );
            let report = AuditJsonReport {
                advisories: &advisories,
                ignored_advisories: if ignored_advisories.is_empty() {
                    None
                } else {
                    Some(&ignored_advisories)
                },
                unreachable_repositories: if unreachable_repos.is_empty() {
                    None
                } else {
                    Some(&unreachable_repos)
                },
                abandoned,
            };

            io.write(&JsonFile::encode(&report)?);

            return Ok(audit_bitmask);
        }

        let error_or_warn = BStr::new(if warning_only { "warning" } else { "error" });
        if affected_packages_count > 0 || !ignored_advisories.is_empty() {
            let passes: Vec<(
                &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
                &BStr,
                &BStr,
            )> = vec![
                (
                    &ignored_advisories,
                    BStr::new("info"),
                    BStr::new("ignored "),
                ),
                (&advisories, error_or_warn, BStr::new("")),
            ];
            for (advisories_to_output, tag, ignored) in passes {
                let (pkg_count, total_advisory_count) = self.count_advisories(advisories_to_output);
                if pkg_count > 0 {
                    let plurality = if total_advisory_count == 1 {
                        "y"
                    } else {
                        "ies"
                    };
                    let pkg_plurality = if pkg_count == 1 { "" } else { "s" };
                    let punctuation = if format == "summary" { "." } else { ":" };
                    io.write_error(bconcat!(
                        "<",
                        tag,
                        ">Found ",
                        total_advisory_count,
                        " ",
                        ignored,
                        "security vulnerability advisor",
                        plurality,
                        " affecting ",
                        pkg_count,
                        " package",
                        pkg_plurality,
                        punctuation,
                        "</",
                        tag,
                        ">"
                    ));
                    self.output_advisories(io, advisories_to_output, format)?;
                }
            }

            if format == Self::FORMAT_SUMMARY {
                io.write_error("Run \"shirabe audit\" for a full list of advisories.");
            }
        } else {
            io.write_error("<info>No security vulnerability advisories found.</info>");
        }

        if !unreachable_repos.is_empty() {
            io.write_error("<warning>The following repositories were unreachable:</warning>");
            for repo in &unreachable_repos {
                io.write_error(bconcat!("  - ", repo));
            }
        }

        if !abandoned_packages.is_empty() && format != Self::FORMAT_SUMMARY {
            self.output_abandoned_packages(io, &abandoned_packages, format)?;
        }

        Ok(audit_bitmask)
    }

    pub fn needs_complete_advisory_load(
        &self,
        advisories: &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
        ignore_list: &IndexMap<BString, Option<BString>>,
    ) -> bool {
        if advisories.is_empty() {
            return false;
        }

        // no partial advisories present
        let advisories_values: Vec<_> = advisories.values().collect();
        if array_all(&advisories_values, |pkg_advisories| {
            array_all(
                pkg_advisories,
                |advisory: &std::rc::Rc<AnySecurityAdvisory>| {
                    advisory.as_security_advisory().is_some()
                },
            )
        }) {
            return false;
        }

        let ignored_ids = array_keys(ignore_list);

        array_any(&ignored_ids, |id: &BString| !id.starts_with(b"PKSA-"))
    }

    pub fn filter_abandoned_packages(
        &self,
        packages: &[PackageInterfaceHandle],
        ignore_abandoned: &IndexMap<BString, Option<BString>>,
    ) -> anyhow::Result<Vec<CompletePackageInterfaceHandle>> {
        let mut filter: Option<BString> = None;
        if !ignore_abandoned.is_empty() {
            filter = Some(
                base_package::package_names_to_regexp(
                    &array_keys(ignore_abandoned),
                    BStr::new("{^(?:%s)$}iD"),
                )
                .to_owned(),
            );
        }

        // PHP: array_filter($packages, fn(PackageInterface $pkg) => $pkg instanceof CompletePackageInterface && $pkg->isAbandoned() && ($filter === null || !Preg::isMatch($filter, $pkg->getName())))
        let mut result: Vec<CompletePackageInterfaceHandle> = vec![];
        for pkg in packages {
            let Some(pkg) = pkg.as_complete() else {
                continue;
            };
            if pkg.is_abandoned()
                && (filter.is_none() || !preg_is_match(filter.as_ref().unwrap(), pkg.get_name()))
            {
                result.push(pkg);
            }
        }
        Ok(result)
    }

    pub fn process_advisories(
        &self,
        all_advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
        ignore_list: &IndexMap<BString, Option<BString>>,
        ignored_severities: &IndexMap<BString, Option<BString>>,
    ) -> ProcessAdvisoriesResult {
        if ignore_list.is_empty() && ignored_severities.is_empty() {
            return ProcessAdvisoriesResult {
                advisories: all_advisories,
                ignored_advisories: IndexMap::new(),
            };
        }

        let mut advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> =
            IndexMap::new();
        let mut ignored: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>> = IndexMap::new();
        let mut ignore_reason: Option<BString> = None;

        for (package, pkg_advisories) in all_advisories {
            for advisory in pkg_advisories {
                let mut is_active = true;

                if array_key_exists(&package, ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list.get(&package).cloned().unwrap_or(None);
                }

                if array_key_exists(bstr::B(advisory.advisory_id()), ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list
                        .get(bstr::B(advisory.advisory_id()))
                        .cloned()
                        .unwrap_or(None);
                }

                if let Some(full) = advisory.as_security_advisory() {
                    if let Some(severity) = &full.severity
                        && array_key_exists(severity, ignored_severities)
                    {
                        is_active = false;
                        ignore_reason = ignored_severities
                            .get(severity)
                            .cloned()
                            .flatten()
                            .or_else(|| Some(bconcat!(severity, " severity is ignored")));
                    }

                    if let Some(cve) = &full.cve
                        && array_key_exists(cve, ignore_list)
                    {
                        is_active = false;
                        ignore_reason = ignore_list.get(cve).cloned().flatten();
                    }

                    for source in &full.sources {
                        let remote_id = source
                            .get(b"remoteId".as_slice())
                            .cloned()
                            .unwrap_or_default();
                        if array_key_exists(&remote_id, ignore_list) {
                            is_active = false;
                            ignore_reason = ignore_list.get(&remote_id).cloned().flatten();
                            break;
                        }
                    }
                }

                if is_active {
                    advisories
                        .entry(package.clone())
                        .or_default()
                        .push(advisory);
                    continue;
                }

                // Partial security advisories only used in summary mode
                // and in that case we do not need to cast the object.
                let advisory = if advisory.as_security_advisory().is_some() {
                    let full = advisory.as_security_advisory().unwrap();
                    std::rc::Rc::new(AnySecurityAdvisory::Ignored(
                        full.to_ignored_advisory(ignore_reason.clone()),
                    ))
                } else {
                    advisory
                };

                ignored.entry(package.clone()).or_default().push(advisory);
            }
        }

        ProcessAdvisoriesResult {
            advisories,
            ignored_advisories: ignored,
        }
    }

    fn count_advisories(
        &self,
        advisories: &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    ) -> (i64, i64) {
        let mut count: i64 = 0;
        for package_advisories in advisories.values() {
            count += package_advisories.len() as i64;
        }

        (advisories.len() as i64, count)
    }

    fn output_advisories(
        &self,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        advisories: &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
        format: &BStr,
    ) -> anyhow::Result<()> {
        match format {
            _ if format == Self::FORMAT_TABLE => {
                let io_ref = io.borrow();
                // PHP: `$io instanceof ConsoleIO`. BufferIO extends ConsoleIO in PHP but is
                // modeled as composition here, so also unwrap it to its inner ConsoleIO.
                let io_as_console = io_ref.as_any().downcast_ref::<ConsoleIO>().or_else(|| {
                    io_ref
                        .as_any()
                        .downcast_ref::<crate::io::buffer_io::BufferIO>()
                        .map(|buffer_io| &buffer_io.inner)
                });
                if io_as_console.is_none() {
                    return Err(InvalidArgumentException::new(bconcat!(
                        "Cannot use table format with ",
                        get_class(&PhpMixed::Null)
                    ))
                    .into());
                }
                self.output_advisories_table(io_as_console.unwrap(), advisories)?;

                Ok(())
            }
            _ if format == Self::FORMAT_PLAIN => {
                self.output_advisories_plain(io, advisories)?;

                Ok(())
            }
            _ if format == Self::FORMAT_SUMMARY => Ok(()),
            _ => Err(
                InvalidArgumentException::new(bconcat!("Invalid format \"", format, "\".")).into(),
            ),
        }
    }

    fn output_advisories_table(
        &self,
        io: &ConsoleIO,
        advisories: &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    ) -> anyhow::Result<()> {
        for package_advisories in advisories.values() {
            for advisory in package_advisories {
                let mut headers: Vec<BString> = vec![
                    BString::from("Package"),
                    BString::from("Severity"),
                    BString::from("Advisory ID"),
                    BString::from("CVE"),
                    BString::from("Title"),
                    BString::from("URL"),
                    BString::from("Affected versions"),
                    BString::from("Reported at"),
                ];
                let sa = advisory
                    .as_security_advisory()
                    .expect("output_advisories_table only receives full advisories");
                let mut row: Vec<BString> = vec![
                    BString::from(sa.package_name()),
                    self.get_severity(sa),
                    self.get_advisory_id(sa),
                    self.get_cve(sa),
                    sa.title.clone(),
                    self.get_url(sa),
                    sa.affected_versions().get_pretty_string(),
                    display_to_bytes(sa.reported_at.format(DATE_ATOM)),
                ];
                if let Some(ignored) = advisory.as_ignored() {
                    headers.push(BString::from("Ignore reason"));
                    row.push(
                        ignored
                            .ignore_reason
                            .clone()
                            .unwrap_or_else(|| BString::from("None specified")),
                    );
                }
                let mut table = io.get_table();
                table
                    .set_horizontal(true)
                    .set_headers(headers.into_iter().map(|h| h.into()).collect());
                table.add_row(
                    row.iter()
                        .map(|cell| ConsoleIO::sanitize(cell, true).into())
                        .collect::<Vec<Cell>>()
                        .into(),
                );
                table
                    .set_column_width(1, 80)
                    .set_column_max_width(1, 80)
                    .render()?;
            }
        }

        Ok(())
    }

    fn output_advisories_plain(
        &self,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        advisories: &IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    ) -> anyhow::Result<()> {
        let mut error: Vec<BString> = vec![];
        let mut first_advisory = true;
        for package_advisories in advisories.values() {
            for advisory in package_advisories {
                if !first_advisory {
                    error.push(BString::from("--------"));
                }
                let sa = advisory
                    .as_security_advisory()
                    .expect("output_advisories_plain only receives full advisories");
                error.push(bconcat!("Package: ", sa.package_name()));
                error.push(bconcat!("Severity: ", self.get_severity(sa)));
                error.push(bconcat!("Advisory ID: ", self.get_advisory_id(sa)));
                error.push(bconcat!("CVE: ", self.get_cve(sa)));
                error.push(bconcat!("Title: ", OutputFormatter::escape(&sa.title)?));
                error.push(bconcat!("URL: ", self.get_url(sa)));
                error.push(bconcat!(
                    "Affected versions: ",
                    OutputFormatter::escape(sa.affected_versions().get_pretty_string())?
                ));
                error.push(bconcat!(
                    "Reported at: ",
                    display_to_bytes(sa.reported_at.format(DATE_ATOM))
                ));
                if let Some(ignored) = advisory.as_ignored() {
                    error.push(bconcat!(
                        "Ignore reason: ",
                        ignored
                            .ignore_reason
                            .clone()
                            .unwrap_or_else(|| BString::from("None specified"))
                    ));
                }
                first_advisory = false;
            }
        }
        for line in &error {
            io.write_error(line);
        }

        Ok(())
    }

    fn output_abandoned_packages(
        &self,
        io: &std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        packages: &[CompletePackageInterfaceHandle],
        format: &BStr,
    ) -> anyhow::Result<()> {
        io.write_error(bconcat!(
            "<error>Found ",
            packages.len() as i64,
            " abandoned package",
            if packages.len() > 1 { "s" } else { "" },
            ":</error>"
        ));

        if format == Self::FORMAT_PLAIN {
            for pkg in packages {
                let replacement = if pkg.get_replacement_package().is_some() {
                    bconcat!("Use ", pkg.get_replacement_package().unwrap(), " instead")
                } else {
                    BString::from("No replacement was suggested")
                };
                io.write_error(bconcat!(
                    self.get_package_name_with_link(pkg.clone().into()),
                    " is abandoned. ",
                    replacement,
                    "."
                ));
            }

            return Ok(());
        }

        let io_ref = io.borrow();
        // PHP: `$io instanceof ConsoleIO`. BufferIO extends ConsoleIO in PHP but is modeled as
        // composition here, so also unwrap it to its inner ConsoleIO.
        let io_as_console = io_ref.as_any().downcast_ref::<ConsoleIO>().or_else(|| {
            io_ref
                .as_any()
                .downcast_ref::<crate::io::buffer_io::BufferIO>()
                .map(|buffer_io| &buffer_io.inner)
        });
        if io_as_console.is_none() {
            return Err(InvalidArgumentException::new(bconcat!(
                "Cannot use table format with ",
                get_class(&PhpMixed::Null)
            ))
            .into());
        }

        let mut table = io_as_console.unwrap().get_table();
        table
            .set_headers(vec![
                BString::from("Abandoned Package").into(),
                BString::from("Suggested Replacement").into(),
            ])
            .set_column_width(1, 80)
            .set_column_max_width(1, 80);

        for pkg in packages {
            let replacement = pkg
                .get_replacement_package()
                .unwrap_or_else(|| BString::from("none"));
            let row: Vec<Cell> = vec![
                ConsoleIO::sanitize(self.get_package_name_with_link(pkg.clone().into()), true)
                    .into(),
                ConsoleIO::sanitize(&replacement, true).into(),
            ];
            table.add_row(row.into());
        }

        table.render()?;

        Ok(())
    }

    fn get_package_name_with_link(&self, package: PackageInterfaceHandle) -> BString {
        let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());

        if let Some(package_url) = package_url {
            bconcat!(
                "<href=",
                OutputFormatter::escape(&package_url)
                    .expect("OutputFormatter::escape does not fail"),
                ">",
                package.get_pretty_name(),
                "</>"
            )
        } else {
            package.get_pretty_name()
        }
    }

    fn get_severity(&self, advisory: &SecurityAdvisory) -> BString {
        if advisory.severity.is_none() {
            return BString::default();
        }

        advisory.severity.clone().unwrap()
    }

    fn get_advisory_id(&self, advisory: &SecurityAdvisory) -> BString {
        let advisory_id = advisory.advisory_id();
        if advisory_id.starts_with(b"PKSA-") {
            return bconcat!(
                "<href=https://packagist.org/security-advisories/",
                advisory_id,
                ">",
                advisory_id,
                "</>"
            );
        }

        BString::from(advisory_id)
    }

    fn get_cve(&self, advisory: &SecurityAdvisory) -> BString {
        let Some(cve) = advisory.cve.as_ref() else {
            return BString::from("NO CVE");
        };

        bconcat!(
            "<href=https://cve.mitre.org/cgi-bin/cvename.cgi?name=",
            cve,
            ">",
            cve,
            "</>"
        )
    }

    fn get_url(&self, advisory: &SecurityAdvisory) -> BString {
        let Some(link) = advisory.link.as_ref() else {
            return BString::default();
        };

        let link = OutputFormatter::escape(link).expect("OutputFormatter::escape does not fail");
        bconcat!("<href=", link, ">", link, "</>")
    }

    fn calculate_bitmask(
        &self,
        has_vulnerable_packages: bool,
        has_abandoned_packages: bool,
    ) -> i64 {
        let mut bitmask: i64 = Self::STATUS_OK;

        if has_vulnerable_packages {
            bitmask |= Self::STATUS_VULNERABLE;
        }

        if has_abandoned_packages {
            bitmask |= Self::STATUS_ABANDONED;
        }

        bitmask
    }
}

#[derive(Debug)]
pub struct ProcessAdvisoriesResult {
    pub advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
    pub ignored_advisories: IndexMap<BString, Vec<std::rc::Rc<AnySecurityAdvisory>>>,
}
