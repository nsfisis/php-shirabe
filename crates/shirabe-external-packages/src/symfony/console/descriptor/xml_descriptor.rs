//! ref: composer/vendor/symfony/console/Descriptor/XmlDescriptor.php

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor::Descriptor;
use crate::symfony::console::descriptor::descriptor_interface::{
    DescribableObject, DescriptorInterface,
};
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::{DOMDocument, DOMNode, PhpMixed};

/// XML descriptor.
///
/// @internal
#[derive(Debug, Default)]
pub struct XmlDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

impl XmlDescriptor {
    pub fn get_input_definition_document(&self, definition: &InputDefinition) -> DOMDocument {
        let dom = DOMDocument::new("1.0", "UTF-8");
        let definition_xml = dom.append_child(dom.create_element("definition"));

        let arguments_xml = definition_xml.append_child(dom.create_element("arguments"));
        for argument in definition.get_arguments().values() {
            let argument_xml = self.get_input_argument_document(argument);
            self.append_document(&arguments_xml, &argument_xml.as_node());
        }

        let options_xml = definition_xml.append_child(dom.create_element("options"));
        for option in definition.get_options().values() {
            let option_xml = self.get_input_option_document(option);
            self.append_document(&options_xml, &option_xml.as_node());
        }

        dom
    }

    pub fn get_command_document(&self, command: &dyn Command, short: bool) -> DOMDocument {
        let dom = DOMDocument::new("1.0", "UTF-8");
        let command_xml = dom.append_child(dom.create_element("command"));

        let name = command.get_name().unwrap_or_default();
        command_xml.set_attribute("id", &name);
        command_xml.set_attribute("name", &name);
        command_xml.set_attribute("hidden", if command.is_hidden() { "1" } else { "0" });

        let usages_xml = command_xml.append_child(dom.create_element("usages"));

        let description_xml = command_xml.append_child(dom.create_element("description"));
        description_xml.append_child(dom.create_text_node(&shirabe_php_shim::str_replace(
            "\n",
            "\n ",
            &command.get_description(),
        )));

        if short {
            for usage in command.get_aliases() {
                usages_xml.append_child(dom.create_element_with_value("usage", &usage));
            }
        } else {
            command.merge_application_definition(false);

            let mut usages = vec![command.get_synopsis(false)];
            usages.extend(command.get_aliases());
            usages.extend(command.get_usages());
            for usage in usages {
                usages_xml.append_child(dom.create_element_with_value("usage", &usage));
            }

            let help_xml = command_xml.append_child(dom.create_element("help"));
            help_xml.append_child(dom.create_text_node(&shirabe_php_shim::str_replace(
                "\n",
                "\n ",
                &command.get_processed_help(),
            )));

            let command_definition = command.get_definition().clone();
            let definition_xml = self.get_input_definition_document(&command_definition);
            let definition_node = definition_xml
                .get_elements_by_tag_name("definition")
                .item(0)
                .expect("input definition document always contains a <definition> element");
            self.append_document(&command_xml, &definition_node);
        }

        dom
    }

    pub fn get_application_document(
        &self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        namespace: Option<String>,
        short: bool,
    ) -> DOMDocument {
        let dom = DOMDocument::new("1.0", "UTF-8");
        let root_xml = dom.append_child(dom.create_element("symfony"));

        let app_name = application.borrow().get_name();
        if app_name != "UNKNOWN" {
            root_xml.set_attribute("name", &app_name);
            let app_version = application.borrow().get_version();
            if app_version != "UNKNOWN" {
                root_xml.set_attribute("version", &app_version);
            }
        }

        let commands_xml = root_xml.append_child(dom.create_element("commands"));

        let mut description =
            ApplicationDescription::new(application.clone(), namespace.clone(), true);

        if let Some(ref namespace) = namespace {
            commands_xml.set_attribute("namespace", namespace);
        }

        for command in description.get_commands().values() {
            let command = command.borrow();
            let command_xml = self.get_command_document(&*command, short);
            self.append_document(&commands_xml, &command_xml.as_node());
        }

        if namespace.is_none() {
            let namespaces_xml = root_xml.append_child(dom.create_element("namespaces"));

            let namespaces = description.get_namespaces();
            for namespace_description in namespaces.values() {
                let namespace_array_xml =
                    namespaces_xml.append_child(dom.create_element("namespace"));
                let id = match namespace_description.get("id") {
                    Some(PhpMixed::String(s)) => s.as_str(),
                    _ => "",
                };
                namespace_array_xml.set_attribute("id", id);

                if let Some(PhpMixed::List(names)) = namespace_description.get("commands") {
                    for name in names {
                        let command_xml =
                            namespace_array_xml.append_child(dom.create_element("command"));
                        command_xml
                            .append_child(dom.create_text_node(name.as_string().unwrap_or("")));
                    }
                }
            }
        }

        dom
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_document(self.get_input_argument_document(argument));
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_document(self.get_input_option_document(option));
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_document(self.get_input_definition_document(definition));
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let short = matches!(options.get("short"), Some(PhpMixed::Bool(true)));
        self.write_document(self.get_command_document(command, short));
        Ok(())
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let namespace = match options.get("namespace") {
            Some(PhpMixed::String(s)) => Some(s.clone()),
            _ => None,
        };
        let short = matches!(options.get("short"), Some(PhpMixed::Bool(true)));
        self.write_document(self.get_application_document(application, namespace, short));
        Ok(())
    }

    /// Appends document children to parent node.
    fn append_document(&self, parent_node: &DOMNode, imported_parent: &DOMNode) {
        for child_node in imported_parent.child_nodes() {
            parent_node.append_child(parent_node.owner_document().import_node(&child_node, true));
        }
    }

