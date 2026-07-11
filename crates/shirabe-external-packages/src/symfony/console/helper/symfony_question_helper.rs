//! ref: composer/vendor/symfony/console/Helper/SymfonyQuestionHelper.php

use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::helper::question_helper::QuestionHelper;
use crate::symfony::console::output::output_interface;
use crate::symfony::console::output::output_interface::OutputInterface;
use crate::symfony::console::question::QuestionInterface;
use crate::symfony::console::style::symfony_style::SymfonyStyle;
use shirabe_php_shim::PhpMixed;
use std::ops::{Deref, DerefMut};

/// Symfony Style Guide compliant question helper.
#[derive(Debug, Default)]
pub struct SymfonyQuestionHelper {
    inner: QuestionHelper,
}

impl SymfonyQuestionHelper {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn write_prompt(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) {
        let mut text = OutputFormatter::escape_trailing_backslash(question.get_question());
        let default = question.get_default();

        if question.is_multiline() {
            text += &format!(" (press {} to continue)", self.get_eof_shortcut());
        }

        // switch (true)
        if matches!(default, PhpMixed::Null) {
            text = format!(" <info>{}</info>:", text);
        } else if question.as_confirmation().is_some() {
            text = format!(
                " <info>{} (yes/no)</info> [<comment>{}</comment>]:",
                text,
                if shirabe_php_shim::boolval(&default) {
                    "yes"
                } else {
                    "no"
                },
            );
        } else if let Some(choice_question) = question.as_choice().filter(|q| q.is_multiselect()) {
            let choices = choice_question.get_choices();
            let default_parts = shirabe_php_shim::explode(",", &default.to_string());

            let resolved: Vec<String> = default_parts
                .iter()
                .map(|value| {
                    choices
                        .get(&shirabe_php_shim::trim(value, None))
                        .map(|v| v.to_string())
                        .unwrap()
                })
                .collect();

            text = format!(
                " <info>{}</info> [<comment>{}</comment>]:",
                text,
                OutputFormatter::escape(&resolved.join(", ")).unwrap(),
            );
        } else if let Some(choice_question) = question.as_choice() {
            let choices = choice_question.get_choices();
            text = format!(
                " <info>{}</info> [<comment>{}</comment>]:",
                text,
                OutputFormatter::escape(
                    &choices
                        .get(&default.to_string())
                        .cloned()
                        .unwrap_or(default.clone())
                        .to_string(),
                )
                .unwrap(),
            );
        } else {
            text = format!(
                " <info>{}</info> [<comment>{}</comment>]:",
                text,
                OutputFormatter::escape(&default.to_string()).unwrap(),
            );
        }

        output
            .borrow()
            .writeln(&[text], output_interface::OUTPUT_NORMAL);

        let mut prompt = " > ".to_string();

        if let Some(choice_question) = question.as_choice() {
            output.borrow().writeln(
                &self
                    .inner
                    .format_choice_question_choices(choice_question, "comment"),
                output_interface::OUTPUT_NORMAL,
            );

            prompt = choice_question.get_prompt().to_string();
        }

        output
            .borrow()
            .write(&[prompt], false, output_interface::OUTPUT_NORMAL);
    }

    pub(crate) fn write_error(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        error: &shirabe_php_shim::Exception,
    ) {
        let is_symfony_style = {
            let borrowed = output.borrow();
            (*borrowed)
                .as_any()
                .downcast_ref::<SymfonyStyle>()
                .is_some()
        };
        if is_symfony_style {
            // $output->newLine(); $output->error($error->getMessage());
            // SymfonyStyle's newLine()/error() require mutable access to the
            // concrete type; mutable downcasting through the trait object is
            // resolved in a later phase.
            todo!("SymfonyStyle newLine()/error() require &mut SymfonyStyle");
        }

        self.inner.write_error(output, error);
    }

    fn get_eof_shortcut(&self) -> String {
        if shirabe_php_shim::php_os_family() == "Windows" {
            return "<comment>Ctrl+Z</comment> then <comment>Enter</comment>".to_string();
        }

        "<comment>Ctrl+D</comment>".to_string()
    }
}

impl Deref for SymfonyQuestionHelper {
    type Target = QuestionHelper;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for SymfonyQuestionHelper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
