//! ref: composer/src/Composer/Command/ShowCommand.php

use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData, CompletionTrait};
use crate::composer::PartialComposerHandle;
use crate::console::input::{InputArgument, InputOption, SuggestedValues};
use crate::dependency_resolver::{DefaultPolicy, PolicyInterface};
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::{IOInterfaceImmutable, IOInterfaceWrite};
use crate::json::JsonFile;
use crate::package::version::version_selector::ShowWarnings;
use crate::package::version::{VersionParser, VersionSelector};
use crate::package::{CompletePackageInterfaceHandle, Link, PackageInterfaceHandle, base_package};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{
    ComposerRepository, CompositeRepository, FilterRepository, InstalledArrayRepository,
    InstalledRepository, PlatformRepository, PlatformRepositoryHandle, RepositoryFactory,
    RepositoryInterface, RepositoryInterfaceHandle, RepositorySet, RepositoryUtils,
    RootPackageRepository,
};
use crate::util::PackageInfo;
use bstr::{BStr, BString, ByteSlice, ByteVec};
use indexmap::IndexMap;
use shirabe_php_shim::{
    CmpOp, DATE_ATOM, InvalidArgumentException, LogicException, PhpMixed, STR_PAD_RIGHT,
    UnexpectedValueException, array_search, bconcat, date_format_to_strftime, date_local,
    display_to_bytes, extension_loaded, get_class, impl_php_class, implode, in_array_loose,
    in_array_strict, intval, json_bytes, ltrim2, path_to_bytes, php_regex, preg_is_match,
    preg_match, preg_quote, preg_replace, realpath, rtrim, str_pad, str_replace, strtolower,
    version_compare,
};
use shirabe_semver::Semver;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_spdx_licenses::SpdxLicenses;
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::formatter::{OutputFormatter, OutputFormatterStyle};
use shirabe_symfony_console::input::{InputInterface, InputInterfaceByName, InputValue};
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct ShowCommand {
    base_command_data: BaseCommandData,

    version_parser: std::cell::RefCell<VersionParser>,
    colors: std::cell::RefCell<Vec<BString>>,
    repository_set: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<RepositorySet>>>>,
}

impl_php_class!(ShowCommand, r"Composer\Command\ShowCommand");

impl Default for ShowCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ShowCommand {
    pub fn new() -> Self {
        let command = ShowCommand {
            base_command_data: BaseCommandData::new(None),
            version_parser: std::cell::RefCell::new(VersionParser::new()),
            colors: std::cell::RefCell::new(Vec::new()),
            repository_set: std::cell::RefCell::new(None),
        };
        command
            .configure()
            .expect("ShowCommand::configure uses static, valid metadata");
        command
    }

    /// PHP: protected function suggestPackageBasedOnMode(): \Closure
    fn suggest_package_based_on_mode(&self) -> crate::console::input::SuggestedValues {
        crate::console::input::SuggestedValues::Closure(Box::new(|this, input, suggestions| {
            if input.get_option("available")?.to_bool() || input.get_option("all")?.to_bool() {
                return this.suggest_available_package_incl_platform().call(
                    this,
                    input,
                    suggestions,
                );
            }

            if input.get_option("platform")?.to_bool() {
                return this
                    .suggest_platform_package()
                    .call(this, input, suggestions);
            }

            this.suggest_installed_package(false, false)
                .call(this, input, suggestions)
        }))
    }

    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    fn print_packages(
        &self,
        packages: &[PackageViewData],
        indent: &BStr,
        write_version: bool,
        write_latest: bool,
        write_description: bool,
        width: usize,
        version_length: usize,
        name_length: usize,
        latest_length: usize,
        write_release_date: bool,
        release_date_length: usize,
    ) {
        let io = self.get_io();
        let pad_name = write_version || write_latest || write_release_date || write_description;
        let pad_version = write_latest || write_release_date || write_description;
        let pad_latest = write_description || write_release_date;
        let pad_release_date = write_description;
        for package in packages.iter() {
            let link = package
                .source
                .clone()
                .flatten()
                .or_else(|| package.homepage.clone().flatten())
                .unwrap_or_default();
            let name = &package.name;
            if !link.is_empty() {
                let pad = if pad_name && name_length > name.len() {
                    name_length - name.len()
                } else {
                    0
                };
                io.write_no_newline(bconcat!(
                    indent,
                    "<href=",
                    OutputFormatter::escape(&link).expect("OutputFormatter::escape failed"),
                    ">",
                    name,
                    "</>",
                    " ".repeat(pad)
                ));
            } else {
                let width_pad = if pad_name { name_length } else { 0 };
                io.write_no_newline(bconcat!(
                    indent,
                    str_pad(name, width_pad, " ", STR_PAD_RIGHT)
                ));
            }
            if let Some(version) = &package.version
                && write_version
            {
                let width_pad = if pad_version { version_length } else { 0 };
                io.write_no_newline(bconcat!(
                    " ",
                    str_pad(version, width_pad, " ", STR_PAD_RIGHT)
                ));
            }
            if let (Some(latest_version), Some(update_status)) =
                (&package.latest, &package.latest_status)
                && write_latest
            {
                let mut latest_version = latest_version.clone();
                let style = Self::update_status_to_version_style(update_status.as_bstr());
                if !io.is_decorated() {
                    let marker = update_status
                        .replace("up-to-date", "=")
                        .replace("semver-safe-update", "!")
                        .replace("update-possible", "~");
                    latest_version = bconcat!(marker, " ", latest_version);
                }
                let width_pad = if pad_latest { latest_length } else { 0 };
                io.write_no_newline(bconcat!(
                    " <",
                    style,
                    ">",
                    str_pad(&latest_version, width_pad, " ", STR_PAD_RIGHT),
                    "</",
                    style,
                    ">"
                ));
                if write_release_date && let Some(age) = &package.release_age {
                    let width_pad = if pad_release_date {
                        release_date_length
                    } else {
                        0
                    };
                    io.write_no_newline(bconcat!(" ", str_pad(age, width_pad, " ", STR_PAD_RIGHT)));
                }
            }
            if let Some(Some(description)) = &package.description
                && write_description
            {
                let mut description = BString::from(first_token(description));

                // Compute remaining width available for the description.
                let mut remaining = (width as i64)
                    - (name_length as i64)
                    - (version_length as i64)
                    - (release_date_length as i64)
                    - 4;
                if write_latest {
                    remaining -= latest_length as i64;
                }

                // If nothing fits, clear the description.
                if remaining <= 0 {
                    description = BString::default();
                } else if extension_loaded("mbstring") {
                    // Use mb_strwidth/mb_strimwidth to measure and trim by display width
                    // (CJK characters count as width 2). mb_strimwidth counts the trim
                    // marker ('...') in the width parameter, so pass $remaining directly.
                    // TODO(unicode): this counts code points, not display width.
                    if description.chars().count() > remaining as usize {
                        let end = description
                            .char_indices()
                            .nth((remaining as usize).saturating_sub(3))
                            .map_or(description.len(), |(start, _, _)| start);
                        description = bconcat!(&description[..end], "...");
                    }
                } else {
                    // Fallback when mbstring is not available: do a conservative byte-based cut.
                    // Ensure cut length is non-negative and leave room for the ellipsis.
                    let cut = (remaining - 3).max(0) as usize;
                    if description.len() > cut {
                        description = bconcat!(&description[..cut], "...");
                    }
                }

                io.write_no_newline(bconcat!(" ", description));
            }
            if let Some(path) = &package.path {
                let path_str = match path {
                    Some(s) => s.clone(),
                    None => BString::from("null"),
                };
                io.write_no_newline(bconcat!(" ", path_str));
            }
            io.write("");
            if let Some(warning) = &package.warning {
                io.write(bconcat!("<warning>", warning, "</warning>"));
            }
        }
    }

    fn get_root_requires(&self) -> Vec<BString> {
        let composer_rc = self.try_composer(None, None);
        let composer_rc = match composer_rc {
            None => return vec![],
            Some(c) => c,
        };
        let composer = crate::composer::composer_full(&composer_rc);

        let root_package = composer.get_package();

        let mut combined: IndexMap<BString, Link> = IndexMap::new();
        for (k, v) in root_package.get_requires().iter() {
            combined.insert(k.clone(), v.clone());
        }
        for (k, v) in root_package.get_dev_requires().iter() {
            combined.insert(k.clone(), v.clone());
        }
        combined.keys().map(strtolower).collect()
    }

