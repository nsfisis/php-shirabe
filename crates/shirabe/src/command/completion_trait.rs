//! ref: composer/src/Composer/Command/CompletionTrait.php

// TODO(cli-completion): CompletionTrait powered shell completion for command arguments and
// options. The PHP version exposes Closures that resolve to package names, types, etc. We do not
// port that surface yet — see TODO(cli-completion) markers in each command for the original
// suggestions.

pub trait CompletionTrait {}
