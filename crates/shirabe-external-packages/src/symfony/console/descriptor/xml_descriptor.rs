//! ref: composer/vendor/symfony/console/Descriptor/XmlDescriptor.php

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor::Descriptor;
use crate::symfony::console::descriptor::descriptor_interface::DescriptorInterface;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

// NOTE: This descriptor relies on the PHP standard classes \DOMDocument and \DOMNode.
// No equivalent has been introduced into the project yet, so every DOM operation is left
// as todo!() pending a decision on how to model the DOM types. The type `DOMDocument` /
// `DOMNode` placeholders below stand in for those PHP classes.

/// Placeholder for the PHP standard class \DOMDocument.
#[derive(Debug)]
pub struct DOMDocument;

/// Placeholder for the PHP standard class \DOMNode.
#[derive(Debug)]
pub struct DOMNode;

/// XML descriptor.
///
/// @internal
#[derive(Debug, Default)]
pub struct XmlDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

impl XmlDescriptor {
    pub fn get_input_definition_document(&self, definition: &InputDefinition) -> DOMDocument {
        let dom: DOMDocument = todo!("new \\DOMDocument('1.0', 'UTF-8')");
        let definition_xml: DOMNode = todo!("$dom->createElement('definition')");
        // $dom->appendChild($definitionXML)

        let arguments_xml: DOMNode = todo!("$dom->createElement('arguments')");
        // $definitionXML->appendChild($argumentsXML)
        for argument in definition.get_arguments().values() {
            self.append_document(&arguments_xml, &self.get_input_argument_document(argument));
        }

        let options_xml: DOMNode = todo!("$dom->createElement('options')");
        // $definitionXML->appendChild($optionsXML)
        for option in definition.get_options().values() {
            self.append_document(&options_xml, &self.get_input_option_document(option));
        }

        dom
    }

    pub fn get_command_document(&self, command: &mut dyn Command, short: bool) -> DOMDocument {
        let dom: DOMDocument = todo!("new \\DOMDocument('1.0', 'UTF-8')");
        let command_xml: DOMNode = todo!("$dom->createElement('command')");
        // $dom->appendChild($commandXML)

        // $commandXML->setAttribute('id', $command->getName())
        let _ = command.get_name();
        // $commandXML->setAttribute('name', $command->getName())
        // $commandXML->setAttribute('hidden', $command->isHidden() ? 1 : 0)
        let _ = command.is_hidden();

        let usages_xml: DOMNode = todo!("$dom->createElement('usages'); appendChild");

        let _description_xml: DOMNode = todo!("$dom->createElement('description'); appendChild");
        // $descriptionXML->appendChild($dom->createTextNode(str_replace("\n", "\n ", $command->getDescription())))
        let _ = shirabe_php_shim::str_replace("\n", "\n ", &command.get_description());

        if short {
            for usage in command.get_aliases() {
                let _ = usage;
                // $usagesXML->appendChild($dom->createElement('usage', $usage))
                let _ = &usages_xml;
                todo!("createElement('usage', $usage) and append");
            }
        } else {
            command.merge_application_definition(false);

            let mut usages = vec![command.get_synopsis(false)];
            usages.extend(command.get_aliases());
            usages.extend(command.get_usages());
            for usage in usages {
                let _ = usage;
                todo!("createElement('usage', $usage) and append");
            }

            let _help_xml: DOMNode = todo!("$dom->createElement('help'); appendChild");
            // $helpXML->appendChild($dom->createTextNode(str_replace("\n", "\n ", $command->getProcessedHelp())))
            let _ = shirabe_php_shim::str_replace("\n", "\n ", &command.get_processed_help());

            let command_definition = command.get_definition().clone();
            let definition_xml = self.get_input_definition_document(&command_definition);
            let _ = definition_xml;
            // $this->appendDocument($commandXML, $definitionXML->getElementsByTagName('definition')->item(0))
            self.append_document(
                &command_xml,
                todo!("$definitionXML->getElementsByTagName('definition')->item(0)"),
            );
        }

        dom
    }

