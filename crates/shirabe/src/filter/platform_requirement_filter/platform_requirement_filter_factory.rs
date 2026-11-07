//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterFactory.php sha1:76632cb894b3e1ffccddaa8e5774199fcadc8fbc

use crate::filter::platform_requirement_filter::{
    IgnoreAllPlatformRequirementFilter, IgnoreListPlatformRequirementFilter,
    IgnoreNothingPlatformRequirementFilter, PlatformRequirementFilterInterface,
};
use bstr::BString;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, bconcat, get_debug_type};

pub struct PlatformRequirementFilterFactory;

impl PlatformRequirementFilterFactory {
    pub fn from_bool_or_list(
        bool_or_list: PhpMixed,
    ) -> anyhow::Result<std::rc::Rc<dyn PlatformRequirementFilterInterface>> {
        match bool_or_list {
            PhpMixed::Bool(b) => {
                if b {
                    Ok(Self::ignore_all())
                } else {
                    Ok(Self::ignore_nothing())
                }
            }
            list_or_array @ (PhpMixed::List(_) | PhpMixed::Array(_)) => {
                let list: Vec<BString> = match list_or_array {
                    PhpMixed::List(items) => items
                        .into_iter()
                        .filter_map(|v| v.as_string().map(BString::from))
                        .collect(),
                    PhpMixed::Array(map) => map
                        .into_iter()
                        .filter_map(|(_, v)| v.as_string().map(BString::from))
                        .collect(),
                    _ => unreachable!(),
                };
                Ok(std::rc::Rc::new(IgnoreListPlatformRequirementFilter::new(
                    list,
                )?))
            }
            other => Err(InvalidArgumentException::new(bconcat!(
                "PlatformRequirementFilter: Unknown $boolOrList parameter ",
                get_debug_type(&other),
                ". Please report at https://github.com/nsfisis/php-shirabe/issues/new."
            ))
            .into()),
        }
    }

    pub fn ignore_all() -> std::rc::Rc<dyn PlatformRequirementFilterInterface> {
        std::rc::Rc::new(IgnoreAllPlatformRequirementFilter)
    }

    pub fn ignore_nothing() -> std::rc::Rc<dyn PlatformRequirementFilterInterface> {
        std::rc::Rc::new(IgnoreNothingPlatformRequirementFilter)
    }
}
