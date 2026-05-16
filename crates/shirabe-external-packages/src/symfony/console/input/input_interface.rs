use shirabe_php_shim::PhpMixed;
use indexmap::IndexMap;

pub trait InputInterface {
    fn get_first_argument(&self) -> Option<String>;
    fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool;
    fn get_parameter_option(&self, values: &[&str], default: PhpMixed, only_params: bool) -> PhpMixed;
    fn validate(&self) -> anyhow::Result<()>;
    fn get_arguments(&self) -> IndexMap<String, PhpMixed>;
    fn get_argument(&self, name: &str) -> PhpMixed;
    fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
    fn has_argument(&self, name: &str) -> bool;
    fn get_options(&self) -> IndexMap<String, PhpMixed>;
    fn get_option(&self, name: &str) -> PhpMixed;
    fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;
    fn has_option(&self, name: &str) -> bool;
    fn is_interactive(&self) -> bool;
    fn set_interactive(&mut self, interactive: bool);
}
