//! ref: composer/src/Composer/Repository/LockArrayRepository.php

use crate::repository::array_repository::ArrayRepository;
use crate::repository::canonical_packages_trait::CanonicalPackagesTrait;

#[derive(Debug)]
pub struct LockArrayRepository {
    inner: ArrayRepository,
}

impl CanonicalPackagesTrait for LockArrayRepository {}

impl LockArrayRepository {
    pub fn get_repo_name(&self) -> &str {
        "lock repo"
    }
}
