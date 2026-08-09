//! ref: composer/tests/Composer/Test/Util/IniHelperTest.php
//!
//! All four cases of this test feed IniHelper made-up ini paths through COMPOSER_ORIGINAL_INIS.
//! That variable exists so a process restarted without Xdebug can name the ini files it replaced;
//! nothing here restarts PHP, so IniHelper::get_all() reports the ini files the PHP worker loaded
//! and never reads it (see docs/dev/xdebug.md). With no way to feed the helper, none of the cases
//! are ported.
