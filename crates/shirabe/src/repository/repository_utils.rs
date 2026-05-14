//! ref: composer/src/Composer/Repository/RepositoryUtils.php

use std::any::Any;
use indexmap::IndexMap;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::filter_repository::FilterRepository;
use crate::repository::repository_interface::RepositoryInterface;

pub struct RepositoryUtils;

impl RepositoryUtils {
    pub fn filter_required_packages(
        packages: &[Box<dyn PackageInterface>],
        requirer: &dyn PackageInterface,
        include_require_dev: bool,
        mut bucket: Vec<Box<dyn PackageInterface>>,
    ) -> Vec<Box<dyn PackageInterface>> {
        let mut requires: IndexMap<String, Link> = requirer.get_requires();
        if include_require_dev {
            requires.extend(requirer.get_dev_requires());
        }

        for candidate in packages {
            for name in candidate.get_names() {
                if requires.contains_key(&name) {
                    let already_in_bucket = bucket.iter().any(|b| {
                        std::ptr::eq(
                            b.as_ref() as *const dyn PackageInterface as *const (),
                            candidate.as_ref() as *const dyn PackageInterface as *const (),
                        )
                    });
                    if !already_in_bucket {
                        bucket.push(candidate.clone_box());
                        bucket = Self::filter_required_packages(packages, candidate.as_ref(), false, bucket);
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
            if let Some(filter_repo) = (repo.as_any() as &dyn Any).downcast_ref::<FilterRepository>() {
                filter_repo.get_repository()
            } else {
                repo
            }
        } else {
            repo
        };

        if let Some(composite_repo) = (repo.as_any() as &dyn Any).downcast_ref::<CompositeRepository>() {
            let mut repos = Vec::new();
            for r in composite_repo.get_repositories() {
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
