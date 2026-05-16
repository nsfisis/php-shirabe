//! ref: composer/src/Composer/Advisory/Auditor.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_php_shim::{
    array_all, array_any, array_key_exists, array_keys, array_reduce, get_class, is_string,
    sprintf, str_starts_with, InvalidArgumentException, PhpMixed, DATE_ATOM,
};

use crate::advisory::ignored_security_advisory::IgnoredSecurityAdvisory;
use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;
use crate::advisory::security_advisory::SecurityAdvisory;
use crate::io::console_io::ConsoleIO;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::base_package::BasePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::repository_set::RepositorySet;
use crate::util::package_info::PackageInfo;

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

    /// @param PackageInterface[] $packages
    /// @param self::FORMAT_* $format The format that will be used to output audit results.
    /// @param bool $warningOnly If true, outputs a warning. If false, outputs an error.
    /// @param array<string, string|null> $ignoreList List of advisory IDs, remote IDs, CVE IDs or package names that reported but not listed as vulnerabilities.
    /// @param self::ABANDONED_* $abandoned
    /// @param array<string, string|null> $ignoredSeverities List of ignored severity levels
    /// @param array<string, string|null> $ignoreAbandoned List of abandoned package name that reported but not listed as vulnerabilities.
    ///
    /// @return int-mask<self::STATUS_*> A bitmask of STATUS_* constants or 0 on success
    /// @throws InvalidArgumentException If no packages are passed in
    pub fn audit(
        &self,
        io: &dyn IOInterface,
        repo_set: &RepositorySet,
        packages: Vec<Box<dyn PackageInterface>>,
        format: &str,
        warning_only: bool,
        ignore_list: IndexMap<String, Option<String>>,
        abandoned: &str,
        ignored_severities: IndexMap<String, Option<String>>,
        ignore_unreachable: bool,
        ignore_abandoned: IndexMap<String, Option<String>>,
    ) -> Result<i64> {
        // TODO(phase-b): packages is moved into get_matching_security_advisories; PHP keeps the
        // original $packages alive — needs cloning/borrowing strategy for trait objects
        let result = repo_set.get_matching_security_advisories(
            packages,
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
            // TODO(phase-b): $packages reused here; see note above
            let result = repo_set.get_matching_security_advisories(
                vec![],
                false,
                ignore_unreachable,
            )?;
            all_advisories = result.advisories;
            unreachable_repos.extend(result.unreachable_repos);
        }
        let processed =
            self.process_advisories(all_advisories, &ignore_list, &ignored_severities);
        let advisories = processed.advisories;
        let ignored_advisories = processed.ignored_advisories;

        let mut abandoned_count: i64 = 0;
        let affected_packages_count = advisories.len() as i64;
        let abandoned_packages: Vec<Box<dyn CompletePackageInterface>>;
        if abandoned == Self::ABANDONED_IGNORE {
            abandoned_packages = vec![];
        } else {
            // TODO(phase-b): $packages reused here; see note above
            abandoned_packages = self.filter_abandoned_packages(&[], &ignore_abandoned);
            if abandoned == Self::ABANDONED_FAIL {
                abandoned_count = abandoned_packages.len() as i64;
            }
        }

        let audit_bitmask =
            self.calculate_bitmask(0 < affected_packages_count, 0 < abandoned_count);

        if Self::FORMAT_JSON == format {
            let mut json: IndexMap<String, PhpMixed> = IndexMap::new();
            // TODO(phase-b): serialize advisories / ignored_advisories into PhpMixed
            json.insert("advisories".to_string(), PhpMixed::Null);
            if !ignored_advisories.is_empty() {
                json.insert("ignored-advisories".to_string(), PhpMixed::Null);
            }
            if !unreachable_repos.is_empty() {
                json.insert(
                    "unreachable-repositories".to_string(),
                    PhpMixed::List(
                        unreachable_repos
                            .iter()
                            .map(|r| Box::new(PhpMixed::String(r.clone())))
                            .collect(),
                    ),
                );
            }
            let abandoned_map = array_reduce(
                &abandoned_packages,
                |mut carry: IndexMap<String, Option<String>>,
                 package: &Box<dyn CompletePackageInterface>| {
                    carry.insert(
                        package.get_pretty_name().to_string(),
                        package.get_replacement_package().map(|s| s.to_string()),
                    );
                    carry
                },
                IndexMap::new(),
            );
            json.insert(
                "abandoned".to_string(),
                PhpMixed::Array(
                    abandoned_map
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                Box::new(match v {
                                    Some(s) => PhpMixed::String(s),
                                    None => PhpMixed::Null,
                                }),
                            )
                        })
                        .collect(),
                ),
            );

            io.write(
                PhpMixed::String(JsonFile::encode(
                    &PhpMixed::Array(
                        json.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    ),
                    shirabe_php_shim::JSON_UNESCAPED_SLASHES
                        | shirabe_php_shim::JSON_PRETTY_PRINT
                        | shirabe_php_shim::JSON_UNESCAPED_UNICODE,
                    JsonFile::INDENT_DEFAULT,
                )),
                true,
                IOInterface::NORMAL,
            );

            return Ok(audit_bitmask);
        }

        let error_or_warn = if warning_only { "warning" } else { "error" };
        if affected_packages_count > 0 || ignored_advisories.len() > 0 {
            let passes: Vec<(
                &IndexMap<String, Vec<PartialSecurityAdvisory>>,
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
                let (pkg_count, total_advisory_count) =
                    self.count_advisories(advisories_to_output);
                if pkg_count > 0 {
                    let plurality = if total_advisory_count == 1 { "y" } else { "ies" };
                    let pkg_plurality = if pkg_count == 1 { "" } else { "s" };
                    let punctuation = if format == "summary" { "." } else { ":" };
                    io.write_error(
                        PhpMixed::String(sprintf(
                            &message,
                            &[
                                PhpMixed::Int(total_advisory_count),
                                PhpMixed::String(plurality.to_string()),
                                PhpMixed::Int(pkg_count),
                                PhpMixed::String(pkg_plurality.to_string()),
                                PhpMixed::String(punctuation.to_string()),
                            ],
                        )),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.output_advisories(io, advisories_to_output, format)?;
                }
            }

            if format == Self::FORMAT_SUMMARY {
                io.write_error(
                    PhpMixed::String(
                        "Run \"composer audit\" for a full list of advisories.".to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );
            }
        } else {
            io.write_error(
                PhpMixed::String(
                    "<info>No security vulnerability advisories found.</info>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }

        if !unreachable_repos.is_empty() {
            io.write_error(
                PhpMixed::String(
                    "<warning>The following repositories were unreachable:</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
            for repo in &unreachable_repos {
                io.write_error(
                    PhpMixed::String(format!("  - {}", repo)),
                    true,
                    IOInterface::NORMAL,
                );
            }
        }

        if !abandoned_packages.is_empty() && format != Self::FORMAT_SUMMARY {
            self.output_abandoned_packages(io, &abandoned_packages, format)?;
        }

        Ok(audit_bitmask)
    }

    /// @param array<string, array<SecurityAdvisory|PartialSecurityAdvisory>> $advisories
    /// @param array<string, string|null> $ignoreList
    /// @return bool
    pub fn needs_complete_advisory_load(
        &self,
        advisories: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
        ignore_list: &IndexMap<String, Option<String>>,
    ) -> bool {
        if advisories.len() == 0 {
            return false;
        }

        // no partial advisories present
        let advisories_values: Vec<&Vec<PartialSecurityAdvisory>> =
            advisories.values().collect();
        if array_all(
            &advisories_values,
            |pkg_advisories: &&Vec<PartialSecurityAdvisory>| {
                array_all(pkg_advisories, |_advisory: &PartialSecurityAdvisory| {
                    // TODO(phase-b): `$advisory instanceof SecurityAdvisory` — needs an advisory
                    // enum or trait downcast; SecurityAdvisoriesResult currently only holds
                    // PartialSecurityAdvisory so this is hard-coded to false
                    false
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
        packages: &[Box<dyn PackageInterface>],
        ignore_abandoned: &IndexMap<String, Option<String>>,
    ) -> Vec<Box<dyn CompletePackageInterface>> {
        let mut filter: Option<String> = None;
        if ignore_abandoned.len() != 0 {
            filter = Some(BasePackage::package_names_to_regexp(
                &array_keys(ignore_abandoned),
                "{^(?:%s)$}iD",
            ));
        }

        // PHP: array_filter($packages, fn($pkg) => $pkg instanceof CompletePackageInterface && $pkg->isAbandoned() && ($filter === null || !Preg::isMatch($filter, $pkg->getName())))
        // TODO(phase-b): downcast Box<dyn PackageInterface> -> Box<dyn CompletePackageInterface>
        let _ = packages;
        let _ = filter;
        let _ = |pkg: &Box<dyn PackageInterface>| -> bool {
            // pkg instanceof CompletePackageInterface && pkg.is_abandoned() && (filter.is_none() || !Preg::is_match(filter.as_ref().unwrap(), pkg.get_name()))
            let _ = Preg::is_match("", "");
            false
        };
        vec![]
    }

    /// @phpstan-param array<string, array<PartialSecurityAdvisory|SecurityAdvisory>> $allAdvisories
    /// @param array<string, string|null> $ignoreList List of advisory IDs, remote IDs, CVE IDs or package names that reported but not listed as vulnerabilities.
    /// @param array<string, string|null> $ignoredSeverities List of ignored severity levels
    /// @phpstan-return array{advisories: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>, ignoredAdvisories: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>}
    pub fn process_advisories(
        &self,
        all_advisories: IndexMap<String, Vec<PartialSecurityAdvisory>>,
        ignore_list: &IndexMap<String, Option<String>>,
        ignored_severities: &IndexMap<String, Option<String>>,
    ) -> ProcessAdvisoriesResult {
        if ignore_list.is_empty() && ignored_severities.is_empty() {
            return ProcessAdvisoriesResult {
                advisories: all_advisories,
                ignored_advisories: IndexMap::new(),
            };
        }

        let mut advisories: IndexMap<String, Vec<PartialSecurityAdvisory>> = IndexMap::new();
        let mut ignored: IndexMap<String, Vec<PartialSecurityAdvisory>> = IndexMap::new();
        let mut ignore_reason: Option<String> = None;

        for (package, pkg_advisories) in all_advisories {
            for advisory in pkg_advisories {
                let mut is_active = true;

                if array_key_exists(&package, ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list.get(&package).cloned().unwrap_or(None);
                }

                if array_key_exists(&advisory.advisory_id, ignore_list) {
                    is_active = false;
                    ignore_reason = ignore_list
                        .get(&advisory.advisory_id)
                        .cloned()
                        .unwrap_or(None);
                }

                // TODO(phase-b): `$advisory instanceof SecurityAdvisory` — needs an advisory enum
                // or trait downcast; the block below is skipped while SecurityAdvisoriesResult
                // only holds PartialSecurityAdvisory
                let advisory_as_full: Option<&SecurityAdvisory> = None;
                if let Some(full) = advisory_as_full {
                    if is_string(&PhpMixed::String(
                        full.severity.clone().unwrap_or_default(),
                    )) && array_key_exists(
                        full.severity.as_deref().unwrap_or(""),
                        ignored_severities,
                    ) {
                        is_active = false;
                        let sev = full.severity.as_deref().unwrap_or("");
                        ignore_reason = ignored_severities
                            .get(sev)
                            .cloned()
                            .unwrap_or_else(|| Some(format!("{} severity is ignored", sev)));
                    }

                    if is_string(&PhpMixed::String(full.cve.clone().unwrap_or_default()))
                        && array_key_exists(
                            full.cve.as_deref().unwrap_or(""),
                            ignore_list,
                        )
                    {
                        is_active = false;
                        ignore_reason = ignore_list
                            .get(full.cve.as_deref().unwrap_or(""))
                            .cloned()
                            .unwrap_or(None);
                    }

                    for source in &full.sources {
                        let remote_id = source.get("remoteId").cloned().unwrap_or_default();
                        if array_key_exists(&remote_id, ignore_list) {
                            is_active = false;
                            ignore_reason =
                                ignore_list.get(&remote_id).cloned().unwrap_or(None);
                            break;
                        }
                    }
                }

                if is_active {
                    advisories
                        .entry(package.clone())
                        .or_insert_with(Vec::new)
                        .push(advisory);
                    continue;
                }

                // Partial security advisories only used in summary mode
                // and in that case we do not need to cast the object.
                // TODO(phase-b): `$advisory instanceof SecurityAdvisory` -> $advisory->toIgnoredAdvisory($ignoreReason)
                let _: Option<IgnoredSecurityAdvisory> = None;
                let _ = &ignore_reason;

                ignored
                    .entry(package.clone())
                    .or_insert_with(Vec::new)
                    .push(advisory);
            }
        }

        ProcessAdvisoriesResult {
            advisories,
            ignored_advisories: ignored,
        }
    }

    /// @param array<string, array<PartialSecurityAdvisory>> $advisories
    /// @return array{int, int} Count of affected packages and total count of advisories
    fn count_advisories(
        &self,
        advisories: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
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
        io: &dyn IOInterface,
        advisories: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
        format: &str,
    ) -> Result<()> {
        match format {
            Self::FORMAT_TABLE => {
                // TODO(phase-b): `$io instanceof ConsoleIO` downcast from &dyn IOInterface
                let io_as_console: Option<&ConsoleIO> = None;
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
                self.output_advisories_table(io_as_console.unwrap(), advisories);

                Ok(())
            }
            Self::FORMAT_PLAIN => {
                self.output_advisories_plain(io, advisories);

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
        advisories: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
    ) {
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
                // TODO(phase-b): advisory typed PartialSecurityAdvisory; PHP accesses
                // SecurityAdvisory fields (title, link, reportedAt, etc.)
                let _ = advisory;
                let row: Vec<String> = vec![
                    /* advisory.packageName */ String::new(),
                    /* self.get_severity(advisory) */ String::new(),
                    /* self.get_advisory_id(advisory) */ String::new(),
                    /* self.get_cve(advisory) */ String::new(),
                    /* advisory.title */ String::new(),
                    /* self.get_url(advisory) */ String::new(),
                    /* advisory.affectedVersions.getPrettyString() */ String::new(),
                    /* advisory.reportedAt.format(DATE_ATOM) */ String::new(),
                ];
                let _ = DATE_ATOM;
                // TODO(phase-b): `$advisory instanceof IgnoredSecurityAdvisory` downcast
                let advisory_as_ignored: Option<&IgnoredSecurityAdvisory> = None;
                if let Some(_ignored) = advisory_as_ignored {
                    headers.push("Ignore reason".to_string());
                    // row.push(ignored.ignore_reason.clone().unwrap_or_else(|| "None specified".to_string()));
                }
                let _ = row;
                io.get_table()
                    .set_horizontal()
                    .set_headers(headers)
                    .add_row(ConsoleIO::sanitize(PhpMixed::Null, false))
                    .set_column_width(1, 80)
                    .set_column_max_width(1, 80)
                    .render();
            }
        }
    }

    /// @param array<string, array<SecurityAdvisory>> $advisories
    fn output_advisories_plain(
        &self,
        io: &dyn IOInterface,
        advisories: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
    ) {
        let mut error: Vec<String> = vec![];
        let mut first_advisory = true;
        for package_advisories in advisories.values() {
            for advisory in package_advisories {
                if !first_advisory {
                    error.push("--------".to_string());
                }
                // TODO(phase-b): advisory typed PartialSecurityAdvisory; PHP accesses
                // SecurityAdvisory fields
                let _ = advisory;
                error.push(format!("Package: {}", /* advisory.packageName */ ""));
                error.push(format!(
                    "Severity: {}",
                    /* self.get_severity(advisory) */ ""
                ));
                error.push(format!(
                    "Advisory ID: {}",
                    /* self.get_advisory_id(advisory) */ ""
                ));
                error.push(format!("CVE: {}", /* self.get_cve(advisory) */ ""));
                error.push(format!(
                    "Title: {}",
                    OutputFormatter::escape(/* advisory.title */ "")
                ));
                error.push(format!("URL: {}", /* self.get_url(advisory) */ ""));
                error.push(format!(
                    "Affected versions: {}",
                    OutputFormatter::escape(/* advisory.affectedVersions.getPrettyString() */ "")
                ));
                error.push(format!(
                    "Reported at: {}",
                    /* advisory.reportedAt.format(DATE_ATOM) */ ""
                ));
                // TODO(phase-b): `$advisory instanceof IgnoredSecurityAdvisory` downcast
                let advisory_as_ignored: Option<&IgnoredSecurityAdvisory> = None;
                if let Some(_ignored) = advisory_as_ignored {
                    error.push(format!(
                        "Ignore reason: {}",
                        /* ignored.ignore_reason.unwrap_or("None specified") */ ""
                    ));
                }
                first_advisory = false;
            }
        }
        io.write_error(
            PhpMixed::List(
                error
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ),
            true,
            IOInterface::NORMAL,
        );
    }

    /// @param array<CompletePackageInterface> $packages
    /// @param self::FORMAT_PLAIN|self::FORMAT_TABLE $format
    fn output_abandoned_packages(
        &self,
        io: &dyn IOInterface,
        packages: &[Box<dyn CompletePackageInterface>],
        format: &str,
    ) -> Result<()> {
        io.write_error(
            PhpMixed::String(sprintf(
                "<error>Found %d abandoned package%s:</error>",
                &[
                    PhpMixed::Int(packages.len() as i64),
                    PhpMixed::String(if packages.len() > 1 {
                        "s".to_string()
                    } else {
                        String::new()
                    }),
                ],
            )),
            true,
            IOInterface::NORMAL,
        );

        if format == Self::FORMAT_PLAIN {
            for pkg in packages {
                let replacement = if pkg.get_replacement_package().is_some() {
                    format!("Use {} instead", pkg.get_replacement_package().unwrap())
                } else {
                    "No replacement was suggested".to_string()
                };
                io.write_error(
                    PhpMixed::String(sprintf(
                        "%s is abandoned. %s.",
                        &[
                            PhpMixed::String(self.get_package_name_with_link_for_complete(pkg)),
                            PhpMixed::String(replacement),
                        ],
                    )),
                    true,
                    IOInterface::NORMAL,
                );
            }

            return Ok(());
        }

        // TODO(phase-b): `$io instanceof ConsoleIO` downcast from &dyn IOInterface
        let io_as_console: Option<&ConsoleIO> = None;
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

        let table = io_as_console
            .unwrap()
            .get_table()
            .set_headers(vec![
                "Abandoned Package".to_string(),
                "Suggested Replacement".to_string(),
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
                    Box::new(PhpMixed::String(
                        self.get_package_name_with_link_for_complete(pkg),
                    )),
                    Box::new(PhpMixed::String(replacement)),
                ]),
                false,
            ));
        }

        table.render();

        Ok(())
    }

    fn get_package_name_with_link(&self, package: &dyn PackageInterface) -> String {
        let package_url = PackageInfo::get_view_source_or_homepage_url(package);

        if package_url.is_some() {
            format!(
                "<href={}>{}</>",
                OutputFormatter::escape(&package_url.unwrap()),
                package.get_pretty_name()
            )
        } else {
            package.get_pretty_name().to_string()
        }
    }

    // TODO(phase-b): merge with get_package_name_with_link once CompletePackageInterface can be
    // upcast to PackageInterface (e.g. via an as_package_interface() trait method)
    fn get_package_name_with_link_for_complete(
        &self,
        package: &Box<dyn CompletePackageInterface>,
    ) -> String {
        let _ = package;
        // PackageInfo::get_view_source_or_homepage_url(package as &dyn PackageInterface)
        String::new()
    }

    fn get_severity(&self, advisory: &SecurityAdvisory) -> String {
        if advisory.severity.is_none() {
            return String::new();
        }

        advisory.severity.clone().unwrap()
    }

    fn get_advisory_id(&self, advisory: &SecurityAdvisory) -> String {
        // TODO(phase-b): advisory.advisory_id lives on inner PartialSecurityAdvisory
        let advisory_id: &str = "";
        let _ = advisory;
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
            OutputFormatter::escape(link),
            OutputFormatter::escape(link)
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
    pub advisories: IndexMap<String, Vec<PartialSecurityAdvisory>>,
    pub ignored_advisories: IndexMap<String, Vec<PartialSecurityAdvisory>>,
}
