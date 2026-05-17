//! ref: composer/src/Composer/Command/ProhibitsCommand.php

use shirabe_external_packages::symfony::component::console::command::command::Command;

use crate::command::base_command::BaseCommand;
use crate::command::base_dependency_command::BaseDependencyCommand;
use crate::composer::Composer;
use crate::io::io_interface::IOInterface;
use crate::command::completion_trait::CompletionTrait;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;

pub struct ProhibitsCommand {
    colors: Vec<String>,
}

impl CompletionTrait for ProhibitsCommand {}

impl ProhibitsCommand {
    pub fn configure(&mut self) {
        let package_suggestions = self.suggest_available_package();
        self.inner
            .set_name("prohibits")
            .set_aliases(vec!["why-not".to_string()])
            .set_description("Shows which packages prevent the given package from being installed")
            .set_definition(vec![
                InputArgument::new(
                    BaseDependencyCommand::ARGUMENT_PACKAGE,
                    InputArgument::REQUIRED,
                    "Package to inspect",
                    None,
                    package_suggestions,
                ),
                InputArgument::new(
                    BaseDependencyCommand::ARGUMENT_CONSTRAINT,
                    InputArgument::REQUIRED,
                    "Version constraint, which version you expected to be installed",
                    None,
                    None,
                ),
                InputOption::new(
                    BaseDependencyCommand::OPTION_RECURSIVE,
                    Some("r"),
                    InputOption::VALUE_NONE,
                    "Recursively resolves up to the root package",
                ),
                InputOption::new(
                    BaseDependencyCommand::OPTION_TREE,
                    Some("t"),
                    InputOption::VALUE_NONE,
                    "Prints the results as a nested tree",
                ),
                InputOption::new(
                    "locked",
                    None,
                    InputOption::VALUE_NONE,
                    "Read dependency information from composer.lock",
                ),
            ])
            .set_help(
                "Displays detailed information about why a package cannot be installed.\n\n\
                <info>php composer.phar prohibits composer/composer</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#prohibits-why-not",
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> i64 {
        self.inner.do_execute(input, output, true)
    }
}

impl BaseCommand for ProhibitsCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}

impl BaseDependencyCommand for ProhibitsCommand {
    fn colors(&self) -> &[String] {
        &self.colors
    }

    fn colors_mut(&mut self) -> &mut [String] {
        &mut self.colors
    }
}