    fn get_version_style(
        &self,
        latest_package: PackageInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<BString> {
        Ok(Self::update_status_to_version_style(
            Self::get_update_status(latest_package, package)?.as_bstr(),
        )
        .to_owned())
    }

    /// finds a package by name and version if provided
    fn get_package(
        &self,
        installed_repo: &RepositoryInterfaceHandle,
        repos: &RepositoryInterfaceHandle,
        name: &BStr,
        version: PhpMixed,
    ) -> anyhow::Result<(
        Option<crate::package::CompletePackageInterfaceHandle>,
        IndexMap<BString, BString>,
    )> {
        let name = strtolower(name);
        let constraint: Option<AnyConstraint> = match &version {
            PhpMixed::String(s) => Some(self.version_parser.borrow().parse_constraints(s)?),
            PhpMixed::Null => None,
            _ => None, // already a ConstraintInterface
        };

        let policy = DefaultPolicy::new(false, false, None);
        let mut repository_set = RepositorySet::new(
            "dev",
            IndexMap::new(),
            Vec::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        repository_set.allow_installed_repositories(true);
        repository_set.add_repository(repos.clone())?;

        let mut matched_package: Option<crate::package::PackageInterfaceHandle> = None;
        let mut versions: IndexMap<BString, BString> = IndexMap::new();
        let mut pool = if PlatformRepository::is_platform_package(&name) {
            repository_set.create_pool_with_all_packages()?
        } else {
            repository_set.create_pool_for_package(bstr::BStr::new(&name), None)?
        };
        let matches = pool.what_provides(bstr::BStr::new(&name), constraint.as_ref());
        let mut literals: Vec<i64> = Vec::new();
        for package in matches.iter() {
            // avoid showing the 9999999-dev alias if the default branch has no branch-alias set
            let mut p: crate::package::PackageInterfaceHandle = package.clone();
            if let Some(alias) = p.as_alias()
                && p.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS
            {
                p = alias.get_alias_of().into();
            }

            // select an exact match if it is in the installed repo and no specific version was required
            if version.is_null() && installed_repo.has_package(p.clone())? {
                matched_package = Some(p.clone());
            }

            versions.insert(p.get_pretty_version(), p.get_version());
            literals.push(p.get_id());
        }

        // select preferred package according to policy rules
        if matched_package.is_none() && !literals.is_empty() {
            let preferred = policy.select_preferred_packages(&pool, literals.clone(), None);
            matched_package = Some(pool.literal_to_package(preferred[0]));
        }

        if let Some(ref mp) = matched_package
            && mp.as_complete().is_none()
        {
            return Err(LogicException::new(bconcat!(
                "ShowCommand::getPackage can only work with CompletePackageInterface, but got ",
                get_class(&PhpMixed::Null)
            ))
            .into());
        }

        let matched_package = matched_package.and_then(|mp| mp.as_complete());
        Ok((matched_package, versions))
    }

    /// Prints package info.
    fn print_package_info(
        &self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<BString, BString>,
        installed_repo: &mut dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        self.print_meta(package.clone(), versions, installed_repo, latest_package)?;
        self.print_links(package.clone(), Link::TYPE_REQUIRE, None);
        self.print_links(
            package.clone(),
            Link::TYPE_DEV_REQUIRE,
            Some(BStr::new("requires (dev)")),
        );

        if !package.get_suggests().is_empty() {
            self.get_io().write("\n<info>suggests</info>");
            for (suggested, reason) in package.get_suggests().iter() {
                self.get_io()
                    .write(bconcat!(suggested, " <comment>", reason, "</comment>"));
            }
        }

        self.print_links(package.clone(), Link::TYPE_PROVIDE, None);
        self.print_links(package.clone(), Link::TYPE_CONFLICT, None);
        self.print_links(package, Link::TYPE_REPLACE, None);
        Ok(())
    }

    /// Prints package metadata.
    fn print_meta(
        &self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<BString, BString>,
        installed_repo: &mut dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let is_installed_package = !PlatformRepository::is_platform_package(package.get_name())
            && installed_repo.has_package(package.clone().into())?;

        self.get_io().write(bconcat!(
            "<info>name</info>     : ",
            package.get_pretty_name()
        ));
        self.get_io().write(bconcat!(
            "<info>descrip.</info> : ",
            package.get_description().unwrap_or_default()
        ));
        let keywords = package.get_keywords();
        self.get_io().write(bconcat!(
            "<info>keywords</info> : ",
            implode(", ", &keywords)
        ));
        self.print_versions(package.clone(), versions, installed_repo)?;
        if is_installed_package && let Some(rd) = package.get_release_date() {
            let rel = self.get_relative_time(&rd);
            self.get_io().write(bconcat!(
                "<info>released</info> : ",
                display_to_bytes(rd.format(date_format_to_strftime(b"Y-m-d"))),
                ", ",
                rel
            ));
        }
        let latest: PackageInterfaceHandle = if let Some(latest) = latest_package {
            let style = self.get_version_style(latest.clone(), package.clone().into())?;
            let released_time = match latest.get_release_date() {
                None => BString::default(),
                Some(rd) => {
                    let rel = self.get_relative_time(&rd);
                    bconcat!(
                        " released ",
                        display_to_bytes(rd.format(date_format_to_strftime(b"Y-m-d"))),
                        ", ",
                        rel
                    )
                }
            };
            self.get_io().write(bconcat!(
                "<info>latest</info>   : <",
                style,
                ">",
                latest.get_pretty_version(),
                "</",
                style,
                ">",
                released_time
            ));
            latest
        } else {
            package.clone().into()
        };
        self.get_io()
            .write(bconcat!("<info>type</info>     : ", package.get_type()));
        self.print_licenses(package.clone());
        self.get_io().write(bconcat!(
            "<info>homepage</info> : ",
            package.get_homepage().unwrap_or_default()
        ));
        self.get_io().write(bconcat!(
            "<info>source</info>   : [",
            package.get_source_type().unwrap_or_default(),
            "] <comment>",
            package.get_source_url().unwrap_or_default(),
            "</comment> ",
            package.get_source_reference().unwrap_or_default()
        ));
        self.get_io().write(bconcat!(
            "<info>dist</info>     : [",
            package.get_dist_type().unwrap_or_default(),
            "] <comment>",
            package.get_dist_url().unwrap_or_default(),
            "</comment> ",
            package.get_dist_reference().unwrap_or_default()
        ));
        if is_installed_package {
            let path = self.require_composer(None, None).ok().and_then(|c| {
                let installation_manager = c.borrow_partial().get_installation_manager();

                installation_manager
                    .borrow_mut()
                    .get_install_path(package.clone().into())
            });
            if let Some(p) = path {
                self.get_io().write(bconcat!(
                    "<info>path</info>     : ",
                    realpath(&p).unwrap_or_default()
                ));
            } else {
                self.get_io().write("<info>path</info>     : null");
            }
        }
        self.get_io().write(bconcat!(
            "<info>names</info>    : ",
            implode(", ", &package.get_names(true))
        ));

        if let Some(c) = latest.as_complete()
            && c.is_abandoned()
        {
            let replacement = match c.get_replacement_package() {
                Some(rp) => bconcat!(" The author suggests using the ", rp, " package instead."),
                None => BString::default(),
            };

            self.get_io().write_error(bconcat!(
                "<warning>Attention: This package is abandoned and no longer maintained.",
                replacement,
                "</warning>"
            ));
        }

        let support = package.get_support();
        if !support.is_empty() {
            self.get_io().write("\n<info>support</info>");
            for (r#type, value) in support.iter() {
                self.get_io()
                    .write(bconcat!("<comment>", r#type, "</comment> : ", value));
            }
        }

        let autoload_config = package.get_autoload();
        if !autoload_config.is_empty() {
            self.get_io().write("\n<info>autoload</info>");
            for (r#type, autoloads) in autoload_config.iter() {
                self.get_io()
                    .write(bconcat!("<comment>", r#type, "</comment>"));

                if r#type == "psr-0" || r#type == "psr-4" {
                    if let PhpMixed::Array(map) = autoloads {
                        for (name, path) in map.iter() {
                            let path_str = match path {
                                PhpMixed::List(l) => implode(
                                    ", ",
                                    &l.iter().filter_map(|p| p.as_string()).collect::<Vec<_>>(),
                                ),
                                PhpMixed::String(s) if !s.is_empty() => s.clone(),
                                _ => BString::from("."),
                            };
                            let name_disp = if name.is_empty() {
                                "*".into()
                            } else {
                                name.as_bstr()
                            };
                            self.get_io().write(bconcat!(name_disp, " => ", path_str));
                        }
                    }
                } else if r#type == "classmap"
                    && let PhpMixed::List(l) = autoloads
                {
                    let joined: Vec<&BStr> = l.iter().filter_map(|v| v.as_string()).collect();
                    self.get_io().write(implode(", ", &joined));
                }
            }
            let include_paths = package.get_include_paths();
            if !include_paths.is_empty() {
                self.get_io().write("<comment>include-path</comment>");
                self.get_io().write(implode(", ", &include_paths));
            }
        }

        Ok(())
    }

    /// Prints all available versions of this package and highlights the installed one if any.
    fn print_versions(
        &self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<BString, BString>,
        installed_repo: &mut dyn RepositoryInterface,
    ) -> anyhow::Result<()> {
        let mut versions_keys: Vec<BString> = versions.keys().cloned().collect();
        versions_keys = Semver::rsort(versions_keys)?;

        // highlight installed version
        let installed_packages =
            installed_repo.find_packages(bstr::BStr::new(&package.get_name()), None)?;
        if !installed_packages.is_empty() {
            for installed_package in installed_packages.iter() {
                let installed_version = installed_package.get_pretty_version();
                let key_map: IndexMap<BString, BString> = versions_keys
                    .iter()
                    .map(|v| (v.clone(), v.clone()))
                    .collect();
                if let Some(found) = array_search(&installed_version, &key_map)
                    && let Some(idx) = versions_keys.iter().position(|v| v == &found)
                {
                    versions_keys[idx] = bconcat!("<info>* ", installed_version, "</info>");
                }
            }
        }

        self.get_io().write(bconcat!(
            "<info>versions</info> : ",
            implode(", ", &versions_keys)
        ));

        Ok(())
    }

    /// print link objects
    fn print_links(
        &self,
        package: CompletePackageInterfaceHandle,
        link_type: &BStr,
        title: Option<&BStr>,
    ) {
        let title = title.unwrap_or(link_type);
        let io = self.get_io();
        let links = package.get_links_for_type(link_type);
        if !links.is_empty() {
            io.write(bconcat!("\n<info>", title, "</info>"));

            for link in links.iter() {
                io.write(bconcat!(
                    link.1.get_target(),
                    " <comment>",
                    link.1.get_pretty_constraint(),
                    "</comment>"
                ));
            }
        }
    }

    /// Prints the licenses of a package with metadata
    fn print_licenses(&self, package: CompletePackageInterfaceHandle) {
        let spdx_licenses = SpdxLicenses::new();

        let licenses = package.get_license();
        let io = self.get_io();

        for license_id in licenses.iter() {
            let license = spdx_licenses.get_license_by_identifier(license_id);

            let out = match license {
                None => license_id.clone(),
                Some(license) => {
                    if license.is_osi_approved {
                        bconcat!(
                            license.name,
                            " (",
                            license_id,
                            ") (OSI approved) ",
                            license.url
                        )
                    } else {
                        bconcat!(license.name, " (", license_id, ") ", license.url)
                    }
                }
            };

            io.write(bconcat!("<info>license</info>  : ", out));
        }
    }

    /// Prints package info in JSON format.
    fn print_package_info_as_json(
        &self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<BString, BString>,
        installed_repo: &mut dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let mut json = PackageInfoJson {
            name: package.get_pretty_name(),
            description: package.get_description(),
            keywords: package.get_keywords(),
            r#type: package.get_type(),
            homepage: package.get_homepage(),
            names: package.get_names(true),
            versions: Vec::new(),
            licenses: None,
            latest: None,
            source: None,
            dist: None,
            path: None,
            released: None,
            replacement: None,
            suggests: None,
            support: None,
            autoload: None,
            include_path: None,
            requires: None,
            dev_requires: None,
            provides: None,
            conflicts: None,
            replaces: None,
        };

        Self::append_versions(&mut json, versions);
        Self::append_licenses(&mut json, package.clone());

        let latest: PackageInterfaceHandle = if let Some(latest) = latest_package {
            json.latest = Some(latest.get_pretty_version());
            latest
        } else {
            package.clone().into()
        };

        if let Some(source_type) = package.get_source_type() {
            json.source = Some(PackageReferenceJson {
                r#type: source_type,
                url: package.get_source_url(),
                reference: package.get_source_reference(),
            });
        }

        if let Some(dist_type) = package.get_dist_type() {
            json.dist = Some(PackageReferenceJson {
                r#type: dist_type,
                url: package.get_dist_url(),
                reference: package.get_dist_reference(),
            });
        }

        if !PlatformRepository::is_platform_package(package.get_name())
            && installed_repo.has_package(package.clone().into())?
        {
            let composer = self.require_composer(None, None)?;
            let installation_manager = composer.borrow_partial().get_installation_manager();
            let path = installation_manager
                .borrow_mut()
                .get_install_path(package.clone().into());
            match path {
                Some(p) => {
                    if let Some(r) = realpath(&p) {
                        json.path = Some(Some(BString::from(path_to_bytes(&r))));
                    }
                }
                None => {
                    json.path = Some(None);
                }
            }

            if let Some(rd) = package.get_release_date() {
                json.released = Some(display_to_bytes(rd.format(DATE_ATOM)));
            }
        }

        if let Some(c) = latest.as_complete()
            && c.is_abandoned()
        {
            json.replacement = Some(c.get_replacement_package());
        }

        if !package.get_suggests().is_empty() {
            json.suggests = Some(package.get_suggests());
        }

        if !package.get_support().is_empty() {
            json.support = Some(package.get_support());
        }

        Self::append_autoload(&mut json, package.clone());

        if !package.get_include_paths().is_empty() {
            json.include_path = Some(package.get_include_paths());
        }

        Self::append_links(&mut json, package);

        self.get_io().write(&JsonFile::encode(&json)?);
        Ok(())
    }

    fn append_versions(json: &mut PackageInfoJson, versions: &IndexMap<BString, BString>) {
        let mut versions_pairs: Vec<(BString, BString)> = versions
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // uasort($versions, 'version_compare');
        versions_pairs.sort_by(|a, b| {
            if version_compare(&a.1, &b.1, CmpOp::Lt) {
                std::cmp::Ordering::Less
            } else if version_compare(&a.1, &b.1, CmpOp::Gt) {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        versions_pairs.reverse();
        json.versions = versions_pairs.into_iter().map(|(k, _)| k).collect();
    }

    fn append_licenses(json: &mut PackageInfoJson, package: CompletePackageInterfaceHandle) {
        let licenses = package.get_license();
        if !licenses.is_empty() {
            let spdx_licenses = SpdxLicenses::new();

            json.licenses = Some(
                licenses
                    .into_iter()
                    .map(|license_id| {
                        let license = spdx_licenses.get_license_by_identifier(&license_id);
                        match license {
                            None => LicenseJson::Id(license_id),
                            // The 'osi' key holds the license id string, not the OSI-approved flag.
                            Some(l) => LicenseJson::Detail {
                                name: l.name,
                                osi: license_id,
                                url: l.url,
                            },
                        }
                    })
                    .collect(),
            );
        }
    }

    fn append_autoload(json: &mut PackageInfoJson, package: CompletePackageInterfaceHandle) {
        let autoload_config = package.get_autoload();
        if !autoload_config.is_empty() {
            let mut autoload: IndexMap<BString, PhpMixed> = IndexMap::new();

            for (r#type, autoloads) in autoload_config.iter() {
                if r#type == "psr-0" || r#type == "psr-4" {
                    let mut psr: IndexMap<BString, PhpMixed> = IndexMap::new();

                    if let PhpMixed::Array(map) = autoloads {
                        for (name, path) in map.iter() {
                            let mut path_val = path.clone();
                            let is_empty_path = match &path_val {
                                PhpMixed::String(s) => s.is_empty(),
                                PhpMixed::Null => true,
                                _ => false,
                            };
                            if is_empty_path {
                                path_val = PhpMixed::string(BString::from("."));
                            }

                            let key = if name.is_empty() {
                                "*".into()
                            } else {
                                name.clone()
                            };
                            psr.insert(key, path_val);
                        }
                    }

                    autoload.insert(r#type.clone(), PhpMixed::Array(psr));
                } else if r#type == "classmap" {
                    autoload.insert("classmap".into(), autoloads.clone());
                }
            }

            json.autoload = Some(PhpMixed::Array(autoload));
        }
    }

    fn append_links(json: &mut PackageInfoJson, package: CompletePackageInterfaceHandle) {
        for link_type in Link::types() {
            Self::append_link(json, package.clone(), link_type);
        }
    }

    fn append_link(
        json: &mut PackageInfoJson,
        package: CompletePackageInterfaceHandle,
        link_type: &BStr,
    ) {
        let links = package.get_links_for_type(link_type);

        if !links.is_empty() {
            let mut m: IndexMap<BString, BString> = IndexMap::new();
            for link in links.iter() {
                m.insert(
                    BString::from(link.1.get_target()),
                    BString::from(link.1.get_pretty_constraint()),
                );
            }
            let slot = if link_type == Link::TYPE_REQUIRE {
                &mut json.requires
            } else if link_type == Link::TYPE_DEV_REQUIRE {
                &mut json.dev_requires
            } else if link_type == Link::TYPE_PROVIDE {
                &mut json.provides
            } else if link_type == Link::TYPE_CONFLICT {
                &mut json.conflicts
            } else if link_type == Link::TYPE_REPLACE {
                &mut json.replaces
            } else {
                unreachable!("Link::types() yields only the five link types")
            };
            *slot = Some(m);
        }
    }

    /// Init styles for tree
    fn init_styles(&self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        *self.colors.borrow_mut() = vec![
            BString::from("green"),
            BString::from("yellow"),
            BString::from("cyan"),
            BString::from("magenta"),
            BString::from("blue"),
        ];

        for color in self.colors.borrow().iter() {
            let style =
                OutputFormatterStyle::new(Some(bstr::BStr::new(color.as_bstr())), None, vec![]);
            output
                .borrow()
                .get_formatter()
                .borrow_mut()
                .set_style(bstr::BStr::new(color), Box::new(style));
        }
    }

    /// Display the tree
    fn display_package_tree(&self, array_tree: Vec<PackageTree>) {
        for package in array_tree.iter() {
            self.get_io()
                .write_no_newline(bconcat!("<info>", package.name, "</info>"));
            self.get_io()
                .write_no_newline(bconcat!(" ", package.version));
            if let Some(description) = &package.description {
                self.get_io().write(bconcat!(" ", first_token(description)));
            } else {
                // output newline
                self.get_io().write("");
            }

            if let Some(requires) = &package.requires {
                let mut tree_bar = BString::from("├");
                let mut j = 0_usize;
                let total = requires.len();
                for require in requires.iter() {
                    j += 1;
                    if j == total {
                        tree_bar = BString::from("└");
                    }
                    let level: usize = 1;
                    let color = self.colors.borrow().get(level).cloned().unwrap_or_default();
                    let info = bconcat!(
                        tree_bar,
                        "──<",
                        color,
                        ">",
                        require.name,
                        "</",
                        color,
                        "> ",
                        require.version
                    );
                    self.write_tree_line(BStr::new(&info));

                    tree_bar = str_replace("└", " ", &tree_bar);
                    let packages_in_tree: Vec<BString> =
                        vec![package.name.clone(), require.name.clone()];

                    self.display_tree(require, &packages_in_tree, tree_bar.as_bstr(), level + 1);
                }
            }
        }
    }

    /// Generate the package tree
    fn generate_package_tree(
        &self,
        package: PackageInterfaceHandle,
        installed_repo: &RepositoryInterfaceHandle,
        remote_repos: &RepositoryInterfaceHandle,
    ) -> PackageTree {
        let requires = {
            let mut r = (*package.get_requires()).clone();
            r.sort_keys();
            r
        };
        let mut children: Vec<PackageTreeChild> = Vec::new();
        for (require_name, require) in requires.iter() {
            let packages_in_tree: Vec<BString> = vec![package.get_name(), require_name.clone()];

            let mut tree_child_desc = PackageTreeChild {
                name: require_name.clone(),
                version: BString::from(require.get_pretty_constraint()),
                requires: None,
            };

            let deep_children = self
                .add_tree(
                    BStr::new(require_name),
                    require,
                    installed_repo,
                    remote_repos,
                    &packages_in_tree,
                )
                .unwrap_or_default();

            if !deep_children.is_empty() {
                tree_child_desc.requires = Some(deep_children);
            }

            children.push(tree_child_desc);
        }
        let mut tree = PackageTree {
            name: package.get_pretty_name(),
            version: package.get_pretty_version(),
            description: match package.as_complete() {
                Some(c) => c.get_description(),
                None => Some(BString::default()),
            },
            requires: None,
        };

        if !children.is_empty() {
            tree.requires = Some(children);
        }

        tree
    }

    /// Display a package tree
    fn display_tree(
        &self,
        package: &PackageTreeChild,
        packages_in_tree: &[BString],
        previous_tree_bar: &BStr,
        level: usize,
    ) {
        let previous_tree_bar = str_replace("├", "│", previous_tree_bar);
        let requires = match &package.requires {
            Some(r) => r,
            None => return,
        };
        let mut tree_bar = bconcat!(previous_tree_bar, "  ├");
        let mut i = 0_usize;
        let total = requires.len();
        for require in requires.iter() {
            let mut current_tree = packages_in_tree.to_vec();
            i += 1;
            if i == total {
                tree_bar = bconcat!(previous_tree_bar, "  └");
            }
            let color_ident = level % self.colors.borrow().len();
            let color = self
                .colors
                .borrow()
                .get(color_ident)
                .cloned()
                .unwrap_or_default();

            let circular_warn = if current_tree.contains(&require.name) {
                "(circular dependency aborted here)"
            } else {
                ""
            };
            let info = rtrim(bconcat!(
                tree_bar,
                "──<",
                color,
                ">",
                require.name,
                "</",
                color,
                "> ",
                require.version,
                " ",
                circular_warn
            ));
            self.write_tree_line(BStr::new(&info));

            tree_bar = str_replace("└", " ", &tree_bar);

            current_tree.push(require.name.clone());
            self.display_tree(require, &current_tree, tree_bar.as_bstr(), level + 1);
        }
    }

    /// Display a package tree
    fn add_tree(
        &self,
        name: &BStr,
        link: &Link,
        installed_repo: &RepositoryInterfaceHandle,
        remote_repos: &RepositoryInterfaceHandle,
        packages_in_tree: &[BString],
    ) -> anyhow::Result<Vec<PackageTreeChild>> {
        let mut children: Vec<PackageTreeChild> = Vec::new();
        let version_arg: PhpMixed = if link.get_pretty_constraint() == "self.version" {
            // pass the ConstraintInterface object — signal via Null in this scalar shape
            PhpMixed::Null
        } else {
            PhpMixed::string(link.get_pretty_constraint())
        };
        let (package, _) = self.get_package(installed_repo, remote_repos, name, version_arg)?;
        if let Some(package) = package {
            let mut requires = (*package.get_requires()).clone();
            requires.sort_keys();
            for (require_name, require) in requires.iter() {
                let mut current_tree = packages_in_tree.to_vec();

                let mut tree_child_desc = PackageTreeChild {
                    name: require_name.clone(),
                    version: BString::from(require.get_pretty_constraint()),
                    requires: None,
                };

                if !current_tree.contains(require_name) {
                    current_tree.push(require_name.clone());
                    let deep_children = self.add_tree(
                        BStr::new(require_name),
                        require,
                        installed_repo,
                        remote_repos,
                        &current_tree,
                    )?;
                    if !deep_children.is_empty() {
                        tree_child_desc.requires = Some(deep_children);
                    }
                }

                children.push(tree_child_desc);
            }
        }

        Ok(children)
    }

    fn update_status_to_version_style(update_status: &BStr) -> &'static BStr {
        // 'up-to-date' is printed green
        // 'semver-safe-update' is printed red
        // 'update-possible' is printed yellow
        BStr::new(match update_status.as_bytes() {
            b"up-to-date" => "info",
            b"semver-safe-update" => "highlight",
            b"update-possible" => "comment",
            _ => "comment",
        })
    }

    fn get_update_status(
        latest_package: PackageInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<BString> {
        if latest_package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
            == package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
        {
            return Ok(BString::from("up-to-date"));
        }

        let mut constraint = package.get_version();
        if !constraint.starts_with(b"dev-") {
            constraint = bconcat!("^", constraint);
        }
        if !latest_package.get_version().is_empty()
            && Semver::satisfies(latest_package.get_version(), constraint)?
        {
            // it needs an immediate semver-compliant upgrade
            return Ok(BString::from("semver-safe-update"));
        }

        // it needs an upgrade but has potential BC breaks so is not urgent
        Ok(BString::from("update-possible"))
    }

    fn write_tree_line(&self, line: &BStr) {
        let io = self.get_io();
        if !io.is_decorated() {
            io.write(
                line.replace("└", "`-")
                    .replace("├", "|-")
                    .replace("──", "-")
                    .replace("│", "|"),
            );
        } else {
            io.write(line);
        }
    }

    /// Given a package, this finds the latest package matching it
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    fn find_latest_package(
        &self,
        package: PackageInterfaceHandle,
        composer: &PartialComposerHandle,
        platform_repo: &PlatformRepositoryHandle,
        major_only: bool,
        minor_only: bool,
        patch_only: bool,
        platform_req_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface>,
    ) -> anyhow::Result<Option<crate::package::PackageInterfaceHandle>> {
        // find the latest version allowed in this repo set
        let name = package.get_name();
        let repo_set = self.get_repository_set(composer)?;
        let composer_ref = crate::composer::composer_full(composer);
        let mut version_selector =
            VersionSelector::new(repo_set, Some(&mut *platform_repo.borrow_mut()))?;
        let mut stability = composer_ref.get_package().get_minimum_stability();
        let flags = composer_ref.get_package().get_stability_flags();
        if let Some(flag_value) = flags.get(bstr::B(&name)) {
            let key_map: IndexMap<BString, BString> = base_package::STABILITIES
                .iter()
                .map(|(k, v)| (BString::from(*k), bconcat!(v)))
                .collect();
            let needle = bconcat!(flag_value);
            if let Some(found_key) = array_search(&needle, &key_map) {
                stability = found_key;
            }
        }

        let mut best_stability = stability;
        if composer_ref.get_package().get_prefer_stable() {
            best_stability = package.get_stability();
        }

        let mut target_version: Option<BString> = None;
        if package.get_version().starts_with(b"dev-") {
            target_version = Some(package.get_version());

            // dev-x branches are considered to be on the latest major version always, do not look up for a new commit as that is deemed a minor upgrade (albeit risky)
            if major_only {
                return Ok(None);
            }
        }

        if target_version.is_none() {
            if major_only
                && let Some(groups) = preg_match(
                    php_regex!(r"{^(?P<zero_major>(?:0\.)+)?(?P<first_meaningful>\d+)\.}"),
                    &package.get_version(),
                )
            {
                let zero_major = groups.name("zero_major").unwrap_or_default();
                let first_meaningful = intval(&PhpMixed::string(
                    groups.name("first_meaningful").unwrap_or_default(),
                ));
                target_version = Some(bconcat!(
                    ">=",
                    zero_major,
                    first_meaningful + 1,
                    ",<9999999-dev"
                ));
            }

            if minor_only {
                target_version = Some(bconcat!("^", package.get_version()));
            }

            if patch_only {
                let trimmed_version =
                    preg_replace(php_regex!(r"{(\.0)+$}D"), "", package.get_version());
                let parts_needed = if trimmed_version.starts_with(b"0") {
                    4
                } else {
                    3
                };
                let mut trimmed_version = trimmed_version;
                while trimmed_version.iter().filter(|&&c| c == b'.').count() + 1 < parts_needed {
                    trimmed_version.push_str(".0");
                }
                target_version = Some(bconcat!("~", trimmed_version));
            }
        }

        let show_warnings = if self.get_io().is_verbose() {
            ShowWarnings::Always
        } else {
            let package_version = package.get_version();
            ShowWarnings::Predicate(Box::new(
                move |candidate: &PackageInterfaceHandle| -> bool {
                    if candidate.get_version().starts_with(b"dev-")
                        || package_version.starts_with(b"dev-")
                    {
                        return false;
                    }

                    version_compare(candidate.get_version(), &package_version, CmpOp::Le)
                },
            ))
        };
        let mut candidate = version_selector.find_best_candidate(
            bstr::BStr::new(&name),
            target_version.as_ref().map(|version| version.as_ref()),
            bstr::BStr::new(&best_stability),
            Some(platform_req_filter),
            0,
            Some(self.get_io().clone()),
            show_warnings,
        )?;
        while let Some(ref c) = candidate {
            if let Some(alias) = c.as_alias() {
                candidate = Some(alias.get_alias_of().into());
            } else {
                break;
            }
        }

        Ok(candidate)
    }

    fn get_repository_set(
        &self,
        composer: &PartialComposerHandle,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<RepositorySet>>> {
        let composer = crate::composer::composer_full(composer);
        if self.repository_set.borrow().is_none() {
            let mut rs = RepositorySet::new(
                composer.get_package().get_minimum_stability(),
                composer.get_package().get_stability_flags(),
                Vec::new(),
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            );
            rs.add_repository(RepositoryInterfaceHandle::new(CompositeRepository::new(
                composer
                    .get_repository_manager()
                    .borrow()
                    .get_repositories()
                    .to_vec(),
            )))?;
            *self.repository_set.borrow_mut() = Some(std::rc::Rc::new(std::cell::RefCell::new(rs)));
        }

        Ok(self.repository_set.borrow().as_ref().unwrap().clone())
    }

    fn get_relative_time(&self, release_date: &chrono::DateTime<chrono::Utc>) -> BString {
        if display_to_bytes(release_date.format(date_format_to_strftime(b"Ymd")))
            == date_local("Ymd", None)
        {
            return BString::from("today");
        }

        let diff = chrono::Utc::now().signed_duration_since(*release_date);
        let days = diff.num_days();
        if days < 7 {
            return BString::from("this week");
        }

        if days < 14 {
            return BString::from("last week");
        }

        let months = days / 30;
        if months < 1 && days < 31 {
            return bconcat!(days / 7, " weeks ago");
        }

        let years = days / 365;
        if years < 1 {
            return bconcat!(months, " month", if months > 1 { "s" } else { "" }, " ago");
        }

        bconcat!(years, " year", if years > 1 { "s" } else { "" }, " ago")
    }

    fn same_repository<T, U>(a: &T, b: &U) -> bool
    where
        T: Into<RepositoryInterfaceHandle> + Clone,
        U: Into<RepositoryInterfaceHandle> + Clone,
    {
        let a = a.clone().into();
        let b = b.clone().into();
        Self::same_repository_handle(&a, &b)
    }

    fn same_repository_handle(
        a: &RepositoryInterfaceHandle,
        b: &RepositoryInterfaceHandle,
    ) -> bool {
        a.ptr_eq(b)
    }
}

#[async_trait::async_trait(?Send)]
impl Command for ShowCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("show".into())?;
        self.set_aliases(vec!["info".into()])?;
        self.set_description("Shows information about packages".into());
        let opt_none = |name: &BStr, shortcut: Option<&BStr>, description: &BStr| {
            InputOption::new(
                name,
                shortcut,
                Some(InputOption::VALUE_NONE),
                description,
                None,
            )
            .unwrap()
            .into()
        };
        self.set_definition(&[
            InputArgument::new5(
                "package",
                Some(InputArgument::OPTIONAL),
                "Package to inspect. Or a name including a wildcard (*) to filter lists of packages instead.",
                None,
                self.suggest_package_based_on_mode(),
            )
            .unwrap()
            .into(),
            InputArgument::new(
                "version",
                Some(InputArgument::OPTIONAL),
                "Version or version constraint to inspect",
                None,
            )
            .unwrap()
            .into(),
            opt_none(BStr::new("all"), None, BStr::new("List all packages")),
            opt_none(BStr::new("locked"), None, BStr::new("List all locked packages")),
            opt_none(
                BStr::new("installed"),
                Some(BStr::new("i")),
                BStr::new("List installed packages only (enabled by default, only present for BC)."),
            ),
            opt_none(BStr::new("platform"), Some(BStr::new("p")), BStr::new("List platform packages only")),
            opt_none(BStr::new("available"), Some(BStr::new("a")), BStr::new("List available packages only")),
            opt_none(BStr::new("self"), Some(BStr::new("s")), BStr::new("Show the root package information")),
            opt_none(BStr::new("name-only"), Some(BStr::new("N")), BStr::new("List package names only")),
            opt_none(BStr::new("path"), Some(BStr::new("P")), BStr::new("Show package paths")),
            opt_none(BStr::new("tree"), Some(BStr::new("t")), BStr::new("List the dependencies as a tree")),
            opt_none(BStr::new("latest"), Some(BStr::new("l")), BStr::new("Show the latest version")),
            opt_none(
                BStr::new("outdated"),
                Some(BStr::new("o")),
                BStr::new("Show the latest version but only for packages that are outdated"),
            ),
            InputOption::new6(
                "ignore",
                None,
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "Ignore specified package(s). Can contain wildcards (*). Use it with the --outdated option if you don't want to be informed about new versions of some packages.",
                None,
                self.suggest_installed_package(false, false),
            )
            .unwrap()
            .into(),
            opt_none(
                BStr::new("major-only"),
                Some(BStr::new("M")),
                BStr::new("Show only packages that have major SemVer-compatible updates. Use with the --latest or --outdated option."),
            ),
            opt_none(
                BStr::new("minor-only"),
                Some(BStr::new("m")),
                BStr::new("Show only packages that have minor SemVer-compatible updates. Use with the --latest or --outdated option."),
            ),
            opt_none(
                BStr::new("patch-only"),
                None,
                BStr::new("Show only packages that have patch SemVer-compatible updates. Use with the --latest or --outdated option."),
            ),
            opt_none(
                BStr::new("sort-by-age"),
                Some(BStr::new("A")),
                BStr::new("Displays the installed version's age, and sorts packages oldest first. Use with the --latest or --outdated option."),
            ),
            opt_none(
                BStr::new("direct"),
                Some(BStr::new("D")),
                BStr::new("Shows only packages that are directly required by the root package"),
            ),
            opt_none(
                BStr::new("strict"),
                None,
                BStr::new("Return a non-zero exit code when there are outdated packages"),
            ),
            InputOption::new6(
                "format",
                Some("f".into()),
                Some(InputOption::VALUE_REQUIRED),
                "Format of the output: text or json",
                Some(InputValue::from("text")),
                SuggestedValues::List(vec![BString::from("json"), BString::from("text")]),
            )
            .unwrap()
            .into(),
            opt_none(BStr::new("no-dev"), None, BStr::new("Disables search in require-dev packages.")),
            InputOption::new(
                "ignore-platform-req",
                None,
                Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                "Ignore a specific platform requirement (php & ext- packages). Use with the --outdated option",
                None,
            )
            .unwrap()
            .into(),
            opt_none(
                BStr::new("ignore-platform-reqs"),
                None,
                BStr::new("Ignore all platform requirements (php & ext- packages). Use with the --outdated option"),
            ),
        ]);
        self.set_help(
            "The show command displays detailed information about a package, or\n\
             lists all packages available.\n\n\
             Read more at https://getcomposer.org/doc/03-cli.md#show-info"
                .into(),
        );
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        *self.version_parser.borrow_mut() = VersionParser::new();
        if input.borrow().get_option("tree")?.as_bool() == Some(true) {
            self.init_styles(output.clone());
        }

        let composer = self.try_composer(None, None);

        if input.borrow().get_option("installed")?.as_bool() == Some(true)
            && input.borrow().get_option("self")?.as_bool() != Some(true)
        {
            self.get_io().write_error("<warning>You are using the deprecated option \"installed\". Only installed packages are shown by default now. The --all option can be used to show all packages.</warning>");
        }

        if input.borrow().get_option("outdated")?.as_bool() == Some(true) {
            input
                .borrow_mut()
                .set_option("latest", InputValue::Bool(true));
        } else if input
            .borrow()
            .get_option("ignore")?
            .as_array()
            .map_or(0, <[bstr::BString]>::len)
            > 0
        {
            self.get_io().write_error("<warning>You are using the option \"ignore\" for action other than \"outdated\", it will be ignored.</warning>");
        }

        if input.borrow().get_option("direct")?.as_bool() == Some(true)
            && (input.borrow().get_option("all")?.as_bool() == Some(true)
                || input.borrow().get_option("available")?.as_bool() == Some(true)
                || input.borrow().get_option("platform")?.as_bool() == Some(true))
        {
            self.get_io().write_error("The --direct (-D) option is not usable in combination with --all, --platform (-p) or --available (-a)");

            return Ok(1);
        }

        if input.borrow().get_option("tree")?.as_bool() == Some(true)
            && (input.borrow().get_option("all")?.as_bool() == Some(true)
                || input.borrow().get_option("available")?.as_bool() == Some(true))
        {
            self.get_io().write_error("The --tree (-t) option is not usable in combination with --all or --available (-a)");

            return Ok(1);
        }

        let only_count: usize = [
            input.borrow().get_option("patch-only")?.as_bool() == Some(true),
            input.borrow().get_option("minor-only")?.as_bool() == Some(true),
            input.borrow().get_option("major-only")?.as_bool() == Some(true),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if only_count > 1 {
            self.get_io().write_error(
                "Only one of --major-only, --minor-only or --patch-only can be used at once",
            );

            return Ok(1);
        }

        if input.borrow().get_option("tree")?.as_bool() == Some(true)
            && input.borrow().get_option("latest")?.as_bool() == Some(true)
        {
            self.get_io().write_error(
                "The --tree (-t) option is not usable in combination with --latest (-l)",
            );

            return Ok(1);
        }

        if input.borrow().get_option("tree")?.as_bool() == Some(true)
            && input.borrow().get_option("path")?.as_bool() == Some(true)
        {
            self.get_io().write_error(
                "The --tree (-t) option is not usable in combination with --path (-P)",
            );

            return Ok(1);
        }

        let format = BString::from(
            input
                .borrow()
                .get_option("format")?
                .as_string()
                .unwrap_or(BStr::new("text")),
        );
        if !in_array_loose(
            format.clone(),
            &[
                PhpMixed::string(BString::from("text")),
                PhpMixed::string(BString::from("json")),
            ],
        ) {
            self.get_io().write_error(bconcat!(
                "Unsupported format \"",
                format,
                "\". See help for supported formats."
            ));

            return Ok(1);
        }

        let platform_req_filter = self.get_platform_requirement_filter(input.clone())?;

        // init repos
        let mut platform_overrides: IndexMap<BString, PhpMixed> = IndexMap::new();
        if let Some(ref composer) = composer {
            let composer = crate::composer::composer_full(composer);
            if let Some(p) = composer
                .get_config()
                .borrow()
                .get("platform")
                .as_array()
                .cloned()
            {
                platform_overrides = p.into_iter().collect();
            }
        }
        let platform_repo = PlatformRepositoryHandle::new(PlatformRepository::new(
            vec![],
            platform_overrides,
            None,
            None,
        )?);
        let mut locked_repo: Option<RepositoryInterfaceHandle> = None;

        // The single-package $package binding from PHP gets surfaced here.
        let mut single_package: Option<crate::package::CompletePackageInterfaceHandle> = None;
        let mut versions_map: IndexMap<BString, BString> = IndexMap::new();
        let installed_repo: RepositoryInterfaceHandle;
        let repos: RepositoryInterfaceHandle;

        if input.borrow().get_option("self")?.as_bool() == Some(true)
            && input.borrow().get_option("installed")?.as_bool() != Some(true)
            && input.borrow().get_option("locked")?.as_bool() != Some(true)
        {
            let composer = self.require_composer(None, None)?;
            let package = crate::package::RootPackageInterfaceHandle::dup(
                composer.borrow_partial().get_package(),
            );
            if input.borrow().get_option("name-only")?.as_bool() == Some(true) {
                self.get_io().write(package.get_name());

                return Ok(0);
            }
            if input
                .borrow()
                .get_argument("package")?
                .as_string()
                .is_some()
            {
                return Err(InvalidArgumentException::new(BString::from(
                    "You cannot use --self together with a package name",
                ))
                .into());
            }
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                RepositoryInterfaceHandle::new(RootPackageRepository::new(package.clone())),
            ]));
            repos = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                RepositoryInterfaceHandle::new(RootPackageRepository::new(package.clone())),
            ]));
            single_package = Some(package.into());
        } else if input.borrow().get_option("platform")?.as_bool() == Some(true) {
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
            repos = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
        } else if input.borrow().get_option("available")?.as_bool() == Some(true) {
            let mut ir = InstalledRepository::new(vec![platform_repo.clone().into()]);
            if let Some(ref composer) = composer {
                let composer = crate::composer::composer_full(composer);
                repos = RepositoryInterfaceHandle::new(CompositeRepository::new(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_repositories()
                        .to_vec(),
                ));
                ir.add_repository(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_local_repository(),
                );
                installed_repo = RepositoryInterfaceHandle::new(ir);
            } else {
                let default_repos =
                    RepositoryFactory::default_repos_with_default_manager(self.get_io())?;
                let names: Vec<BString> = default_repos.keys().cloned().collect();
                repos = RepositoryInterfaceHandle::new(CompositeRepository::new(
                    default_repos.into_values().collect(),
                ));
                self.get_io().write_error(bconcat!("No composer.json found in the current directory, showing available packages from ", implode(", ", &names)));
                installed_repo = RepositoryInterfaceHandle::new(ir);
            }
        } else if input.borrow().get_option("all")?.as_bool() == Some(true) && composer.is_some() {
            let composer_ref = crate::composer::composer_full(composer.as_ref().unwrap());
            let local_repo = composer_ref
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            let locker_rc = composer_ref.get_locker().clone();
            let mut locker = locker_rc.borrow_mut();
            if locker.is_locked() {
                let lr_handle: RepositoryInterfaceHandle =
                    locker.get_locked_repository(true)?.into();
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    lr_handle.clone(),
                    local_repo,
                    platform_repo.clone().into(),
                ]));
                locked_repo = Some(lr_handle);
            } else {
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    local_repo,
                    platform_repo.clone().into(),
                ]));
            }
            let mut composite_input: Vec<RepositoryInterfaceHandle> =
                vec![RepositoryInterfaceHandle::new(FilterRepository::new(
                    installed_repo.clone(),
                    {
                        let mut m = IndexMap::new();
                        m.insert("canonical".into(), PhpMixed::Bool(false));
                        m
                    },
                )?)];
            for r in composer_ref
                .get_repository_manager()
                .borrow()
                .get_repositories()
            {
                composite_input.push(r.clone());
            }
            repos = RepositoryInterfaceHandle::new(CompositeRepository::new(composite_input));
        } else if input.borrow().get_option("all")?.as_bool() == Some(true) {
            let default_repos =
                RepositoryFactory::default_repos_with_default_manager(self.get_io())?;
            let names: Vec<BString> = default_repos.keys().cloned().collect();
            self.get_io().write_error(bconcat!(
                "No composer.json found in the current directory, showing available packages from ",
                implode(", ", &names)
            ));
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
            let mut composite_input: Vec<RepositoryInterfaceHandle> = vec![installed_repo.clone()];
            for (_k, v) in default_repos.into_iter() {
                composite_input.push(v);
            }
            repos = RepositoryInterfaceHandle::new(CompositeRepository::new(composite_input));
        } else if input.borrow().get_option("locked")?.as_bool() == Some(true) {
            if composer.is_none()
                || !crate::composer::composer_full(composer.as_ref().unwrap())
                    .get_locker()
                    .borrow_mut()
                    .is_locked()
            {
                return Err(UnexpectedValueException::new(BString::from("A valid composer.json and composer.lock files is required to run this command with --locked"))
                .into());
            }
            let composer_ref = crate::composer::composer_full(composer.as_ref().unwrap());
            let locker_rc = composer_ref.get_locker().clone();
            let mut locker = locker_rc.borrow_mut();
            let lr = locker.get_locked_repository(
                input.borrow().get_option("no-dev")?.as_bool() != Some(true),
            )?;
            if input.borrow().get_option("self")?.as_bool() == Some(true) {
                lr.add_package(
                    crate::package::RootPackageInterfaceHandle::dup(composer_ref.get_package())
                        .into(),
                )?;
            }
            let lr_handle: RepositoryInterfaceHandle = lr.into();
            locked_repo = Some(lr_handle.clone());
            let new_repo =
                RepositoryInterfaceHandle::new(InstalledRepository::new(vec![lr_handle]));
            installed_repo = new_repo.clone();
            repos = new_repo;
        } else {
            // --installed / default case
            let composer_local_owned;
            let _guard_from_existing;
            let composer_local = match composer.as_ref() {
                Some(c) => {
                    _guard_from_existing = crate::composer::composer_full(c);
                    &*_guard_from_existing
                }
                None => {
                    composer_local_owned = self.require_composer(None, None)?;
                    _guard_from_existing = crate::composer::composer_full(&composer_local_owned);
                    &*_guard_from_existing
                }
            };
            let root_pkg = composer_local.get_package();

            let root_repo: RepositoryInterfaceHandle =
                if input.borrow().get_option("self")?.as_bool() == Some(true) {
                    RepositoryInterfaceHandle::new(RootPackageRepository::new(
                        crate::package::RootPackageInterfaceHandle::dup(
                            composer_local.get_package(),
                        ),
                    ))
                } else {
                    RepositoryInterfaceHandle::new(InstalledArrayRepository::new()?)
                };
            if input.borrow().get_option("no-dev")?.as_bool() == Some(true) {
                let local_packages = composer_local
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository()
                    .get_packages()?;
                let packages = RepositoryUtils::filter_required_packages(
                    &local_packages,
                    root_pkg.clone().into(),
                    false,
                    Vec::new(),
                );
                let cloned: Vec<crate::package::PackageInterfaceHandle> = packages
                    .iter()
                    .map(crate::package::PackageInterfaceHandle::dup)
                    .collect();
                let new_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    root_repo,
                    RepositoryInterfaceHandle::new(InstalledArrayRepository::new_with_packages(
                        cloned,
                    )?),
                ]));
                installed_repo = new_repo.clone();
                repos = new_repo;
            } else {
                let repository_manager = composer_local.get_repository_manager().clone();
                let repository_manager = repository_manager.borrow();
                let lr = repository_manager.get_local_repository();
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    root_repo.clone(),
                    lr.clone(),
                ]));
                repos =
                    RepositoryInterfaceHandle::new(InstalledRepository::new(vec![root_repo, lr]));
            }

            if installed_repo.get_packages()?.is_empty() {
                let has_non_platform_reqs = |reqs: &IndexMap<BString, Link>| -> bool {
                    reqs.keys()
                        .any(|name| !PlatformRepository::is_platform_package(name))
                };

                if has_non_platform_reqs(&root_pkg.get_requires())
                    || has_non_platform_reqs(&root_pkg.get_dev_requires())
                {
                    let _ = root_pkg;
                    self.get_io().write_error("<warning>No dependencies installed. Try running shirabe install or update.</warning>");
                }
            }
        }

        if let Some(ref composer) = composer {
            let composer = crate::composer::composer_full(composer);
            let mut command_event = CommandEvent::new6(
                PluginEvents::COMMAND,
                "show",
                input.clone(),
                output,
                vec![],
                IndexMap::new(),
            );
            let command_event_name = command_event.get_name().to_owned();
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(command_event_name.as_ref()), Some(&mut command_event))?;
        }

        if input.borrow().get_option("latest")?.as_bool() == Some(true) && composer.is_none() {
            self.get_io().write_error(
                "No composer.json found in the current directory, disabling \"latest\" option",
            );
            input
                .borrow_mut()
                .set_option("latest", InputValue::Bool(false));
        }

        let package_filter: Option<BString> = input
            .borrow()
            .get_argument("package")?
            .as_string()
            .map(BString::from);

        // show single package or single version
        if let Some(ref pkg) = single_package {
            versions_map.insert(pkg.get_pretty_version(), pkg.get_version());
        } else if let Some(ref pf) = package_filter
            && !pf.contains(&b'*')
        {
            let (matched_package, vers) = self.get_package(
                &installed_repo,
                &repos,
                BStr::new(pf),
                input.borrow().get_argument("version")?.to_php_mixed(),
            )?;

            if let Some(ref pkg) = matched_package
                && input.borrow().get_option("direct")?.as_bool() == Some(true)
                && !in_array_strict(
                    pkg.get_name(),
                    &self
                        .get_root_requires()
                        .into_iter()
                        .map(PhpMixed::string)
                        .collect::<Vec<_>>(),
                )
            {
                return Err(InvalidArgumentException::new(bconcat!(
                    "Package \"",
                    pkg.get_name(),
                    "\" is installed but not a direct dependent of the root package."
                ))
                .into());
            }

            if matched_package.is_none() {
                let options = input.borrow().get_options();
                let mut hint = BString::default();
                if input.borrow().get_option("locked")?.as_bool() == Some(true) {
                    hint.push_str(" in lock file");
                }
                if let Some(working_dir) =
                    options.get(bstr::B("working-dir")).filter(|v| !v.is_null())
                {
                    hint.push_str(&bconcat!(
                        " in ",
                        working_dir.as_string().unwrap_or_default(),
                        "/composer.json"
                    ));
                }
                if PlatformRepository::is_platform_package(pf)
                    && input.borrow().get_option("platform")?.as_bool() != Some(true)
                {
                    hint.push_str(", try using --platform (-p) to show platform packages");
                }
                if input.borrow().get_option("all")?.as_bool() != Some(true)
                    && input.borrow().get_option("available")?.as_bool() != Some(true)
                {
                    hint.push_str(", try using --available (-a) to show all available packages");
                }

                return Err(InvalidArgumentException::new(bconcat!(
                    "Package \"",
                    pf,
                    "\" not found",
                    hint,
                    "."
                ))
                .into());
            }
            single_package = matched_package;
            versions_map = vers;
        }

        if let Some(ref package) = single_package {
            // assert(isset($versions));

            let mut exit_code: i64 = 0;
            if input.borrow().get_option("tree")?.as_bool() == Some(true) {
                let array_tree =
                    self.generate_package_tree(package.clone().into(), &installed_repo, &repos);

                if format == "json" {
                    self.get_io().write(&JsonFile::encode(&PackageTreeJson {
                        installed: vec![array_tree],
                    })?);
                } else {
                    self.display_package_tree(vec![array_tree]);
                }

                return Ok(exit_code);
            }

            let mut latest_package: Option<crate::package::PackageInterfaceHandle> = None;
            if input.borrow().get_option("latest")?.as_bool() == Some(true) {
                latest_package = self.find_latest_package(
                    package.clone().into(),
                    composer.as_ref().unwrap(),
                    &platform_repo,
                    input
                        .borrow()
                        .get_option("major-only")?
                        .as_bool()
                        .unwrap_or(false),
                    input
                        .borrow()
                        .get_option("minor-only")?
                        .as_bool()
                        .unwrap_or(false),
                    input
                        .borrow()
                        .get_option("patch-only")?
                        .as_bool()
                        .unwrap_or(false),
                    platform_req_filter.clone(),
                )?;
            }
            if input.borrow().get_option("outdated")?.as_bool() == Some(true)
                && input.borrow().get_option("strict")?.as_bool() == Some(true)
                && latest_package.is_some()
                && latest_package
                    .as_ref()
                    .unwrap()
                    .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                    != package
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                && (latest_package
                    .as_ref()
                    .unwrap()
                    .as_complete()
                    .is_none_or(|c| !c.is_abandoned()))
            {
                exit_code = 1;
            }
            if input.borrow().get_option("path")?.as_bool() == Some(true) {
                self.get_io().write_no_newline(package.get_name());
                let path = {
                    let composer_ref = composer.as_ref().unwrap();
                    composer_ref
                        .borrow_partial()
                        .get_installation_manager()
                        .borrow_mut()
                        .get_install_path(package.clone().into())
                };
                if let Some(path) = path {
                    let real = realpath(&path).unwrap_or_default();
                    self.get_io()
                        .write(bconcat!(" ", first_token(path_to_bytes(&real))));
                } else {
                    self.get_io().write(" null");
                }

                return Ok(exit_code);
            }

            if format == "json" {
                self.print_package_info_as_json(
                    package.clone(),
                    &versions_map,
                    &mut *installed_repo.borrow_mut(),
                    latest_package,
                )?;
            } else {
                self.print_package_info(
                    package.clone(),
                    &versions_map,
                    &mut *installed_repo.borrow_mut(),
                    latest_package,
                )?;
            }

            return Ok(exit_code);
        }

        // show tree view if requested
        if input.borrow().get_option("tree")?.as_bool() == Some(true) {
            let root_requires = self.get_root_requires();
            let mut packages = installed_repo.get_packages()?;
            packages.sort_by(|a, b| {
                let sa = a.to_php_string();
                let sb = b.to_php_string();
                sa.cmp(&sb)
            });
            let mut array_tree: Vec<PackageTree> = Vec::new();
            for package in packages.iter() {
                if in_array_strict(
                    package.get_name(),
                    &root_requires
                        .iter()
                        .map(|s| PhpMixed::string(s.clone()))
                        .collect::<Vec<_>>(),
                ) {
                    array_tree.push(self.generate_package_tree(
                        package.clone(),
                        &installed_repo,
                        &repos,
                    ));
                }
            }

            if format == "json" {
                self.get_io().write(&JsonFile::encode(&PackageTreeJson {
                    installed: array_tree,
                })?);
            } else {
                self.display_package_tree(array_tree);
            }

            return Ok(0);
        }

        // list packages
        let mut packages: IndexMap<BString, IndexMap<BString, PackageOrName>> = IndexMap::new();
        let mut package_filter_regex: Option<BString> = None;
        if let Some(ref pf) = package_filter {
            let escaped = preg_quote(pf, None);
            package_filter_regex = Some(bconcat!("{^", str_replace("\\*", ".*?", escaped), "$}i"));
        }

        let mut package_list_filter: Option<Vec<BString>> = None;
        if input.borrow().get_option("direct")?.as_bool() == Some(true) {
            package_list_filter = Some(self.get_root_requires());
        }

        if input.borrow().get_option("path")?.as_bool() == Some(true) && composer.is_none() {
            self.get_io().write_error(
                "No composer.json found in the current directory, disabling \"path\" option",
            );
            input
                .borrow_mut()
                .set_option("path", InputValue::Bool(false));
        }

        for repo in RepositoryUtils::flatten_repositories(repos, true) {
            let r#type = if Self::same_repository(&repo, &platform_repo) {
                "platform"
            } else if locked_repo
                .as_ref()
                .is_some_and(|lr| Self::same_repository(&repo, lr))
            {
                "locked"
            } else if Self::same_repository(&repo, &installed_repo)
                || installed_repo
                    .borrow()
                    .as_any()
                    .downcast_ref::<InstalledRepository>()
                    .is_some_and(|ir| ir.get_repositories().iter().any(|r| r.ptr_eq(&repo)))
            {
                "installed"
            } else {
                "available"
            };
            let type_owned = BString::from(r#type);
            if let Some(cr_rc) = repo.downcast_rc::<ComposerRepository>() {
                let names = cr_rc
                    .borrow_mut()
                    .get_package_names(package_filter.as_ref().map(BStr::new))?;
                for name in names {
                    packages
                        .entry(type_owned.clone())
                        .or_default()
                        .insert(name.clone(), PackageOrName::Name(name));
                }
            } else {
                for package in repo.get_packages()? {
                    let existing = packages
                        .get(&type_owned)
                        .and_then(|m| m.get(&package.get_name()));
                    let need_replace = match existing {
                        None => true,
                        Some(PackageOrName::Name(_)) => true,
                        Some(PackageOrName::Pkg(existing)) => version_compare(
                            existing.get_version(),
                            package.get_version(),
                            CmpOp::Lt,
                        ),
                    };
                    if need_replace {
                        let mut p: crate::package::PackageInterfaceHandle = package.clone();
                        while let Some(alias) = p.as_alias() {
                            p = alias.get_alias_of().into();
                        }
                        let matches_filter = match &package_filter_regex {
                            None => true,
                            Some(r) => preg_is_match(r, p.get_name()),
                        };
                        if matches_filter {
                            let matches_list = match &package_list_filter {
                                None => true,
                                Some(list) => in_array_strict(
                                    p.get_name(),
                                    &list
                                        .iter()
                                        .map(|s| PhpMixed::string(s.clone()))
                                        .collect::<Vec<_>>(),
                                ),
                            };
                            if matches_list {
                                packages
                                    .entry(type_owned.clone())
                                    .or_default()
                                    .insert(p.get_name(), PackageOrName::Pkg(p));
                            }
                        }
                    }
                }
                if Self::same_repository(&repo, &platform_repo) {
                    for (name, p) in platform_repo.borrow().get_disabled_packages() {
                        packages
                            .entry(type_owned.clone())
                            .or_default()
                            .insert(name.clone(), PackageOrName::Pkg(p.clone().into()));
                    }
                }
            }
        }

        let show_all_types = input.borrow().get_option("all")?.as_bool() == Some(true);
        let show_latest = input.borrow().get_option("latest")?.as_bool() == Some(true);
        let show_major_only = input.borrow().get_option("major-only")?.as_bool() == Some(true);
        let show_minor_only = input.borrow().get_option("minor-only")?.as_bool() == Some(true);
        let show_patch_only = input.borrow().get_option("patch-only")?.as_bool() == Some(true);
        let ignored_packages_regex = base_package::package_names_to_regexp(
            &input
                .borrow()
                .get_option("ignore")?
                .as_array()
                .map(|l| l.iter().map(strtolower).collect::<Vec<_>>())
                .unwrap_or_default(),
            BStr::new("{^(?:%s)$}iD"),
        );
        let indent = BStr::new(if show_all_types { "  " } else { "" });
        let mut latest_packages: IndexMap<BString, crate::package::PackageInterfaceHandle> =
            IndexMap::new();
        let mut exit_code: i64 = 0;
        let mut view_data: IndexMap<BString, Vec<PackageViewData>> = IndexMap::new();
        let mut view_meta_data: IndexMap<BString, ViewMetaData> = IndexMap::new();

        let mut write_version = false;
        let mut write_description = false;

        let type_order: Vec<(&BStr, bool)> = vec![
            (BStr::new("platform"), true),
            (BStr::new("locked"), true),
            (BStr::new("available"), false),
            (BStr::new("installed"), true),
        ];
        for (r#type, show_version) in type_order.iter() {
            if let Some(type_packages) = packages.get_mut(*r#type) {
                type_packages.sort_keys();

                let mut name_length: usize = 0;
                let mut version_length: usize = 0;
                let mut latest_length: usize = 0;
                let mut release_date_length: usize = 0;

                if show_latest && *show_version {
                    for package_or_name in type_packages.values() {
                        if let PackageOrName::Pkg(package) = package_or_name
                            && !preg_is_match(&ignored_packages_regex, package.get_pretty_name())
                        {
                            let latest = self.find_latest_package(
                                package.clone(),
                                composer.as_ref().unwrap(),
                                &platform_repo,
                                show_major_only,
                                show_minor_only,
                                show_patch_only,
                                platform_req_filter.clone(),
                            )?;
                            if latest.is_none() {
                                continue;
                            }

                            latest_packages.insert(package.get_pretty_name(), latest.unwrap());
                        }
                    }
                }

                let write_path = input.borrow().get_option("name-only")?.as_bool() != Some(true)
                    && input.borrow().get_option("path")?.as_bool() == Some(true);
                write_version = input.borrow().get_option("name-only")?.as_bool() != Some(true)
                    && input.borrow().get_option("path")?.as_bool() != Some(true)
                    && *show_version;
                let write_latest = write_version && show_latest;
                write_description = input.borrow().get_option("name-only")?.as_bool() != Some(true)
                    && input.borrow().get_option("path")?.as_bool() != Some(true);
                let write_release_date = write_latest
                    && (input.borrow().get_option("sort-by-age")?.as_bool() == Some(true)
                        || format == "json");

                let mut has_outdated_packages = false;

                if input.borrow().get_option("sort-by-age")?.as_bool() == Some(true) {
                    type_packages.sort_by(|_ka, a, _kb, b| match (a, b) {
                        (PackageOrName::Pkg(a), PackageOrName::Pkg(b)) => {
                            a.get_release_date().cmp(&b.get_release_date())
                        }
                        _ => std::cmp::Ordering::Equal,
                    });
                }

                let mut view_type: Vec<PackageViewData> = Vec::new();
                for package_or_name in type_packages.values() {
                    let mut package_view_data = PackageViewData::default();
                    if let PackageOrName::Pkg(package) = package_or_name {
                        let latest_package = if show_latest
                            && latest_packages.contains_key(&package.get_pretty_name())
                        {
                            latest_packages.get(&package.get_pretty_name())
                        } else {
                            None
                        };

                        // Determine if Composer is checking outdated dependencies and if current package should trigger non-default exit code
                        let mut package_is_up_to_date = if let Some(latest) = latest_package {
                            latest.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            ) == package.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            ) && latest.as_complete().is_none_or(|c| !c.is_abandoned())
                        } else {
                            false
                        };
                        // When using --major-only, and no bigger version than current major is found then it is considered up to date
                        package_is_up_to_date =
                            package_is_up_to_date || (latest_package.is_none() && show_major_only);
                        let package_is_ignored =
                            preg_is_match(&ignored_packages_regex, package.get_pretty_name());
                        if input.borrow().get_option("outdated")?.as_bool() == Some(true)
                            && (package_is_up_to_date || package_is_ignored)
                        {
                            continue;
                        }

                        if input.borrow().get_option("outdated")?.as_bool() == Some(true)
                            || input.borrow().get_option("strict")?.as_bool() == Some(true)
                        {
                            has_outdated_packages = true;
                        }

                        package_view_data.name = package.get_pretty_name();
                        package_view_data.direct_dependency = Some(in_array_strict(
                            package.get_name(),
                            &self
                                .get_root_requires()
                                .into_iter()
                                .map(PhpMixed::string)
                                .collect::<Vec<_>>(),
                        ));
                        if format != "json"
                            || input.borrow().get_option("name-only")?.as_bool() != Some(true)
                        {
                            package_view_data.homepage =
                                Some(package.as_complete().and_then(|c| c.get_homepage()));
                            package_view_data.source =
                                Some(PackageInfo::get_view_source_url(package.clone()));
                        }
                        name_length = name_length.max(package_view_data.name.len());
                        if write_version {
                            let mut version_str = package.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            );
                            if format == "text" {
                                version_str = ltrim2(&version_str, "v");
                            }
                            version_length = version_length.max(version_str.len());
                            package_view_data.version = Some(version_str);
                        }
                        if write_release_date {
                            if let Some(release_date) = package.get_release_date() {
                                let mut age = str_replace(
                                    " ago",
                                    " old",
                                    self.get_relative_time(&release_date),
                                );
                                if !age.contains_str(" old") {
                                    age = bconcat!("from ", age);
                                }
                                release_date_length = release_date_length.max(age.len());
                                package_view_data.release_age = Some(age);
                                package_view_data.release_date =
                                    Some(display_to_bytes(release_date.format(DATE_ATOM)));
                            } else {
                                package_view_data.release_age = Some(BString::default());
                                package_view_data.release_date = Some(BString::default());
                            }
                        }
                        if write_latest && let Some(latest) = latest_package {
                            let mut latest_version_str = latest.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            );
                            if format == "text" {
                                latest_version_str = ltrim2(&latest_version_str, "v");
                            }
                            let update_status =
                                Self::get_update_status(latest.clone(), package.clone())?;
                            latest_length = latest_length.max(latest_version_str.len());
                            package_view_data.latest = Some(latest_version_str);
                            package_view_data.latest_status = Some(update_status);

                            if let Some(rd) = latest.get_release_date() {
                                package_view_data.latest_release_date =
                                    Some(display_to_bytes(rd.format(DATE_ATOM)));
                            } else {
                                package_view_data.latest_release_date = Some(BString::default());
                            }
                        } else if write_latest {
                            package_view_data.latest = Some(BString::from("[none matched]"));
                            package_view_data.latest_status = Some(BString::from("up-to-date"));
                            latest_length = latest_length.max("[none matched]".len());
                        }
                        if write_description && let Some(c) = package.as_complete() {
                            package_view_data.description = Some(c.get_description());
                        }
                        if write_path {
                            let installation_manager = composer
                                .as_ref()
                                .unwrap()
                                .borrow_partial()
                                .get_installation_manager();
                            let path = installation_manager
                                .borrow_mut()
                                .get_install_path(package.clone());
                            if let Some(p) = path {
                                let r = realpath(&p).unwrap_or_default();
                                package_view_data.path =
                                    Some(Some(BString::from(first_token(path_to_bytes(&r)))));
                            } else {
                                package_view_data.path = Some(None);
                            }
                        }

                        let mut package_is_abandoned = AbandonedState::Flag(false);
                        if let Some(latest) = latest_package
                            && let Some(c) = latest.as_complete()
                            && c.is_abandoned()
                        {
                            let replacement_package_name = c.get_replacement_package();
                            let replacement = if let Some(ref rp) = replacement_package_name {
                                bconcat!("Use ", rp, " instead")
                            } else {
                                BString::from("No replacement was suggested")
                            };
                            let package_warning = bconcat!(
                                "Package ",
                                package.get_pretty_name(),
                                " is abandoned, you should avoid using it. ",
                                replacement,
                                "."
                            );
                            package_view_data.warning = Some(package_warning);
                            package_is_abandoned = match replacement_package_name {
                                Some(rp) => AbandonedState::Replacement(rp),
                                None => AbandonedState::Flag(true),
                            };
                        }

                        package_view_data.abandoned = Some(package_is_abandoned);
                    } else if let PackageOrName::Name(name) = package_or_name {
                        package_view_data.name = name.clone();
                        name_length = name_length.max(name.len());
                    }
                    view_type.push(package_view_data);
                }
                view_data.insert((*r#type).to_owned(), view_type);
                view_meta_data.insert(
                    (*r#type).to_owned(),
                    ViewMetaData {
                        name_length,
                        version_length,
                        latest_length,
                        release_date_length,
                        write_latest,
                        write_release_date,
                    },
                );
                if input.borrow().get_option("strict")?.as_bool() == Some(true)
                    && has_outdated_packages
                {
                    exit_code = 1;
                    break;
                }
            }
        }

        if format == "json" {
            let io = self.get_io();
            io.write(&JsonFile::encode(&ViewDataJson(&view_data))?);
        } else {
            if input.borrow().get_option("latest")?.as_bool() == Some(true)
                && view_data.values().any(|v| !v.is_empty())
            {
                let io = self.get_io();
                if !io.is_decorated() {
                    io.write_error("Legend:");
                    io.write_error("! patch or minor release available - update recommended");
                    io.write_error("~ major release available - update possible");
                    if input.borrow().get_option("outdated")?.as_bool() != Some(true) {
                        io.write_error("= up to date version");
                    }
                } else {
                    io.write_error("<info>Color legend:</info>");
                    io.write_error("- <highlight>patch or minor</highlight> release available - update recommended");
                    io.write_error(
                        "- <comment>major</comment> release available - update possible",
                    );
                    if input.borrow().get_option("outdated")?.as_bool() != Some(true) {
                        io.write_error("- <info>up to date</info> version");
                    }
                }
            }

            let width = self.get_terminal_width();

            for (r#type, packages) in view_data.iter() {
                let meta = match view_meta_data.get(r#type) {
                    Some(m) => m.clone(),
                    None => continue,
                };
                let name_length = meta.name_length;
                let version_length = meta.version_length;
                let mut latest_length = meta.latest_length;
                let release_date_length = meta.release_date_length;
                let write_latest = meta.write_latest;
                let write_release_date = meta.write_release_date;

                let width_usize = width as usize;
                let version_fits = name_length + version_length + 3 <= width_usize;
                let latest_fits = name_length + version_length + latest_length + 3 <= width_usize;
                let release_date_fits =
                    name_length + version_length + latest_length + release_date_length + 3
                        <= width_usize;
                let description_fits =
                    name_length + version_length + latest_length + release_date_length + 24
                        <= width_usize;

                if latest_fits && !self.get_io().is_decorated() {
                    latest_length += 2;
                }

                if show_all_types {
                    if r#type == "available" {
                        self.get_io()
                            .write(bconcat!("<comment>", r#type, "</comment>:"));
                    } else {
                        self.get_io().write(bconcat!("<info>", r#type, "</info>:"));
                    }
                }

                if write_latest && input.borrow().get_option("direct")?.as_bool() != Some(true) {
                    let mut direct_deps: Vec<PackageViewData> = Vec::new();
                    let mut transitive_deps: Vec<PackageViewData> = Vec::new();
                    for pkg in packages.iter() {
                        let is_direct = pkg.direct_dependency.unwrap_or(false);
                        if is_direct {
                            direct_deps.push(pkg.clone());
                        } else {
                            transitive_deps.push(pkg.clone());
                        }
                    }

                    self.get_io().write_error("");
                    self.get_io()
                        .write_error("<info>Direct dependencies required in composer.json:</>");
                    if !direct_deps.is_empty() {
                        self.print_packages(
                            &direct_deps,
                            indent,
                            write_version && version_fits,
                            latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    } else {
                        self.get_io().write_error("Everything up to date");
                    }
                    self.get_io().write_error("");
                    self.get_io().write_error(
                        "<info>Transitive dependencies not required in composer.json:</>",
                    );
                    if !transitive_deps.is_empty() {
                        self.print_packages(
                            &transitive_deps,
                            indent,
                            write_version && version_fits,
                            latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    } else {
                        self.get_io().write_error("Everything up to date");
                    }
                } else {
                    if write_latest && packages.is_empty() {
                        self.get_io()
                            .write_error("All your direct dependencies are up to date");
                    } else {
                        self.print_packages(
                            packages,
                            indent,
                            write_version && version_fits,
                            write_latest && latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    }
                }

                if show_all_types {
                    self.get_io().write("");
                }
            }
        }

        Ok(exit_code)
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ShowCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

#[derive(Debug)]
pub enum PackageOrName {
    Pkg(crate::package::PackageInterfaceHandle),
    Name(BString),
}

/// Shape of `show <package> --format=json` output.
#[derive(Debug, serde::Serialize)]
struct PackageInfoJson {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::option_str")]
    description: Option<BString>,
    #[serde(serialize_with = "json_bytes::str_vec")]
    keywords: Vec<BString>,
    #[serde(serialize_with = "json_bytes::str")]
    r#type: BString,
    #[serde(serialize_with = "json_bytes::option_str")]
    homepage: Option<BString>,
    #[serde(serialize_with = "json_bytes::str_vec")]
    names: Vec<BString>,
    #[serde(serialize_with = "json_bytes::str_vec")]
    versions: Vec<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    licenses: Option<Vec<LicenseJson>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    latest: Option<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<PackageReferenceJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dist: Option<PackageReferenceJson>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    path: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    released: Option<BString>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    replacement: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    suggests: Option<IndexMap<BString, BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    support: Option<IndexMap<BString, BString>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    autoload: Option<PhpMixed>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_vec"
    )]
    include_path: Option<Vec<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    requires: Option<IndexMap<BString, BString>>,
    #[serde(
        rename = "devRequires",
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    dev_requires: Option<IndexMap<BString, BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    provides: Option<IndexMap<BString, BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    conflicts: Option<IndexMap<BString, BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "option_str_map"
    )]
    replaces: Option<IndexMap<BString, BString>>,
}

#[derive(Debug, serde::Serialize)]
struct PackageReferenceJson {
    #[serde(serialize_with = "json_bytes::str")]
    r#type: BString,
    #[serde(serialize_with = "json_bytes::option_str")]
    url: Option<BString>,
    #[serde(serialize_with = "json_bytes::option_str")]
    reference: Option<BString>,
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
enum LicenseJson {
    Id(#[serde(serialize_with = "json_bytes::str")] BString),
    Detail {
        #[serde(serialize_with = "json_bytes::str")]
        name: BString,
        #[serde(serialize_with = "json_bytes::str")]
        osi: BString,
        #[serde(serialize_with = "json_bytes::str")]
        url: BString,
    },
}

/// Shape of `show --tree --format=json` output.
#[derive(Debug, serde::Serialize)]
struct PackageTreeJson {
    installed: Vec<PackageTree>,
}

#[derive(Debug, serde::Serialize)]
struct PackageTree {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::str")]
    version: BString,
    #[serde(serialize_with = "json_bytes::option_str")]
    description: Option<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires: Option<Vec<PackageTreeChild>>,
}

#[derive(Debug, serde::Serialize)]
struct PackageTreeChild {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::str")]
    version: BString,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires: Option<Vec<PackageTreeChild>>,
}

/// Shape of the package list `--format=json` output. PHP `json_encode` emits an empty
/// associative array as `[]`, not `{}`; mirror that for the top-level map.
#[derive(Debug)]
struct ViewDataJson<'a>(&'a IndexMap<BString, Vec<PackageViewData>>);

#[derive(Debug)]
struct ViewDataKey<'a>(&'a BString);

impl serde::Serialize for ViewDataKey<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        json_bytes::str(self.0, serializer)
    }
}

impl serde::Serialize for ViewDataJson<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if self.0.is_empty() {
            use serde::ser::SerializeSeq;
            return serializer.serialize_seq(Some(0))?.end();
        }
        serializer.collect_map(self.0.iter().map(|(key, value)| (ViewDataKey(key), value)))
    }
}

