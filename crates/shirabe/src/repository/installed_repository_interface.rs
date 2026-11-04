//! ref: composer/src/Composer/Repository/InstalledRepositoryInterface.php sha1:d2bef71e67f31ca66fea22bd1981738d6eefedd5

use crate::repository::WritableRepositoryInterface;

pub trait InstalledRepositoryInterface: WritableRepositoryInterface {
    fn get_dev_mode(&self) -> Option<bool>;

    fn is_fresh(&self) -> bool;
}
