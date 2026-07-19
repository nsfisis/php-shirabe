//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterStyleStack.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::formatter::output_formatter_style::OutputFormatterStyle;
use crate::symfony::console::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;

#[derive(Debug)]
pub struct OutputFormatterStyleStack {
    styles: Vec<Box<dyn OutputFormatterStyleInterface>>,
    empty_style: Box<dyn OutputFormatterStyleInterface>,
}

impl OutputFormatterStyleStack {
    pub fn new(empty_style: Option<Box<dyn OutputFormatterStyleInterface>>) -> Self {
        let empty_style =
            empty_style.unwrap_or_else(|| Box::new(OutputFormatterStyle::new(None, None, vec![])));
        let mut this = Self {
            styles: vec![],
            empty_style,
        };
        this.reset();
        this
    }

    /// Pushes a style in the stack.
    pub fn push(&mut self, style: Box<dyn OutputFormatterStyleInterface>) {
        self.styles.push(style);
    }

    /// Pops a style from the stack.
    ///
    /// Throws InvalidArgumentException when style tags incorrectly nested.
    pub fn pop(
        &mut self,
        mut style: Option<Box<dyn OutputFormatterStyleInterface>>,
    ) -> anyhow::Result<Result<Box<dyn OutputFormatterStyleInterface>, InvalidArgumentException>>
    {
        if self.styles.is_empty() {
            return Ok(Ok(self.empty_style.clone_box()));
        }

        let style = match style.as_mut() {
            None => {
                return Ok(Ok(shirabe_php_shim::array_pop(&mut self.styles).unwrap()));
            }
            Some(style) => style,
        };

        for index in (0..self.styles.len()).rev() {
            if style.apply("") == self.styles[index].apply("") {
                // PHP: array_slice($this->styles, 0, $index) keeps elements before $index,
                // dropping the matched element and everything after it.
                let stacked_style = self.styles.remove(index);
                self.styles.truncate(index);

                return Ok(Ok(stacked_style));
            }
        }

        Ok(Err(InvalidArgumentException(
            shirabe_php_shim::InvalidArgumentException {
                message: "Incorrectly nested style tag found.".to_string(),
                code: 0,
            },
        )))
    }

    /// Computes current style with stacks top codes.
    pub fn get_current(&self) -> &dyn OutputFormatterStyleInterface {
        if self.styles.is_empty() {
            return self.empty_style.as_ref();
        }

        self.styles[self.styles.len() - 1].as_ref()
    }

    /// Mutable variant of `get_current`, needed because `apply` lazily mutates style state.
    pub fn get_current_mut(&mut self) -> &mut dyn OutputFormatterStyleInterface {
        if self.styles.is_empty() {
            return self.empty_style.as_mut();
        }

        let last = self.styles.len() - 1;
        self.styles[last].as_mut()
    }

    pub fn set_empty_style(
        &mut self,
        empty_style: Box<dyn OutputFormatterStyleInterface>,
    ) -> &mut Self {
        self.empty_style = empty_style;

        self
    }

    pub fn get_empty_style(&self) -> &dyn OutputFormatterStyleInterface {
        self.empty_style.as_ref()
    }

    pub fn reset(&mut self) {
        self.styles = vec![];
    }
}
