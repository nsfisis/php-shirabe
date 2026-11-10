//! ref: composer/src/Composer/Repository/Vcs/VcsDriverInterface.php sha1:4cbb9ae7d4a16682c229b47ba57e86006b770c2e

use crate::config::Config;
use crate::io::IOInterface;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait VcsDriverInterface: std::fmt::Debug {
    fn initialize(&mut self) -> anyhow::Result<()>;

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>>;

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>>;

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::FixedOffset>>>;

    fn get_root_identifier(&mut self) -> anyhow::Result<BString>;

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>>;

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>>;

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>>;

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString>;

    fn get_url(&self) -> BString;

    fn has_composer_file(&mut self, identifier: &BStr) -> anyhow::Result<bool>;

    fn cleanup(&mut self) -> anyhow::Result<()>;

    fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool>
    where
        Self: Sized;
}
