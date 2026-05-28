//! ref: composer/src/Composer/Repository/RepositoryUtils.php

use crate::package::Link;
use crate::repository::CompositeRepository;
use crate::repository::FilterRepository;
use crate::repository::RepositoryInterfaceHandle;
use indexmap::IndexMap;
use std::any::Any;

pub struct RepositoryUtils;

impl RepositoryUtils {
    pub fn filter_required_packages(
        packages: &[crate::package::BasePackageHandle],
        requirer: crate::package::PackageInterfaceHandle,
        include_require_dev: bool,
        mut bucket: Vec<crate::package::BasePackageHandle>,
    ) -> Vec<crate::package::BasePackageHandle> {
        let mut requires: IndexMap<String, Link> = requirer.get_requires();
        if include_require_dev {
            requires.extend(requirer.get_dev_requires());
        }

        for candidate in packages {
            for name in candidate.get_names(true) {
                if requires.contains_key(&name) {
                    let already_in_bucket = bucket.iter().any(|b| b.ptr_eq(candidate));
                    if !already_in_bucket {
                        bucket.push(candidate.clone());
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
        repo: RepositoryInterfaceHandle,
        unwrap_filter_repos: bool,
    ) -> Vec<RepositoryInterfaceHandle> {
        let repo: RepositoryInterfaceHandle = if unwrap_filter_repos {
            let unwrapped = {
                let r = repo.borrow();
                r.as_any()
                    .downcast_ref::<FilterRepository>()
                    .map(|filter_repo| filter_repo.get_repository())
            };
            unwrapped.unwrap_or(repo)
        } else {
            repo
        };

        let nested = {
            let r = repo.borrow();
            r.as_any()
                .downcast_ref::<CompositeRepository>()
                .map(|composite_repo| composite_repo.get_repositories().clone())
        };
        if let Some(nested) = nested {
            let mut repos = Vec::new();
            for r in nested {
                for r2 in Self::flatten_repositories(r, unwrap_filter_repos) {
                    repos.push(r2);
                }
            }
            repos
        } else {
            vec![repo]
        }
    }
}
