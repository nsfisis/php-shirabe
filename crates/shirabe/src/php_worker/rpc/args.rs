//! Argument decoding for the entity methods.

use bstr::BStr;
use shirabe_php_rpc::{FromPluginValue, PhpThrow, PluginValue, codec};
use shirabe_php_shim::bformat;

/// Decodes the argument at `position`. A missing argument decodes the same way PHP's `null`
/// does; a parameter's own default belongs to `arg_or`.
pub(crate) fn arg<T: FromPluginValue>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
) -> Result<T, PhpThrow> {
    T::from_plugin_value(args.get(position).unwrap_or(&codec::NULL))
        .map_err(|throw| arg_throw(method, position, throw))
}

/// Decodes the argument at `position` for a parameter whose PHP declaration carries a default:
/// an omitted argument, and the `null` PHP passes in its place, take `default`.
pub(crate) fn arg_or<T: FromPluginValue>(
    method: &BStr,
    args: &[PluginValue],
    position: usize,
    default: T,
) -> Result<T, PhpThrow> {
    match args.get(position) {
        None | Some(PluginValue::Null) => Ok(default),
        Some(value) => {
            T::from_plugin_value(value).map_err(|throw| arg_throw(method, position, throw))
        }
    }
}

/// Names the argument a decoding failure is about.
pub(crate) fn arg_throw(method: &BStr, position: usize, throw: PhpThrow) -> PhpThrow {
    PhpThrow {
        message: bformat!("{method} argument {position}: {}", throw.message),
        ..throw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;

    #[test]
    fn a_data_argument_keeps_its_bytes() {
        let bytes = BString::from(b"vendor/\xffname".as_slice());
        let args = [PluginValue::String(bytes.clone())];

        assert_eq!(
            bytes,
            arg::<BString>(BStr::new("method"), &args, 0).unwrap()
        );
    }
}
