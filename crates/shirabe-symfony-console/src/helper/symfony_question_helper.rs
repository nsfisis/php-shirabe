//! ref: composer/vendor/symfony/console/Helper/SymfonyQuestionHelper.php sha1:cdcfae1ff1d0881eced4dd69e3b2afab89611514

use crate::formatter::OutputFormatter;
use crate::helper::{QuestionHelper, QuestionHelperInterface};
use crate::output::{OutputInterface, output_interface};
use crate::question::{QuestionInterface, array_key};
use crate::style::{StyleInterface, SymfonyStyle};
use bstr::{B, BString};
use shirabe_php_shim::{Exception, Stringable, bconcat, explode, implode, php_os_family, trim};
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

    fn get_eof_shortcut(&self) -> BString {
        if php_os_family() == "Windows" {
            return "<comment>Ctrl+Z</comment> then <comment>Enter</comment>".into();
        }

        "<comment>Ctrl+D</comment>".into()
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
        if default.is_null() {
            text = bconcat!(" <info>", text, "</info>:");
        } else if question.as_confirmation().is_some() {
            text = bconcat!(
                " <info>",
                text,
                " (yes/no)</info> [<comment>",
                if default.to_bool() { "yes" } else { "no" },
                "</comment>]:",
            );
        } else if let Some(choice_question) = question.as_choice().filter(|q| q.is_multiselect()) {
            let choices = choice_question.get_choices();
            let default_parts = explode(",", default.to_php_string());

            let resolved: Vec<BString> = default_parts
                .iter()
                .map(|value| choices.get(B(&trim(value))).unwrap().clone())
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
                .get(B(&array_key(&default)))
                .cloned()
                .unwrap_or_else(|| default.to_php_string());
            text = bconcat!(
                " <info>",
                text,
                "</info> [<comment>",
                OutputFormatter::escape(shown).unwrap(),
                "</comment>]:",
            );
        } else {
            text = bconcat!(
                " <info>",
                text,
                "</info> [<comment>",
                OutputFormatter::escape(default.to_php_string()).unwrap(),
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
                    .format_choice_question_choices(choice_question, "comment".into()),
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
            let borrowed = output.borrow();
            if let Some(style) = (*borrowed).as_any().downcast_ref::<SymfonyStyle>() {
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
