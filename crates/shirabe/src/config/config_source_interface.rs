//! ref: composer/src/Composer/Config/ConfigSourceInterface.php sha1:408ad9c3d7093415ff11eeb5630d6e1cabefa60a

use bstr::{BStr, BString};
use shirabe_php_shim::PhpMixed;

pub trait ConfigSourceInterface: std::fmt::Debug {
    fn add_repository(&self, name: &BStr, config: PhpMixed, append: bool) -> anyhow::Result<()>;

    fn insert_repository(
        &self,
        name: &BStr,
        config: PhpMixed,
        reference_name: &BStr,
        offset: i64,
    ) -> anyhow::Result<()>;

    fn set_repository_url(&self, name: &BStr, url: &BStr) -> anyhow::Result<()>;

    fn remove_repository(&self, name: &BStr) -> anyhow::Result<()>;

    fn add_config_setting(&self, name: &BStr, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_config_setting(&self, name: &BStr) -> anyhow::Result<()>;

    fn add_property(&self, name: &BStr, value: PhpMixed) -> anyhow::Result<()>;

    fn remove_property(&self, name: &BStr) -> anyhow::Result<()>;

    fn add_link(&self, r#type: &BStr, name: &BStr, value: &BStr) -> anyhow::Result<()>;

    fn remove_link(&self, r#type: &BStr, name: &BStr) -> anyhow::Result<()>;

    fn get_name(&self) -> BString;
}
