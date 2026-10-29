//! ref: composer/vendor/symfony/console/Helper/SymfonyQuestionHelper.php

use crate::formatter::OutputFormatter;
use crate::helper::{QuestionHelper, QuestionHelperInterface};
use crate::output::{OutputInterface, output_interface};
use crate::question::QuestionInterface;
use crate::style::{StyleInterface, SymfonyStyle};
use bstr::{B, BString};
use shirabe_php_shim::{
    Exception, PhpMixed, bconcat, boolval, explode, implode, php_os_family, php_to_string, trim,
};
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

    fn get_eof_shortcut(&self) -> String {
        if php_os_family() == "Windows" {
            return "<comment>Ctrl+Z</comment> then <comment>Enter</comment>".to_string();
        }

        "<comment>Ctrl+D</comment>".to_string()
    }
}

impl QuestionHelperInterface for SymfonyQuestionHelper {
    fn inner(&self) -> &QuestionHelper {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut QuestionHelper {
        &mut self.inner
    }

    fn write_prompt(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) {
        let mut text = OutputFormatter::escape_trailing_backslash(question.get_question());
        let default = question.get_default();

        if question.is_multiline() {
            text = bconcat!(text, " (press ", self.get_eof_shortcut(), " to continue)");
        }

        // switch (true)
        if matches!(default, PhpMixed::Null) {
            text = bconcat!(" <info>", text, "</info>:");
        } else if question.as_confirmation().is_some() {
            text = bconcat!(
                " <info>",
                text,
                " (yes/no)</info> [<comment>",
                if boolval(&default) { "yes" } else { "no" },
                "</comment>]:",
            );
        } else if let Some(choice_question) = question.as_choice().filter(|q| q.is_multiselect()) {
            let choices = choice_question.get_choices();
            let default_parts = explode(",", php_to_string(&default));

            let resolved: Vec<BString> = default_parts
                .iter()
                .map(|value| php_to_string(choices.get(B(&trim(value))).unwrap()))
                .collect();

            text = bconcat!(
                " <info>",
                text,
                "</info> [<comment>",
                OutputFormatter::escape(implode(", ", &resolved)).unwrap(),
                "</comment>]:",
            );
        } else if let Some(choice_question) = question.as_choice() {
            let choices = choice_question.get_choices();
            let shown = choices
                .get(B(&php_to_string(&default)))
                .cloned()
                .unwrap_or(default);
            text = bconcat!(
                " <info>",
                text,
                "</info> [<comment>",
                OutputFormatter::escape(php_to_string(&shown)).unwrap(),
                "</comment>]:",
            );
        } else {
            text = bconcat!(
                " <info>",
                text,
                "</info> [<comment>",
                OutputFormatter::escape(php_to_string(&default)).unwrap(),
                "</comment>]:",
            );
        }

        output
            .borrow()
            .writeln(&[text], output_interface::OUTPUT_NORMAL);

        let mut prompt = BString::from(" > ");

        if let Some(choice_question) = question.as_choice() {
            output.borrow().writeln(
                &self
                    .inner
                    .format_choice_question_choices(choice_question, "comment"),
                output_interface::OUTPUT_NORMAL,
            );

            prompt = BString::from(choice_question.get_prompt());
        }

        output
            .borrow()
            .write(&[prompt], false, output_interface::OUTPUT_NORMAL);
    }

    fn write_error(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        error: &Exception,
    ) {
        {
            let mut borrowed = output.borrow_mut();
            if let Some(style) = (*borrowed).as_any_mut().downcast_mut::<SymfonyStyle>() {
                style.new_line(1);
                style.error(&[error.get_message().into()]);

                return;
            }
        }

        self.inner.write_error(output, error);
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
