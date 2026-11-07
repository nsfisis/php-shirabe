//! ref: composer/src/Composer/Repository/RepositoryUtils.php sha1:65b85d34acfe21bd69cadf5c2a7371449060236f

use crate::package::{BasePackageHandle, Link, PackageInterfaceHandle};
use crate::repository::{
    CompositeRepository, FilterRepository, InstalledRepository, RepositoryInterfaceHandle,
};
use bstr::BString;
use indexmap::IndexMap;

pub struct RepositoryUtils;

impl RepositoryUtils {
    pub fn filter_required_packages(
        packages: &[BasePackageHandle],
        requirer: PackageInterfaceHandle,
        include_require_dev: bool,
        mut bucket: Vec<BasePackageHandle>,
    ) -> Vec<BasePackageHandle> {
        let mut requires: IndexMap<BString, Link> = (*requirer.get_requires()).clone();
        if include_require_dev {
            requires.extend(
                requirer
                    .get_dev_requires()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
        }

        for candidate in packages {
            for name in candidate.get_names(true) {
                if requires.contains_key(&name) {
                    let already_in_bucket = bucket.iter().any(|b| b.ptr_eq(candidate));
                    if !already_in_bucket {
                        bucket.push(candidate.clone());
                        bucket = Self::filter_required_packages(
                            packages,
                            candidate.clone(),
                            false,
                            bucket,
                        );
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

        // PHP `InstalledRepository extends CompositeRepository`, so it must flatten too. The Rust
        // port embeds the CompositeRepository instead of inheriting it, so the downcast is tried
        // explicitly here.
        let nested = {
            let r = repo.borrow();
            r.as_any()
                .downcast_ref::<CompositeRepository>()
                .map(|composite_repo| composite_repo.get_repositories().clone())
                .or_else(|| {
                    r.as_any()
                        .downcast_ref::<InstalledRepository>()
                        .map(|installed_repo| installed_repo.get_repositories().clone())
                })
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
