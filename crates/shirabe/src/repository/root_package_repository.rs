//! ref: composer/src/Composer/Repository/RootPackageRepository.php

use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::array_repository::ArrayRepository;

#[derive(Debug)]
pub struct RootPackageRepository {
    inner: ArrayRepository,
}

impl RootPackageRepository {
    pub fn new(package: Box<dyn RootPackageInterface>) -> Self {
        Self {
            inner: ArrayRepository::new(vec![package]),
        }
    }

    pub fn get_repo_name(&self) -> String {
        "root package repo".to_string()
    }
}
