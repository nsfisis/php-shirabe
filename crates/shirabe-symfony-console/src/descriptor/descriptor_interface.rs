//! ref: composer/vendor/symfony/console/Descriptor/DescriptorInterface.php sha1:74859c5c3471d26fec6431236cbe230b4bb17e38

use crate::Application;
use crate::command::Command;
use crate::input::{InputArgument, InputDefinition, InputOption};
use crate::output::OutputInterface;
use bstr::BString;

/// The set of objects the descriptors know how to describe.
pub enum DescribableObject {
    InputArgument(InputArgument),
    InputOption(InputOption),
    InputDefinition(InputDefinition),
    Command(std::rc::Rc<std::cell::RefCell<dyn Command>>),
    Application(std::rc::Rc<std::cell::RefCell<dyn Application>>),
}

/// The `$options` array the descriptors read. PHP passes a plain array, and each descriptor
/// ignores the keys it does not read; this struct holds every key that one of them reads.
#[derive(Debug, Clone)]
pub struct DescriptorOptions {
    /// `format`: the name of the descriptor `DescriptorHelper` dispatches to.
    pub format: BString,
    /// `raw_text`: strips tags from the text output.
    pub raw_text: bool,
    /// `raw_output`: when set, the text output is written undecorated if it is true.
    pub raw_output: Option<bool>,
    /// `namespace`: restricts an application description to one namespace.
    pub namespace: Option<BString>,
    /// `short`: omits the usage, help and definition of commands.
    pub short: bool,
    /// `total_width`: the width of the first column of the text output.
    pub total_width: Option<i64>,
    /// `json_encoding`: the flags passed to `json_encode()`.
    pub json_encoding: i64,
}

impl Default for DescriptorOptions {
    /// The options as the descriptors read an empty array, with the defaults that
    /// `DescriptorHelper::describe()` merges in (`raw_text` false, `format` "txt").
    fn default() -> Self {
        Self {
            format: BString::from("txt"),
            raw_text: false,
            raw_output: None,
            namespace: None,
            short: false,
            total_width: None,
            json_encoding: 0,
        }
    }
}

/// Descriptor interface.
pub trait DescriptorInterface {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;
}
