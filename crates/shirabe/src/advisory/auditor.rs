//! ref: composer/src/Composer/Advisory/Auditor.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_php_shim::{
    DATE_ATOM, InvalidArgumentException, PhpMixed, array_all, array_any, array_key_exists,
    array_keys, array_reduce, get_class, sprintf, str_starts_with,
};

use crate::advisory::AnySecurityAdvisory;
use crate::advisory::SecurityAdvisory;
use crate::io::ConsoleIO;
use crate::io::IOInterface;
use crate::json::JsonFile;
use crate::package::CompletePackageInterfaceHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::base_package;
use crate::package::base_package::BasePackage;
use crate::repository::RepositorySet;
use crate::util::PackageInfo;

/// Shape of the `--format=json` audit output.
#[derive(serde::Serialize)]
struct AuditJsonReport<'a> {
    advisories: &'a IndexMap<String, Vec<AnySecurityAdvisory>>,
    #[serde(rename = "ignored-advisories", skip_serializing_if = "Option::is_none")]
    ignored_advisories: Option<&'a IndexMap<String, Vec<AnySecurityAdvisory>>>,
    #[serde(
        rename = "unreachable-repositories",
        skip_serializing_if = "Option::is_none"
    )]
    unreachable_repositories: Option<&'a Vec<String>>,
    abandoned: IndexMap<String, Option<String>>,
}

/// @internal
#[derive(Debug)]
pub struct Auditor;

impl Auditor {
    pub const FORMAT_TABLE: &'static str = "table";

    pub const FORMAT_PLAIN: &'static str = "plain";

    pub const FORMAT_JSON: &'static str = "json";

    pub const FORMAT_SUMMARY: &'static str = "summary";

    pub const FORMATS: [&'static str; 4] = [
        Self::FORMAT_TABLE,
        Self::FORMAT_PLAIN,
        Self::FORMAT_JSON,
        Self::FORMAT_SUMMARY,
    ];

    pub const ABANDONED_IGNORE: &'static str = "ignore";
    pub const ABANDONED_REPORT: &'static str = "report";
    pub const ABANDONED_FAIL: &'static str = "fail";

    /// @internal
    pub const ABANDONEDS: [&'static str; 3] = [
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
        io: &mut dyn IOInterface,
        repo_set: &RepositorySet,
        packages: Vec<PackageInterfaceHandle>,
        format: &str,
        warning_only: bool,
        ignore_list: IndexMap<String, Option<String>>,
        abandoned: &str,
        ignored_severities: IndexMap<String, Option<String>>,
        ignore_unreachable: bool,
        ignore_abandoned: IndexMap<String, Option<String>>,
    ) -> Result<i64> {
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
                |mut carry: IndexMap<String, Option<String>>,
                 package: &CompletePackageInterfaceHandle| {
                    carry.insert(package.get_pretty_name(), package.get_replacement_package());
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

            io.write(&JsonFile::encode(&report));

            return Ok(audit_bitmask);
        }

        let error_or_warn = if warning_only { "warning" } else { "error" };
        if affected_packages_count > 0 || !ignored_advisories.is_empty() {
            let passes: Vec<(
                &IndexMap<String, Vec<AnySecurityAdvisory>>,
                String,
            )> = vec![
                (
                    &ignored_advisories,
                    "<info>Found %d ignored security vulnerability advisor%s affecting %d package%s%s</info>"
                        .to_string(),
                ),
                (
                    &advisories,
                    format!(
                        "<{ew}>Found %d security vulnerability advisor%s affecting %d package%s%s</{ew}>",
                        ew = error_or_warn
                    ),
                ),
            ];
            for (advisories_to_output, message) in passes {
                let (pkg_count, total_advisory_count) = self.count_advisories(advisories_to_output);
                if pkg_count > 0 {
                    let plurality = if total_advisory_count == 1 {
                        "y"
                    } else {
                        "ies"
                    };
                    let pkg_plurality = if pkg_count == 1 { "" } else { "s" };
                    let punctuation = if format == "summary" { "." } else { ":" };
                    io.write_error(&sprintf(
                        &message,
                        &[
                            PhpMixed::Int(total_advisory_count),
                            PhpMixed::String(plurality.to_string()),
                            PhpMixed::Int(pkg_count),
                            PhpMixed::String(pkg_plurality.to_string()),
                            PhpMixed::String(punctuation.to_string()),
                        ],
                    ));
                    self.output_advisories(io, advisories_to_output, format)?;
                }
            }

            if format == Self::FORMAT_SUMMARY {
                io.write_error("Run \"composer audit\" for a full list of advisories.");
            }
        } else {
            io.write_error("<info>No security vulnerability advisories found.</info>");
        }

        if !unreachable_repos.is_empty() {
            io.write_error("<warning>The following repositories were unreachable:</warning>");
            for repo in &unreachable_repos {
                io.write_error(&format!("  - {}", repo));
            }
        }

        if !abandoned_packages.is_empty() && format != Self::FORMAT_SUMMARY {
            self.output_abandoned_packages(io, &abandoned_packages, format)?;
        }

        Ok(audit_bitmask)
    }

