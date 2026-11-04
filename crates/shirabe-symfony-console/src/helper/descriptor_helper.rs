//! ref: composer/vendor/symfony/console/Helper/DescriptorHelper.php sha1:fe292e81f5328111654359a6650dc6b87d18a2b1

use crate::descriptor::{
    DescribableObject, DescriptorInterface, DescriptorOptions, JsonDescriptor, MarkdownDescriptor,
    TextDescriptor, XmlDescriptor,
};
use crate::exception::InvalidArgumentException;
use crate::helper::{Helper, HelperInterface, HelperSet};
use crate::output::OutputInterface;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::bconcat;

/// This class adds helper method to describe objects in various formats.
#[derive(Default)]
pub struct DescriptorHelper {
    inner: Helper,
    descriptors: IndexMap<BString, Box<dyn DescriptorInterface>>,
}

// `DescriptorInterface` does not require `Debug`, so the derive cannot see
// through the trait object; provide a minimal manual impl.
impl std::fmt::Debug for DescriptorHelper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DescriptorHelper")
            .field("inner", &self.inner)
            .field("descriptors", &self.descriptors.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl DescriptorHelper {
    pub fn new() -> Self {
        let mut this = Self {
            inner: Helper::default(),
            descriptors: IndexMap::new(),
        };
        this.register("txt".into(), Box::new(TextDescriptor::default()))
            .register("xml".into(), Box::new(XmlDescriptor::default()))
            .register("json".into(), Box::new(JsonDescriptor::default()))
            .register("md".into(), Box::new(MarkdownDescriptor::default()));
        this
    }

    /// Describes an object if supported.
    ///
    /// Available options are:
    /// * format: string, the output format name
    /// * raw_text: boolean, sets output type as raw
    ///
    /// The defaults PHP merges into `$options` (`raw_text` false, `format` "txt") are those of
    /// `DescriptorOptions::default()`.
    pub fn describe2(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let format = options.format.clone();

        if !self.descriptors.contains_key(&format) {
            return Err(InvalidArgumentException::new(bconcat!(
                "Unsupported format \"",
                format,
                "\"."
            ))
            .into());
        }

        let descriptor = self.descriptors.get_mut(&format).unwrap();
        descriptor.describe(output, object, options)
    }

    /// Registers a descriptor.
    pub fn register(
        &mut self,
        format: &BStr,
        descriptor: Box<dyn DescriptorInterface>,
    ) -> &mut Self {
        self.descriptors.insert(format.to_owned(), descriptor);

        self
    }

    pub fn get_formats(&self) -> Vec<BString> {
        self.descriptors.keys().cloned().collect()
    }
}

impl HelperInterface for DescriptorHelper {
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> BString {
        "descriptor".into()
    }
}