/// One entry of the package list, shared by the text renderer and the `--format=json` output.
/// In a doubly wrapped field the outer `Option` marks key presence and the inner one the value,
/// which PHP `isset()` and `array_key_exists()` tell apart.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct PackageViewData {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(rename = "direct-dependency", skip_serializing_if = "Option::is_none")]
    direct_dependency: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    homepage: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    source: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    version: Option<BString>,
    #[serde(
        rename = "release-age",
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    release_age: Option<BString>,
    #[serde(
        rename = "release-date",
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    release_date: Option<BString>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    latest: Option<BString>,
    #[serde(
        rename = "latest-status",
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    latest_status: Option<BString>,
    #[serde(
        rename = "latest-release-date",
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    latest_release_date: Option<BString>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    description: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "nested_option_str"
    )]
    path: Option<Option<BString>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "json_bytes::option_str"
    )]
    warning: Option<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    abandoned: Option<AbandonedState>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
enum AbandonedState {
    Flag(bool),
    Replacement(#[serde(serialize_with = "json_bytes::str")] BString),
}

#[derive(Debug, Clone)]
struct ViewMetaData {
    name_length: usize,
    version_length: usize,
    latest_length: usize,
    release_date_length: usize,
    write_latest: bool,
    write_release_date: bool,
}

fn nested_option_str<S: serde::Serializer>(
    value: &Option<Option<BString>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    json_bytes::option_str(value.as_ref().unwrap_or(&None), serializer)
}

fn option_str_vec<S: serde::Serializer>(
    value: &Option<Vec<BString>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => json_bytes::str_vec(value, serializer),
        None => serializer.serialize_none(),
    }
}

fn option_str_map<S: serde::Serializer>(
    value: &Option<IndexMap<BString, BString>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => json_bytes::str_map(value, serializer),
        None => serializer.serialize_none(),
    }
}

/// PHP's `strtok($string, "\r\n")`: the first token, skipping leading line breaks.
fn first_token(string: &[u8]) -> &BStr {
    let is_delimiter = |byte: &u8| matches!(byte, b'\r' | b'\n');
    let start = string
        .iter()
        .position(|byte| !is_delimiter(byte))
        .unwrap_or(string.len());
    let token = &string[start..];
    let end = token.iter().position(is_delimiter).unwrap_or(token.len());
    token[..end].as_bstr()
}