    /// @param array<string, array<SecurityAdvisory|AnySecurityAdvisory>> $advisories
    /// @param array<string, string|null> $ignoreList
    /// @return bool
    pub fn needs_complete_advisory_load(
        &self,
        advisories: &IndexMap<String, Vec<AnySecurityAdvisory>>,
        ignore_list: &IndexMap<String, Option<String>>,
    ) -> bool {
        if advisories.is_empty() {
            return false;
        }

        // no partial advisories present
        let advisories_values: Vec<&Vec<AnySecurityAdvisory>> = advisories.values().collect();
        if array_all(
            &advisories_values,
            |pkg_advisories: &&Vec<AnySecurityAdvisory>| {
                array_all(pkg_advisories, |advisory: &AnySecurityAdvisory| {
                    advisory.as_security_advisory().is_some()
                })
            },
        ) {
            return false;
        }

        let ignored_ids = array_keys(ignore_list);

        array_any(&ignored_ids, |id: &String| !str_starts_with(id, "PKSA-"))
    }

    /// @param array<PackageInterface> $packages
    /// @param array<string, string|null> $ignoreAbandoned
    /// @return array<CompletePackageInterface>
    pub fn filter_abandoned_packages(
        &self,
        packages: &[PackageInterfaceHandle],
        ignore_abandoned: &IndexMap<String, Option<String>>,
    ) -> anyhow::Result<Vec<CompletePackageInterfaceHandle>> {
        let mut filter: Option<String> = None;
        if !ignore_abandoned.is_empty() {
            filter = Some(base_package::package_names_to_regexp(
                &array_keys(ignore_abandoned),
                "{^(?:%s)$}iD",
            ));
        }

        // PHP: array_filter($packages, fn(PackageInterface $pkg) => $pkg instanceof CompletePackageInterface && $pkg->isAbandoned() && ($filter === null || !Preg::isMatch($filter, $pkg->getName())))
        let mut result: Vec<CompletePackageInterfaceHandle> = vec![];
        for pkg in packages {
            let Some(pkg) = pkg.as_complete() else {
                continue;
            };
            if pkg.is_abandoned()
                && (filter.is_none() || !Preg::is_match(filter.as_ref().unwrap(), &pkg.get_name()))
            {
                result.push(pkg);
            }
        }
        Ok(result)
    }

