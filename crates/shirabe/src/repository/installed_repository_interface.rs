//! ref: composer/src/Composer/Repository/InstalledRepositoryInterface.php

use crate::repository::WritableRepositoryInterface;

pub trait InstalledRepositoryInterface: WritableRepositoryInterface {
    fn get_dev_mode(&self) -> Option<bool>;

    fn is_fresh(&self) -> bool;

    fn clone_installed_repository_box(&self) -> Box<dyn InstalledRepositoryInterface> {
        todo!()
    }
}