    /// Writes DOM document.
    fn write_document(&self, dom: DOMDocument) {
        dom.set_format_output(true);
        let mut buf = Vec::new();
        dom.save_xml(&mut buf)
            .expect("serializing XML to an in-memory buffer cannot fail");
        let xml = String::from_utf8(buf).expect("DOM serialization yields valid UTF-8");
        self.write(&xml, false);
    }

    fn get_input_argument_document(&self, argument: &InputArgument) -> DOMDocument {
        let dom = DOMDocument::new("1.0", "UTF-8");

        let object_xml = dom.append_child(dom.create_element("argument"));
        object_xml.set_attribute("name", argument.get_name());
        object_xml.set_attribute(
            "is_required",
            if argument.is_required() { "1" } else { "0" },
        );
        object_xml.set_attribute("is_array", if argument.is_array() { "1" } else { "0" });
        let description_xml = object_xml.append_child(dom.create_element("description"));
        description_xml.append_child(dom.create_text_node(argument.get_description()));

        let defaults_xml = object_xml.append_child(dom.create_element("defaults"));
        let defaults: Vec<String> = match argument.get_default() {
            PhpMixed::List(_) | PhpMixed::Array(_) => {
                self.default_values_as_strings(argument.get_default())
            }
            PhpMixed::Bool(_) => vec![shirabe_php_shim::var_export(argument.get_default(), true)],
            d if shirabe_php_shim::php_truthy(d) => {
                vec![shirabe_php_shim::php_to_string(argument.get_default())]
            }
            _ => vec![],
        };
        for default in defaults {
            let default_xml = defaults_xml.append_child(dom.create_element("default"));
            default_xml.append_child(dom.create_text_node(&default));
        }

        dom
    }

    fn get_input_option_document(&self, option: &InputOption) -> DOMDocument {
        let dom = DOMDocument::new("1.0", "UTF-8");

        let object_xml = dom.append_child(dom.create_element("option"));
        object_xml.set_attribute("name", &format!("--{}", option.get_name()));
        let pos = shirabe_php_shim::strpos(option.get_shortcut().unwrap_or(""), "|");
        if let Some(pos) = pos {
            object_xml.set_attribute(
                "shortcut",
                &format!(
                    "-{}",
                    shirabe_php_shim::substr(option.get_shortcut().unwrap(), 0, Some(pos as i64))
                ),
            );
            object_xml.set_attribute(
                "shortcuts",
                &format!(
                    "-{}",
                    shirabe_php_shim::str_replace("|", "|-", option.get_shortcut().unwrap())
                ),
            );
        } else {
            object_xml.set_attribute(
                "shortcut",
                &match option.get_shortcut() {
                    Some(s) => format!("-{}", s),
                    None => String::new(),
                },
            );
        }
        object_xml.set_attribute(
            "accept_value",
            if option.accept_value() { "1" } else { "0" },
        );
        object_xml.set_attribute(
            "is_value_required",
            if option.is_value_required() { "1" } else { "0" },
        );
        object_xml.set_attribute("is_multiple", if option.is_array() { "1" } else { "0" });
        let description_xml = object_xml.append_child(dom.create_element("description"));
        description_xml.append_child(dom.create_text_node(option.get_description()));

        if option.accept_value() {
            let defaults: Vec<String> = match option.get_default() {
                PhpMixed::List(_) | PhpMixed::Array(_) => {
                    self.default_values_as_strings(option.get_default())
                }
                PhpMixed::Bool(_) => vec![shirabe_php_shim::var_export(option.get_default(), true)],
                d if shirabe_php_shim::php_truthy(d) => {
                    vec![shirabe_php_shim::php_to_string(option.get_default())]
                }
                _ => vec![],
            };
            let defaults_xml = object_xml.append_child(dom.create_element("defaults"));

            if !defaults.is_empty() {
                for default in defaults {
                    let default_xml = defaults_xml.append_child(dom.create_element("default"));
                    default_xml.append_child(dom.create_text_node(&default));
                }
            }
        }

        if option.is_negatable() {
            let object_xml = dom.append_child(dom.create_element("option"));
            object_xml.set_attribute("name", &format!("--no-{}", option.get_name()));
            object_xml.set_attribute("shortcut", "");
            object_xml.set_attribute("accept_value", "0");
            object_xml.set_attribute("is_value_required", "0");
            object_xml.set_attribute("is_multiple", "0");
            let description_xml = object_xml.append_child(dom.create_element("description"));
            description_xml.append_child(
                dom.create_text_node(&format!("Negate the \"--{}\" option", option.get_name())),
            );
        }

        dom
    }

    /// Helper used by the default-value branches of getInputArgumentDocument /
    /// getInputOptionDocument when the default is an array (returns it verbatim).
    fn default_values_as_strings(&self, default: &PhpMixed) -> Vec<String> {
        match default {
            PhpMixed::List(list) => list
                .iter()
                .map(|v| shirabe_php_shim::php_to_string(v))
                .collect(),
            PhpMixed::Array(arr) => arr
                .values()
                .map(|v| shirabe_php_shim::php_to_string(v))
                .collect(),
            _ => vec![],
        }
    }
}

impl DescriptorInterface for XmlDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        Descriptor::describe(self, output, object, options)
    }
}

impl Descriptor for XmlDescriptor {
    fn output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        self.output.clone().unwrap()
    }

    fn set_output(&mut self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        self.output = Some(output);
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        XmlDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        XmlDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        XmlDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        XmlDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        XmlDescriptor::describe_application(self, application, options)
    }
}
