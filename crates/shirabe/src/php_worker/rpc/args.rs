//! Argument decoding for the entity methods.

use bstr::BStr;
use shirabe_php_rpc::codec::DecodeError;
use shirabe_php_rpc::{FromRpcValue, PhpThrow, RpcValue, codec};
use shirabe_php_shim::bformat;

/// Decodes the argument at `position`. A missing argument decodes the same way PHP's `null`
/// does; a parameter's own default belongs to `arg_or`.
pub(crate) fn arg<T: FromRpcValue>(
    method: &BStr,
    args: &[RpcValue],
    position: usize,
) -> Result<T, PhpThrow> {
    T::from_rpc_value(args.get(position).unwrap_or(&codec::NULL))
        .map_err(|error| arg_throw(method, position, error))
}

/// Decodes the argument at `position` for a parameter whose PHP declaration carries a default:
/// an omitted argument, and the `null` PHP passes in its place, take `default`.
pub(crate) fn arg_or<T: FromRpcValue>(
    method: &BStr,
    args: &[RpcValue],
    position: usize,
    default: T,
) -> Result<T, PhpThrow> {
    match args.get(position) {
        None | Some(RpcValue::Null) => Ok(default),
        Some(value) => T::from_rpc_value(value).map_err(|error| arg_throw(method, position, error)),
    }
}

/// The `TypeError` of an argument that did not decode, naming the argument.
pub(crate) fn arg_throw(method: &BStr, position: usize, error: DecodeError) -> PhpThrow {
    DecodeError::new(bformat!("{method} argument {position}: {}", error.message)).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;

    #[test]
    fn a_data_argument_keeps_its_bytes() {
        let bytes = BString::from(b"vendor/\xffname".as_slice());
        let args = [RpcValue::String(bytes.clone())];

        assert_eq!(
            bytes,
            arg::<BString>(BStr::new("method"), &args, 0).unwrap()
        );
    }
}