    pub fn process_advisories(
        &self,
        all_advisories: IndexMap<String, Vec<AnySecurityAdvisory>>,
        ignore_list: &IndexMap<String, Option<String>>,
        ignored_severities: &IndexMap<String, Option<String>>,
    ) -> ProcessAdvisoriesResult {
        if ignore_list.is_empty() && ignored_severities.is_empty() {
            return ProcessAdvisoriesResult {
                advisories: all_advisories,
                ignored_advisories: IndexMap::new(),
            };
        }

        let mut advisories: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
        let mut ignored: IndexMap<String, Vec<AnySecurityAdvisory>> = IndexMap::new();
        let mut ignore_reason: Option<String> = None;

        for (package, pkg_advisories) in all_advisories {
            for advisory in pkg_advisories {
                let mut is_active = true;

                if array_key_exists(&package, ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list.get(&package).cloned().unwrap_or(None);
                }

                if array_key_exists(advisory.advisory_id(), ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list
                        .get(advisory.advisory_id())
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
                            .or_else(|| Some(format!("{} severity is ignored", severity)));
                    }

                    if let Some(cve) = &full.cve
                        && array_key_exists(cve, ignore_list)
                    {
                        is_active = false;
                        ignore_reason = ignore_list.get(cve).cloned().flatten();
                    }

                    for source in &full.sources {
                        let remote_id = source.get("remoteId").cloned().unwrap_or_default();
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
                    AnySecurityAdvisory::Ignored(full.to_ignored_advisory(ignore_reason.clone()))
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

    /// @return array{int, int} Count of affected packages and total count of advisories
    fn count_advisories(
        &self,
        advisories: &IndexMap<String, Vec<AnySecurityAdvisory>>,
    ) -> (i64, i64) {
        let mut count: i64 = 0;
        for package_advisories in advisories.values() {
            count += package_advisories.len() as i64;
        }

        (advisories.len() as i64, count)
    }

    /// @param array<string, array<SecurityAdvisory>> $advisories
    /// @param self::FORMAT_* $format The format that will be used to output audit results.
    fn output_advisories(
        &self,
        io: &mut dyn IOInterface,
        advisories: &IndexMap<String, Vec<AnySecurityAdvisory>>,
        format: &str,
    ) -> Result<()> {
        match format {
            Self::FORMAT_TABLE => {
                let io_as_console = io.as_any().downcast_ref::<ConsoleIO>();
                if io_as_console.is_none() {
                    return Err(InvalidArgumentException {
                        message: format!(
                            "Cannot use table format with {}",
                            get_class(&PhpMixed::Null)
                        ),
                        code: 0,
                    }
                    .into());
                }
                self.output_advisories_table(io_as_console.unwrap(), advisories)?;

                Ok(())
            }
            Self::FORMAT_PLAIN => {
                self.output_advisories_plain(io, advisories)?;

                Ok(())
            }
            Self::FORMAT_SUMMARY => Ok(()),
            _ => Err(InvalidArgumentException {
                message: format!("Invalid format \"{}\".", format),
                code: 0,
            }
            .into()),
        }
    }

    /// @param array<string, array<SecurityAdvisory>> $advisories
    fn output_advisories_table(
        &self,
        io: &ConsoleIO,
        advisories: &IndexMap<String, Vec<AnySecurityAdvisory>>,
    ) -> Result<()> {
        for package_advisories in advisories.values() {
            for advisory in package_advisories {
                let mut headers: Vec<String> = vec![
                    "Package".to_string(),
                    "Severity".to_string(),
                    "Advisory ID".to_string(),
                    "CVE".to_string(),
                    "Title".to_string(),
                    "URL".to_string(),
                    "Affected versions".to_string(),
                    "Reported at".to_string(),
                ];
                let sa = advisory
                    .as_security_advisory()
                    .expect("output_advisories_table only receives full advisories");
                let mut row: Vec<String> = vec![
                    sa.package_name().to_string(),
                    self.get_severity(sa),
                    self.get_advisory_id(sa),
                    self.get_cve(sa),
                    sa.title.clone(),
                    self.get_url(sa),
                    sa.affected_versions().get_pretty_string(),
                    sa.reported_at.format(DATE_ATOM).to_string(),
                ];
                if let Some(ignored) = advisory.as_ignored() {
                    headers.push("Ignore reason".to_string());
                    row.push(
                        ignored
                            .ignore_reason
                            .clone()
                            .unwrap_or_else(|| "None specified".to_string()),
                    );
                }
                let mut table = io.get_table();
                table
                    .set_horizontal(true)
                    .set_headers(headers.into_iter().map(|h| h.into()).collect());
                table.add_row(ConsoleIO::sanitize(
                    PhpMixed::List(row.into_iter().map(PhpMixed::String).collect()),
                    false,
                ).into());
                table
                    .set_column_width(1, 80)
                    .set_column_max_width(1, 80)
                    .render();
            }
        }

        Ok(())
    }

    /// @param array<string, array<SecurityAdvisory>> $advisories
    fn output_advisories_plain(
        &self,
        io: &mut dyn IOInterface,
        advisories: &IndexMap<String, Vec<AnySecurityAdvisory>>,
    ) -> Result<()> {
        let mut error: Vec<String> = vec![];
        let mut first_advisory = true;
        for package_advisories in advisories.values() {
            for advisory in package_advisories {
                if !first_advisory {
                    error.push("--------".to_string());
                }
                let sa = advisory
                    .as_security_advisory()
                    .expect("output_advisories_plain only receives full advisories");
                error.push(format!("Package: {}", sa.package_name()));
                error.push(format!("Severity: {}", self.get_severity(sa)));
                error.push(format!("Advisory ID: {}", self.get_advisory_id(sa)));
                error.push(format!("CVE: {}", self.get_cve(sa)));
                error.push(format!("Title: {}", OutputFormatter::escape(&sa.title)?));
                error.push(format!("URL: {}", self.get_url(sa)));
                error.push(format!(
                    "Affected versions: {}",
                    OutputFormatter::escape(&sa.affected_versions().get_pretty_string())?
                ));
                error.push(format!("Reported at: {}", sa.reported_at.format(DATE_ATOM)));
                if let Some(ignored) = advisory.as_ignored() {
                    error.push(format!(
                        "Ignore reason: {}",
                        ignored
                            .ignore_reason
                            .clone()
                            .unwrap_or_else(|| "None specified".to_string())
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

    /// @param array<CompletePackageInterface> $packages
    /// @param self::FORMAT_PLAIN|self::FORMAT_TABLE $format
    fn output_abandoned_packages(
        &self,
        io: &mut dyn IOInterface,
        packages: &[CompletePackageInterfaceHandle],
        format: &str,
    ) -> Result<()> {
        io.write_error(&format!(
            "<error>Found {} abandoned package{}:</error>",
            packages.len() as i64,
            if packages.len() > 1 {
                "s".to_string()
            } else {
                String::new()
            },
        ));

        if format == Self::FORMAT_PLAIN {
            for pkg in packages {
                let replacement = if pkg.get_replacement_package().is_some() {
                    format!("Use {} instead", pkg.get_replacement_package().unwrap())
                } else {
                    "No replacement was suggested".to_string()
                };
                io.write_error(&format!(
                    "{} is abandoned. {}.",
                    self.get_package_name_with_link(pkg.clone().into()),
                    replacement,
                ));
            }

            return Ok(());
        }

        let io_as_console = io.as_any().downcast_ref::<ConsoleIO>();
        if io_as_console.is_none() {
            return Err(InvalidArgumentException {
                message: format!(
                    "Cannot use table format with {}",
                    get_class(&PhpMixed::Null)
                ),
                code: 0,
            }
            .into());
        }

        let mut table = io_as_console.unwrap().get_table();
        table
            .set_headers(vec![
                "Abandoned Package".to_string().into(),
                "Suggested Replacement".to_string().into(),
            ])
            .set_column_width(1, 80)
            .set_column_max_width(1, 80);

        for pkg in packages {
            let replacement = if pkg.get_replacement_package().is_some() {
                pkg.get_replacement_package().unwrap().to_string()
            } else {
                "none".to_string()
            };
            table.add_row(ConsoleIO::sanitize(
                PhpMixed::List(vec![
                    PhpMixed::String(self.get_package_name_with_link(pkg.clone().into())),
                    PhpMixed::String(replacement),
                ]),
                false,
            ).into());
        }

        table.render();

        Ok(())
    }

    fn get_package_name_with_link(&self, package: PackageInterfaceHandle) -> String {
        let package_url = PackageInfo::get_view_source_or_homepage_url(package.clone());

        if let Some(package_url) = package_url {
            format!(
                "<href={}>{}</>",
                OutputFormatter::escape(&package_url)
                    .expect("OutputFormatter::escape does not fail"),
                package.get_pretty_name()
            )
        } else {
            package.get_pretty_name()
        }
    }

    fn get_severity(&self, advisory: &SecurityAdvisory) -> String {
        if advisory.severity.is_none() {
            return String::new();
        }

        advisory.severity.clone().unwrap()
    }

    fn get_advisory_id(&self, advisory: &SecurityAdvisory) -> String {
        let advisory_id = advisory.advisory_id();
        if str_starts_with(advisory_id, "PKSA-") {
            return format!(
                "<href=https://packagist.org/security-advisories/{}>{}</>",
                advisory_id, advisory_id
            );
        }

        advisory_id.to_string()
    }

    fn get_cve(&self, advisory: &SecurityAdvisory) -> String {
        if advisory.cve.is_none() {
            return "NO CVE".to_string();
        }

        format!(
            "<href=https://cve.mitre.org/cgi-bin/cvename.cgi?name={}>{}</>",
            advisory.cve.as_ref().unwrap(),
            advisory.cve.as_ref().unwrap()
        )
    }

    fn get_url(&self, advisory: &SecurityAdvisory) -> String {
        if advisory.link.is_none() {
            return String::new();
        }

        let link = advisory.link.as_ref().unwrap();
        format!(
            "<href={}>{}</>",
            OutputFormatter::escape(link).expect("OutputFormatter::escape does not fail"),
            OutputFormatter::escape(link).expect("OutputFormatter::escape does not fail")
        )
    }

    /// @return int-mask<self::STATUS_*>
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
    pub advisories: IndexMap<String, Vec<AnySecurityAdvisory>>,
    pub ignored_advisories: IndexMap<String, Vec<AnySecurityAdvisory>>,
}
