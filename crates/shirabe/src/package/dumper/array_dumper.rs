//! ref: composer/src/Composer/Package/Dumper/ArrayDumper.php

use indexmap::IndexMap;
use shirabe_php_shim::{DATE_RFC3339, PhpMixed};

use crate::package::Mirror;
use crate::package::PackageInterfaceHandle;
use crate::package::SUPPORTED_LINK_TYPES;

/// Serializes a Mirror back into the PHP array shape `{url, preferred}`.
fn mirror_to_php(mirror: Mirror) -> PhpMixed {
    let mut entry: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
    entry.insert("url".to_string(), Box::new(PhpMixed::String(mirror.url)));
    entry.insert(
        "preferred".to_string(),
        Box::new(PhpMixed::Bool(mirror.preferred)),
    );
    PhpMixed::Array(entry)
}

#[derive(Debug)]
pub struct ArrayDumper;

impl ArrayDumper {
    pub fn new() -> Self {
        Self
    }

    pub fn dump(&self, package: PackageInterfaceHandle) -> IndexMap<String, PhpMixed> {
        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert(
            "name".to_string(),
            PhpMixed::String(package.get_pretty_name().to_string()),
        );
        data.insert(
            "version".to_string(),
            PhpMixed::String(package.get_pretty_version().to_string()),
        );
        data.insert(
            "version_normalized".to_string(),
            PhpMixed::String(package.get_version().to_string()),
        );

        if let Some(target_dir) = package.get_target_dir() {
            data.insert(
                "target-dir".to_string(),
                PhpMixed::String(target_dir.to_string()),
            );
        }

        if let Some(source_type) = package.get_source_type() {
            let mut source: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            source.insert(
                "type".to_string(),
                Box::new(PhpMixed::String(source_type.to_string())),
            );
            source.insert(
                "url".to_string(),
                Box::new(PhpMixed::String(
                    package.get_source_url().unwrap_or_default(),
                )),
            );
            if let Some(reference) = package.get_source_reference() {
                source.insert(
                    "reference".to_string(),
                    Box::new(PhpMixed::String(reference.to_string())),
                );
            }
            if let Some(mirrors) = package.get_source_mirrors() {
                if !mirrors.is_empty() {
                    source.insert(
                        "mirrors".to_string(),
                        Box::new(PhpMixed::Array(
                            mirrors
                                .into_iter()
                                .enumerate()
                                .map(|(i, m)| (i.to_string(), Box::new(mirror_to_php(m))))
                                .collect(),
                        )),
                    );
                }
            }
            data.insert("source".to_string(), PhpMixed::Array(source));
        }

        if let Some(dist_type) = package.get_dist_type() {
            let mut dist: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            dist.insert(
                "type".to_string(),
                Box::new(PhpMixed::String(dist_type.to_string())),
            );
            dist.insert(
                "url".to_string(),
                Box::new(PhpMixed::String(package.get_dist_url().unwrap_or_default())),
            );
            if let Some(reference) = package.get_dist_reference() {
                dist.insert(
                    "reference".to_string(),
                    Box::new(PhpMixed::String(reference.to_string())),
                );
            }
            if let Some(shasum) = package.get_dist_sha1_checksum() {
                dist.insert(
                    "shasum".to_string(),
                    Box::new(PhpMixed::String(shasum.to_string())),
                );
            }
            if let Some(mirrors) = package.get_dist_mirrors() {
                if !mirrors.is_empty() {
                    dist.insert(
                        "mirrors".to_string(),
                        Box::new(PhpMixed::Array(
                            mirrors
                                .into_iter()
                                .enumerate()
                                .map(|(i, m)| (i.to_string(), Box::new(mirror_to_php(m))))
                                .collect(),
                        )),
                    );
                }
            }
            data.insert("dist".to_string(), PhpMixed::Array(dist));
        }

        for type_name in SUPPORTED_LINK_TYPES.keys() {
            let links = package.get_links_for_type(type_name);
            if links.is_empty() {
                continue;
            }
            let mut link_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            for link in links.values() {
                link_map.insert(
                    link.get_target().to_string(),
                    Box::new(PhpMixed::String(link.get_pretty_constraint().to_string())),
                );
            }
            link_map.sort_keys();
            data.insert(type_name.to_string(), PhpMixed::Array(link_map));
        }

        let suggests = package.get_suggests();
        if !suggests.is_empty() {
            let mut sorted_suggests = suggests.clone();
            sorted_suggests.sort_keys();
            data.insert(
                "suggest".to_string(),
                PhpMixed::Array(
                    sorted_suggests
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                        .collect(),
                ),
            );
        }

        if let Some(release_date) = package.get_release_date() {
            data.insert(
                "time".to_string(),
                PhpMixed::String(release_date.format(DATE_RFC3339).to_string()),
            );
        }

        if package.is_default_branch() {
            data.insert("default-branch".to_string(), PhpMixed::Bool(true));
        }

        // dumpValues for base package keys (corresponds to dynamic PHP dispatch)
        let binaries = package.get_binaries();
        if !binaries.is_empty() {
            data.insert(
                "bin".to_string(),
                PhpMixed::List(
                    binaries
                        .into_iter()
                        .map(|b| Box::new(PhpMixed::String(b)))
                        .collect(),
                ),
            );
        }
        let pkg_type = package.get_type();
        if !pkg_type.is_empty() {
            data.insert("type".to_string(), PhpMixed::String(pkg_type.to_string()));
        }
        let extra = package.get_extra();
        if !extra.is_empty() {
            data.insert(
                "extra".to_string(),
                PhpMixed::Array(extra.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }
        if let Some(installation_source) = package.get_installation_source() {
            data.insert(
                "installation-source".to_string(),
                PhpMixed::String(installation_source.to_string()),
            );
        }
        let autoload = package.get_autoload();
        if !autoload.is_empty() {
            data.insert(
                "autoload".to_string(),
                PhpMixed::Array(
                    autoload
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
        }
        let dev_autoload = package.get_dev_autoload();
        if !dev_autoload.is_empty() {
            data.insert(
                "autoload-dev".to_string(),
                PhpMixed::Array(
                    dev_autoload
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
        }
        if let Some(notification_url) = package.get_notification_url() {
            data.insert(
                "notification-url".to_string(),
                PhpMixed::String(notification_url.to_string()),
            );
        }
        let include_paths = package.get_include_paths();
        if !include_paths.is_empty() {
            data.insert(
                "include-path".to_string(),
                PhpMixed::List(
                    include_paths
                        .into_iter()
                        .map(|p| Box::new(PhpMixed::String(p)))
                        .collect(),
                ),
            );
        }
        let php_ext = package.get_php_ext();
        if let Some(php_ext) = php_ext.as_ref().filter(|m| !m.is_empty()) {
            data.insert(
                "php-ext".to_string(),
                PhpMixed::Array(
                    php_ext
                        .iter()
                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                        .collect(),
                ),
            );
        }

        if let Some(complete_pkg) = package.as_complete() {
            if let Some(archive_name) = complete_pkg.get_archive_name() {
                let entry = data
                    .entry("archive".to_string())
                    .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                if let PhpMixed::Array(archive) = entry {
                    archive.insert(
                        "name".to_string(),
                        Box::new(PhpMixed::String(archive_name.to_string())),
                    );
                }
            }
            let archive_excludes = complete_pkg.get_archive_excludes();
            if !archive_excludes.is_empty() {
                let entry = data
                    .entry("archive".to_string())
                    .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                if let PhpMixed::Array(archive) = entry {
                    archive.insert(
                        "exclude".to_string(),
                        Box::new(PhpMixed::List(
                            archive_excludes
                                .into_iter()
                                .map(|e| Box::new(PhpMixed::String(e)))
                                .collect(),
                        )),
                    );
                }
            }

            // dumpValues for complete package keys
            let scripts = complete_pkg.get_scripts();
            if !scripts.is_empty() {
                data.insert(
                    "scripts".to_string(),
                    PhpMixed::Array(
                        scripts
                            .into_iter()
                            .map(|(k, v)| {
                                (
                                    k,
                                    Box::new(PhpMixed::List(
                                        v.into_iter()
                                            .map(|s| Box::new(PhpMixed::String(s)))
                                            .collect(),
                                    )),
                                )
                            })
                            .collect(),
                    ),
                );
            }
            let license = complete_pkg.get_license();
            if !license.is_empty() {
                data.insert(
                    "license".to_string(),
                    PhpMixed::List(
                        license
                            .into_iter()
                            .map(|l| Box::new(PhpMixed::String(l)))
                            .collect(),
                    ),
                );
            }
            let authors = complete_pkg.get_authors();
            if !authors.is_empty() {
                data.insert(
                    "authors".to_string(),
                    PhpMixed::List(
                        authors
                            .into_iter()
                            .map(|a| {
                                Box::new(PhpMixed::Array(
                                    a.into_iter()
                                        .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                        .collect(),
                                ))
                            })
                            .collect(),
                    ),
                );
            }
            if let Some(description) = complete_pkg.get_description() {
                data.insert(
                    "description".to_string(),
                    PhpMixed::String(description.to_string()),
                );
            }
            if let Some(homepage) = complete_pkg.get_homepage() {
                data.insert(
                    "homepage".to_string(),
                    PhpMixed::String(homepage.to_string()),
                );
            }
            let mut keywords = complete_pkg.get_keywords();
            if !keywords.is_empty() {
                keywords.sort();
                data.insert(
                    "keywords".to_string(),
                    PhpMixed::List(
                        keywords
                            .into_iter()
                            .map(|k| Box::new(PhpMixed::String(k)))
                            .collect(),
                    ),
                );
            }
            let repositories = complete_pkg.get_repositories();
            if !repositories.is_empty() {
                data.insert(
                    "repositories".to_string(),
                    PhpMixed::Array(
                        repositories
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(v)))
                            .collect(),
                    ),
                );
            }
            let support = complete_pkg.get_support();
            if !support.is_empty() {
                data.insert(
                    "support".to_string(),
                    PhpMixed::Array(
                        support
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                            .collect(),
                    ),
                );
            }
            let funding = complete_pkg.get_funding();
            if !funding.is_empty() {
                data.insert(
                    "funding".to_string(),
                    PhpMixed::List(
                        funding
                            .into_iter()
                            .map(|f| {
                                Box::new(PhpMixed::Array(
                                    f.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                                ))
                            })
                            .collect(),
                    ),
                );
            }

            if complete_pkg.is_abandoned() {
                let abandoned_value = complete_pkg
                    .get_replacement_package()
                    .map(|r| PhpMixed::String(r.to_string()))
                    .unwrap_or(PhpMixed::Bool(true));
                data.insert("abandoned".to_string(), abandoned_value);
            }
        }

        if let Some(root_pkg) = package.as_root() {
            let minimum_stability = root_pkg.get_minimum_stability();
            if !minimum_stability.is_empty() {
                data.insert(
                    "minimum-stability".to_string(),
                    PhpMixed::String(minimum_stability.to_string()),
                );
            }
        }

        let transport_options = package.get_transport_options();
        if !transport_options.is_empty() {
            data.insert(
                "transport-options".to_string(),
                PhpMixed::Array(
                    transport_options
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
        }

        data
    }
}
