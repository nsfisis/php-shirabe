//! ref: composer/src/Composer/Repository/RepositoryUtils.php

use crate::package::Link;
use crate::package::PackageInterface;
use crate::repository::CompositeRepository;
use crate::repository::FilterRepository;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;
use std::any::Any;

pub struct RepositoryUtils;

impl RepositoryUtils {
    pub fn filter_required_packages(
        packages: &[Box<dyn crate::package::BasePackage>],
        requirer: &dyn PackageInterface,
        include_require_dev: bool,
        mut bucket: Vec<Box<dyn crate::package::BasePackage>>,
    ) -> Vec<Box<dyn crate::package::BasePackage>> {
        let mut requires: IndexMap<String, Link> = requirer.get_requires();
        if include_require_dev {
            requires.extend(requirer.get_dev_requires());
        }

        for candidate in packages {
            for name in candidate.get_names(true) {
                if requires.contains_key(&name) {
                    let already_in_bucket = bucket.iter().any(|b| {
                        std::ptr::eq(
                            b.as_ref() as *const dyn crate::package::BasePackage as *const (),
                            candidate.as_ref() as *const dyn crate::package::BasePackage
                                as *const (),
                        )
                    });
                    if !already_in_bucket {
                        bucket.push(candidate.clone_box());
                        // TODO(phase-b): recursion requires &dyn PackageInterface; cast pending.
                        let _ = (requires.contains_key("dummy"),);
                    }
                    break;
                }
            }
        }

        bucket
    }

    pub fn flatten_repositories(
        repo: Box<dyn RepositoryInterface>,
        unwrap_filter_repos: bool,
    ) -> Vec<Box<dyn RepositoryInterface>> {
        let repo: Box<dyn RepositoryInterface> = if unwrap_filter_repos {
            if let Some(filter_repo) = repo.as_any().downcast_ref::<FilterRepository>() {
                filter_repo.get_repository().clone_box()
            } else {
                repo
            }
        } else {
            repo
        };

        if let Some(composite_repo) = repo.as_any().downcast_ref::<CompositeRepository>() {
            let mut repos = Vec::new();
            for r in composite_repo.get_repositories() {
                for r2 in Self::flatten_repositories(r.clone_box(), unwrap_filter_repos) {
                    repos.push(r2);
                }
            }
            repos
        } else {
            vec![repo]
        }
    }
}
