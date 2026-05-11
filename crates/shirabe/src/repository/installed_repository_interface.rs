//! ref: composer/src/Composer/Repository/InstalledRepositoryInterface.php

use crate::repository::writable_repository_interface::WritableRepositoryInterface;

pub trait InstalledRepositoryInterface: WritableRepositoryInterface {
    fn get_dev_mode(&self) -> Option<bool>;

    fn is_fresh(&self) -> bool;
}
