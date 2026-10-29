//! ref: composer/src/Composer/Package/Dumper/ArrayDumper.php

use crate::package::{Mirror, PackageInterfaceHandle, SUPPORTED_LINK_TYPES};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{DATE_RFC3339, PhpMixed, legacy_lossy};

/// Serializes a Mirror back into the PHP array shape `{url, preferred}`.
fn mirror_to_php(mirror: Mirror) -> PhpMixed {
    let mut entry: IndexMap<BString, PhpMixed> = IndexMap::new();
    entry.insert("url".into(), PhpMixed::string(mirror.url));
    entry.insert("preferred".into(), PhpMixed::Bool(mirror.preferred));
    PhpMixed::Array(entry)
}

#[derive(Debug)]
pub struct ArrayDumper;

impl Default for ArrayDumper {
    fn default() -> Self {
        Self::new()
    }
}

impl ArrayDumper {
    pub fn new() -> Self {
        Self
    }

    pub fn dump(&self, package: PackageInterfaceHandle) -> IndexMap<BString, PhpMixed> {
        let mut data: IndexMap<BString, PhpMixed> = IndexMap::new();
        data.insert("name".into(), PhpMixed::string(package.get_pretty_name()));
        data.insert(
            "version".into(),
            PhpMixed::string(package.get_pretty_version()),
        );
        data.insert(
            "version_normalized".into(),
            PhpMixed::string(package.get_version()),
        );

        if let Some(target_dir) = package.get_target_dir() {
            data.insert("target-dir".into(), PhpMixed::string(target_dir));
        }

        if let Some(source_type) = package.get_source_type() {
            let mut source = IndexMap::new();
            source.insert("type".into(), PhpMixed::string(source_type));
            source.insert(
                "url".into(),
                PhpMixed::string(package.get_source_url().unwrap_or_default()),
            );
            if let Some(reference) = package.get_source_reference() {
                source.insert("reference".into(), PhpMixed::string(reference));
            }
            if let Some(mirrors) = package.get_source_mirrors()
                && !mirrors.is_empty()
            {
                source.insert(
                    "mirrors".into(),
                    PhpMixed::List(mirrors.into_iter().map(mirror_to_php).collect()),
                );
            }
            data.insert("source".into(), PhpMixed::Array(source));
        }

        if let Some(dist_type) = package.get_dist_type() {
            let mut dist: IndexMap<BString, PhpMixed> = IndexMap::new();
            dist.insert("type".into(), PhpMixed::string(dist_type));
            dist.insert(
                "url".into(),
                PhpMixed::string(package.get_dist_url().unwrap_or_default()),
            );
            if let Some(reference) = package.get_dist_reference() {
                dist.insert("reference".into(), PhpMixed::string(reference));
            }
            if let Some(shasum) = package.get_dist_sha1_checksum() {
                dist.insert("shasum".into(), PhpMixed::string(shasum));
            }
            if let Some(mirrors) = package.get_dist_mirrors()
                && !mirrors.is_empty()
            {
                dist.insert(
                    "mirrors".into(),
                    PhpMixed::List(mirrors.into_iter().map(mirror_to_php).collect()),
                );
            }
            data.insert("dist".into(), PhpMixed::Array(dist));
        }

        for (type_name, opts) in SUPPORTED_LINK_TYPES.iter() {
            let links = package.get_links_for_type(opts.method);
            if links.is_empty() {
                continue;
            }
            let mut link_map: IndexMap<BString, PhpMixed> = IndexMap::new();
            for link in links.values() {
                link_map.insert(
                    legacy_lossy(link.get_target()).into_owned().into(),
                    PhpMixed::string(legacy_lossy(link.get_pretty_constraint()).into_owned()),
                );
            }
            link_map.sort_keys();
            data.insert(type_name.to_string().into(), PhpMixed::Array(link_map));
        }

        let suggests = package.get_suggests();
        if !suggests.is_empty() {
            let mut sorted_suggests = suggests;
            sorted_suggests.sort_keys();
            data.insert(
                "suggest".into(),
                PhpMixed::Array(
                    sorted_suggests
                        .into_iter()
                        .map(|(k, v)| (k, PhpMixed::string(v)))
                        .collect(),
                ),
            );
        }

        if let Some(release_date) = package.get_release_date() {
            data.insert(
                "time".into(),
                PhpMixed::string(release_date.format(DATE_RFC3339).to_string()),
            );
        }

        if package.is_default_branch() {
            data.insert("default-branch".into(), PhpMixed::Bool(true));
        }

        // dumpValues for base package keys (corresponds to dynamic PHP dispatch)
        let binaries = package.get_binaries();
        if !binaries.is_empty() {
            data.insert(
                "bin".into(),
                PhpMixed::List(binaries.into_iter().map(PhpMixed::string).collect()),
            );
        }
        let pkg_type = package.get_type();
        if !pkg_type.is_empty() {
            data.insert("type".into(), PhpMixed::string(pkg_type));
        }
        let extra = package.get_extra();
        if !extra.is_empty() {
            data.insert("extra".into(), PhpMixed::Array(extra.into_iter().collect()));
        }
        if let Some(installation_source) = package.get_installation_source() {
            data.insert(
                "installation-source".into(),
                PhpMixed::string(installation_source),
            );
        }
        let autoload = package.get_autoload();
        if !autoload.is_empty() {
            data.insert(
                "autoload".into(),
                PhpMixed::Array(autoload.into_iter().collect()),
            );
        }
        let dev_autoload = package.get_dev_autoload();
        if !dev_autoload.is_empty() {
            data.insert(
                "autoload-dev".into(),
                PhpMixed::Array(dev_autoload.into_iter().collect()),
            );
        }
        if let Some(notification_url) = package.get_notification_url() {
            data.insert(
                "notification-url".into(),
                PhpMixed::string(notification_url),
            );
        }
        let include_paths = package.get_include_paths();
        if !include_paths.is_empty() {
            data.insert(
                "include-path".into(),
                PhpMixed::List(include_paths.into_iter().map(PhpMixed::string).collect()),
            );
        }
        let php_ext = package.get_php_ext();
        if let Some(php_ext) = php_ext.as_ref().filter(|m| !m.is_empty()) {
            data.insert(
                "php-ext".into(),
                PhpMixed::Array(
                    php_ext
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                ),
            );
        }

        if let Some(complete_pkg) = package.as_complete() {
            if let Some(archive_name) = complete_pkg.get_archive_name() {
                let entry = data
                    .entry("archive".into())
                    .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                if let PhpMixed::Array(archive) = entry {
                    archive.insert("name".into(), PhpMixed::string(archive_name));
                }
            }
            let archive_excludes = complete_pkg.get_archive_excludes();
            if !archive_excludes.is_empty() {
                let entry = data
                    .entry("archive".into())
                    .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                if let PhpMixed::Array(archive) = entry {
                    archive.insert(
                        "exclude".into(),
                        PhpMixed::List(
                            archive_excludes.into_iter().map(PhpMixed::string).collect(),
                        ),
                    );
                }
            }

            // dumpValues for complete package keys
            let scripts = complete_pkg.get_scripts();
            if !scripts.is_empty() {
                data.insert(
                    "scripts".into(),
                    PhpMixed::Array(
                        scripts
                            .into_iter()
                            .map(|(k, v)| {
                                (
                                    k,
                                    PhpMixed::List(v.into_iter().map(PhpMixed::string).collect()),
                                )
                            })
                            .collect(),
                    ),
                );
            }
            let license = complete_pkg.get_license();
            if !license.is_empty() {
                data.insert(
                    "license".into(),
                    PhpMixed::List(license.into_iter().map(PhpMixed::string).collect()),
                );
            }
            let authors = complete_pkg.get_authors();
            if !authors.is_empty() {
                data.insert(
                    "authors".into(),
                    PhpMixed::List(
                        authors
                            .into_iter()
                            .map(|a| {
                                PhpMixed::Array(
                                    a.into_iter()
                                        .map(|(k, v)| (k, PhpMixed::string(v)))
                                        .collect(),
                                )
                            })
                            .collect(),
                    ),
                );
            }
            if let Some(description) = complete_pkg.get_description() {
                data.insert("description".into(), PhpMixed::string(description));
            }
            if let Some(homepage) = complete_pkg.get_homepage() {
                data.insert("homepage".into(), PhpMixed::string(homepage));
            }
            let mut keywords = complete_pkg.get_keywords();
            if !keywords.is_empty() {
                keywords.sort();
                data.insert(
                    "keywords".into(),
                    PhpMixed::List(keywords.into_iter().map(PhpMixed::string).collect()),
                );
            }
            let repositories = complete_pkg.get_repositories();
            if !repositories.is_empty() {
                data.insert(
                    "repositories".into(),
                    PhpMixed::Array(repositories.into_iter().collect()),
                );
            }
            let support = complete_pkg.get_support();
            if !support.is_empty() {
                data.insert(
                    "support".into(),
                    PhpMixed::Array(
                        support
                            .into_iter()
                            .map(|(k, v)| (k, PhpMixed::string(v)))
                            .collect(),
                    ),
                );
            }
            let funding = complete_pkg.get_funding();
            if !funding.is_empty() {
                data.insert(
                    "funding".into(),
                    PhpMixed::List(
                        funding
                            .into_iter()
                            .map(|f| PhpMixed::Array(f.into_iter().collect()))
                            .collect(),
                    ),
                );
            }

            if complete_pkg.is_abandoned() {
                let abandoned_value = complete_pkg
                    .get_replacement_package()
                    .map(PhpMixed::string)
                    .unwrap_or(PhpMixed::Bool(true));
                data.insert("abandoned".into(), abandoned_value);
            }
        }

        if let Some(root_pkg) = package.as_root() {
            let minimum_stability = root_pkg.get_minimum_stability();
            if !minimum_stability.is_empty() {
                data.insert(
                    "minimum-stability".into(),
                    PhpMixed::string(minimum_stability),
                );
            }
        }

        let transport_options = package.get_transport_options();
        if !transport_options.is_empty() {
            data.insert(
                "transport-options".into(),
                PhpMixed::Array(transport_options.into_iter().collect()),
            );
        }

        data
    }
}
