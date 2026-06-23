//! ref: composer/vendor/symfony/console/Helper/DescriptorHelper.php

use crate::symfony::console::descriptor::descriptor_interface::{
    DescribableObject, DescriptorInterface,
};
use crate::symfony::console::descriptor::json_descriptor::JsonDescriptor;
use crate::symfony::console::descriptor::markdown_descriptor::MarkdownDescriptor;
use crate::symfony::console::descriptor::text_descriptor::TextDescriptor;
use crate::symfony::console::descriptor::xml_descriptor::XmlDescriptor;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

/// This class adds helper method to describe objects in various formats.
#[derive(Default)]
pub struct DescriptorHelper {
    inner: Helper,
    /// @var DescriptorInterface[]
    descriptors: IndexMap<String, Box<dyn DescriptorInterface>>,
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
        this.register("txt", Box::new(TextDescriptor::default()))
            .register("xml", Box::new(XmlDescriptor::default()))
            .register("json", Box::new(JsonDescriptor::default()))
            .register("md", Box::new(MarkdownDescriptor::default()));
        this
    }

    /// Describes an object if supported.
    ///
    /// Available options are:
    /// * format: string, the output format name
    /// * raw_text: boolean, sets output type as raw
    ///
    /// @throws InvalidArgumentException when the given format is not supported
    pub fn describe2(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: IndexMap<String, shirabe_php_shim::PhpMixed>,
    ) -> anyhow::Result<()> {
        let mut merged: IndexMap<String, shirabe_php_shim::PhpMixed> = IndexMap::new();
        merged.insert(
            "raw_text".to_string(),
            shirabe_php_shim::PhpMixed::Bool(false),
        );
        merged.insert(
            "format".to_string(),
            shirabe_php_shim::PhpMixed::String("txt".to_string()),
        );
        for (key, value) in options {
            merged.insert(key, value);
        }
        let options = merged;

        let format = match &options["format"] {
            shirabe_php_shim::PhpMixed::String(format) => format.clone(),
            _ => String::new(),
        };

        if !self.descriptors.contains_key(&format) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("Unsupported format \"{}\".", format.clone()),
                    code: 0,
                })
                .into(),
            );
        }

        let descriptor = self.descriptors.get_mut(&format).unwrap();
        descriptor.describe(output, object, options)
    }

    /// Registers a descriptor.
    ///
    /// @return $this
    pub fn register(
        &mut self,
        format: &str,
        descriptor: Box<dyn DescriptorInterface>,
    ) -> &mut Self {
        self.descriptors.insert(format.to_string(), descriptor);

        self
    }

    pub fn get_formats(&self) -> Vec<String> {
        self.descriptors.keys().cloned().collect()
    }
}

impl HelperInterface for DescriptorHelper {
    fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        "descriptor".to_string()
    }
}