    pub fn get_application_document(
        &self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        namespace: Option<String>,
        short: bool,
    ) -> DOMDocument {
        let dom: DOMDocument = todo!("new \\DOMDocument('1.0', 'UTF-8')");
        let root_xml: DOMNode = todo!("$dom->createElement('symfony'); appendChild");

        if "UNKNOWN" != application.borrow().get_name() {
            // $rootXml->setAttribute('name', $application->getName())
            if "UNKNOWN" != application.borrow().get_version() {
                // $rootXml->setAttribute('version', $application->getVersion())
            }
        }

        let commands_xml: DOMNode =
            todo!("$dom->createElement('commands'); appendChild to rootXml");

        let mut description =
            ApplicationDescription::new(application.clone(), namespace.clone(), true);

        if let Some(ref namespace) = namespace {
            let _ = namespace;
            // $commandsXML->setAttribute('namespace', $namespace)
        }

        let command_list: Vec<_> = description
            .get_commands()
            .values()
            .map(|c| c.borrow().clone_box())
            .collect();
        for mut command in command_list {
            self.append_document(
                &commands_xml,
                &self.get_command_document(command.as_mut(), short),
            );
        }

        if namespace.is_none() {
            let _namespaces_xml: DOMNode =
                todo!("$dom->createElement('namespaces'); appendChild to rootXml");
            let _ = &root_xml;

            for namespace_description in description.get_namespaces().values() {
                let _namespace_array_xml: DOMNode =
                    todo!("$dom->createElement('namespace'); append; setAttribute('id', ...)");
                let _ = match namespace_description.get("id") {
                    Some(PhpMixed::String(s)) => s.clone(),
                    _ => String::new(),
                };

                if let Some(PhpMixed::List(names)) = namespace_description.get("commands") {
                    for name in names {
                        let _ = name.as_string();
                        // $commandXML = $dom->createElement('command'); append;
                        // $commandXML->appendChild($dom->createTextNode($name))
                        todo!("create command element with text node $name");
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
        command: &mut dyn Command,
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
    ///
    /// In PHP `DOMDocument` extends `DOMNode`; the placeholder model accepts a
    /// `DOMDocument` for `imported_parent` to match the call sites.
    fn append_document(&self, parent_node: &DOMNode, imported_parent: &DOMDocument) {
        let _ = (parent_node, imported_parent);
        // foreach ($importedParent->childNodes as $childNode) {
        //     $parentNode->appendChild($parentNode->ownerDocument->importNode($childNode, true));
        // }
        todo!("DOM import/append of child nodes");
    }

    /// Writes DOM document.
    fn write_document(&self, dom: DOMDocument) {
        // $dom->formatOutput = true;
        // $this->write($dom->saveXML());
        let xml: String = todo!("$dom->saveXML() with formatOutput = true");
        let _ = dom;
        self.write(&xml, false);
    }

    fn get_input_argument_document(&self, argument: &InputArgument) -> DOMDocument {
        let dom: DOMDocument = todo!("new \\DOMDocument('1.0', 'UTF-8')");

        // $objectXML = $dom->createElement('argument'); appendChild
        // $objectXML->setAttribute('name', $argument->getName())
        let _ = argument.get_name();
        // $objectXML->setAttribute('is_required', $argument->isRequired() ? 1 : 0)
        let _ = argument.is_required();
        // $objectXML->setAttribute('is_array', $argument->isArray() ? 1 : 0)
        let _ = argument.is_array();
        // $descriptionXML = $dom->createElement('description'); append;
        // $descriptionXML->appendChild($dom->createTextNode($argument->getDescription()))
        let _ = argument.get_description();

        // $defaultsXML = $dom->createElement('defaults'); append
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
            let _ = default;
            // $defaultXML = $dom->createElement('default'); append;
            // $defaultXML->appendChild($dom->createTextNode($default))
            todo!("create default element with text node");
        }

        dom
    }

    fn get_input_option_document(&self, option: &InputOption) -> DOMDocument {
        let dom: DOMDocument = todo!("new \\DOMDocument('1.0', 'UTF-8')");

        // $objectXML = $dom->createElement('option'); appendChild
        // $objectXML->setAttribute('name', '--'.$option->getName())
        let _ = format!("--{}", option.get_name());
        let pos = shirabe_php_shim::strpos(option.get_shortcut().unwrap_or(""), "|");
        if let Some(pos) = pos {
            // $objectXML->setAttribute('shortcut', '-'.substr($option->getShortcut(), 0, $pos))
            let _ = format!(
                "-{}",
                shirabe_php_shim::substr(option.get_shortcut().unwrap(), 0, Some(pos as i64))
            );
            // $objectXML->setAttribute('shortcuts', '-'.str_replace('|', '|-', $option->getShortcut()))
            let _ = format!(
                "-{}",
                shirabe_php_shim::str_replace("|", "|-", option.get_shortcut().unwrap())
            );
        } else {
            // $objectXML->setAttribute('shortcut', $option->getShortcut() ? '-'.$option->getShortcut() : '')
            let _ = match option.get_shortcut() {
                Some(s) => format!("-{}", s),
                None => String::new(),
            };
        }
        // $objectXML->setAttribute('accept_value', $option->acceptValue() ? 1 : 0)
        let _ = option.accept_value();
        // $objectXML->setAttribute('is_value_required', $option->isValueRequired() ? 1 : 0)
        let _ = option.is_value_required();
        // $objectXML->setAttribute('is_multiple', $option->isArray() ? 1 : 0)
        let _ = option.is_array();
        // $descriptionXML = $dom->createElement('description'); append;
        // $descriptionXML->appendChild($dom->createTextNode($option->getDescription()))
        let _ = option.get_description();

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
            // $defaultsXML = $dom->createElement('defaults'); append

            if !defaults.is_empty() {
                for default in defaults {
                    let _ = default;
                    // $defaultXML = $dom->createElement('default'); append;
                    // $defaultXML->appendChild($dom->createTextNode($default))
                    todo!("create default element with text node");
                }
            }
        }

        if option.is_negatable() {
            // $objectXML = $dom->createElement('option'); $dom->appendChild
            // $objectXML->setAttribute('name', '--no-'.$option->getName())
            let _ = format!("--no-{}", option.get_name());
            // setAttribute('shortcut', ''); setAttribute('accept_value', 0);
            // setAttribute('is_value_required', 0); setAttribute('is_multiple', 0);
            // $descriptionXML = $dom->createElement('description'); append;
            // $descriptionXML->appendChild($dom->createTextNode('Negate the "--'.$option->getName().'" option'))
            let _ = format!("Negate the \"--{}\" option", option.get_name());
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
        object: PhpMixed,
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
        command: &mut dyn Command,
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
