//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterStyleInterface.php

use bstr::{BStr, BString};

/// Formatter style interface for defining styles.
pub trait OutputFormatterStyleInterface: std::fmt::Debug {
    /// Sets style foreground color.
    fn set_foreground(&mut self, color: Option<&BStr>);

    /// Sets style background color.
    fn set_background(&mut self, color: Option<&BStr>);

    /// Sets some specific style option.
    fn set_option(&mut self, option: &BStr);

    /// Unsets some specific style option.
    fn unset_option(&mut self, option: &BStr);

    /// Sets multiple style options at once.
    fn set_options(&mut self, options: Vec<BString>);

    /// Applies the style to a given text.
    fn apply(&mut self, text: &BStr) -> BString;

    /// Clones the style into a new boxed trait object. PHP shares the style
    /// instance by reference; styles are immutable once configured, so cloning
    /// is behaviorally equivalent here.
    fn clone_box(&self) -> Box<dyn OutputFormatterStyleInterface>;
}
