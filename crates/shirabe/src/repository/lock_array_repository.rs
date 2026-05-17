//! ref: composer/src/Composer/Repository/LockArrayRepository.php

use crate::repository::array_repository::ArrayRepository;
use crate::repository::canonical_packages_trait::CanonicalPackagesTrait;

#[derive(Debug)]
pub struct LockArrayRepository {
    inner: ArrayRepository,
}

impl CanonicalPackagesTrait for LockArrayRepository {
    fn get_packages(&self) -> Vec<Box<dyn crate::package::package_interface::PackageInterface>> {
        todo!()
    }
}

impl LockArrayRepository {
    pub fn get_repo_name(&self) -> &str {
        "lock repo"
    }
}
