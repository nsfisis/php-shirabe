//! Exceptions crossing the boundary between the Rust process and the PHP worker, in both
//! directions: a `Throw` from the worker is rebuilt as the ported exception it describes, and a
//! ported exception is described as the `Throw` the worker rebuilds.

use crate::dependency_resolver::SolverBugException;
use crate::downloader::{FilesystemException, MaxFileSizeExceededException, TransportException};
use crate::event_dispatcher::ScriptExecutionException;
use crate::exception::{IrrecoverableDownloadException, NoSslException};
use crate::json::JsonValidationException;
use crate::package::loader::InvalidPackageException;
use crate::plugin::PluginBlockedException;
use crate::repository::{InvalidRepositoryException, RepositorySecurityException};
use bstr::ByteSlice;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpThrow, PluginValue};
use shirabe_php_shim::{
    AnyThrowable, BadFunctionCallException, BadMethodCallException, Catch as _, Error, Exception,
    InvalidArgumentException, LogicException, OutOfBoundsException, PhpClass as _, PhpMixed,
    RuntimeException, TypeError, UnexpectedValueException, UnportedThrowable, legacy_lossy,
};

/// The error a `Throw` from the PHP worker stands for: the ported exception of its class, or,
/// for a class the port has no type for, the nearest superclass that has one, answering to the
/// class it was thrown as. It keeps the code, the location and the state the class declares.
///
/// TODO(plugin): the `previous` of the exception does not cross.
pub(crate) fn throw_into_error(throw: PhpThrow) -> anyhow::Error {
    let revived = std::iter::once(&throw.exception_class)
        .chain(throw.parents.iter())
        .enumerate()
        .find_map(|(depth, class)| revive(class, &throw).map(|exception| (depth, exception)));
    let mut exception = match revived {
        Some((0, exception)) => exception,
        Some((_, parent)) => UnportedThrowable::new(throw.exception_class.clone(), parent).into(),
        // Every throwable extends \Exception or \Error, which revive() knows, so only a Throw
        // that names no superclasses at all gets here.
        None => UnportedThrowable::new(
            throw.exception_class.clone(),
            Exception::new(throw.message.clone()).into(),
        )
        .into(),
    };
    exception.set_code(throw.code);
    if !throw.file.is_empty() {
        exception.set_location(legacy_lossy(&throw.file).into_owned(), throw.line);
    }
    anyhow::Error::new(exception)
}

/// The `Throw` the fields of `MaterializedThrowable::describe()` stand for, when they come back
/// as a plain value rather than as a frame.
pub(crate) fn described_throw(fields: &[PhpMixed]) -> PhpThrow {
    let string = |position: usize| {
        legacy_lossy(
            fields
                .get(position)
                .and_then(PhpMixed::as_string)
                .unwrap_or_default(),
        )
        .into_owned()
    };
    let int = |position: usize| fields.get(position).and_then(PhpMixed::as_int).unwrap_or(0);
    let mut throw = PhpThrow::new(string(0), string(1), int(2));
    if let Some(PhpMixed::Array(properties)) = fields.get(3) {
        *throw.properties = properties
            .iter()
            .map(|(name, value)| (name.clone(), PluginValue::from_php_mixed(value)))
            .collect();
    }
    throw.file = fields
        .get(4)
        .and_then(PhpMixed::as_string)
        .map(bstr::BString::from)
        .unwrap_or_default();
    throw.line = int(5);
    if let Some(PhpMixed::List(parents)) = fields.get(6) {
        throw.parents = parents
            .iter()
            .filter_map(|parent| parent.as_string().map(|s| s.to_str_lossy().into_owned()))
            .collect();
    }
    throw
}

/// The ported exception of exactly `class`, built from what `throw` carries, or `None` if the
/// port has no type for it or cannot build one from a `Throw`.
fn revive(class: &str, throw: &PhpThrow) -> Option<AnyThrowable> {
    let message = throw.message.clone();
    let code = throw.code;
    let property = |name: &str| {
        throw
            .properties
            .get(bstr::B(name))
            .map(|value| {
                value
                    .to_php_mixed()
                    .expect("an exception property is a plain PHP value")
            })
            .unwrap_or(PhpMixed::Null)
    };
    let strings = |name: &str| match property(name) {
        PhpMixed::List(items) => items
            .iter()
            .map(|item| legacy_lossy(item.as_string().unwrap_or_default()).into_owned())
            .collect(),
        PhpMixed::Array(items) => items
            .values()
            .map(|item| legacy_lossy(item.as_string().unwrap_or_default()).into_owned())
            .collect(),
        _ => Vec::new(),
    };
    Some(match class {
        "Exception" => Exception::new(message).into(),
        "Error" => Error::new(message).into(),
        "TypeError" => TypeError::new(message).into(),
        "RuntimeException" => RuntimeException::new(message).into(),
        "UnexpectedValueException" => UnexpectedValueException::new(message).into(),
        "OutOfBoundsException" => OutOfBoundsException::new(message).into(),
        "LogicException" => LogicException::new(message).into(),
        "InvalidArgumentException" => InvalidArgumentException::new(message).into(),
        "BadFunctionCallException" => BadFunctionCallException::new(message).into(),
        "BadMethodCallException" => BadMethodCallException::new(message).into(),
        r"Composer\Downloader\TransportException" => {
            let mut exception = TransportException::new(message, code);
            if let PhpMixed::List(_) | PhpMixed::Array(_) = property("headers") {
                exception.set_headers(strings("headers"));
            }
            exception.set_response(
                property("response")
                    .as_string()
                    .map(|s| s.to_str_lossy().into_owned()),
            );
            exception.set_status_code(property("statusCode").as_int());
            exception.set_response_info(match property("responseInfo") {
                PhpMixed::List(items) => items,
                PhpMixed::Array(items) => items.into_values().collect(),
                _ => Vec::new(),
            });
            exception.into()
        }
        r"Composer\Downloader\MaxFileSizeExceededException" => {
            MaxFileSizeExceededException::new(message).into()
        }
        r"Composer\Downloader\FilesystemException" => {
            FilesystemException::new(message, code).into()
        }
        r"Composer\Exception\IrrecoverableDownloadException" => {
            IrrecoverableDownloadException::new(message).into()
        }
        r"Composer\Exception\NoSslException" => NoSslException::new(message).into(),
        r"Composer\DependencyResolver\SolverBugException" => {
            SolverBugException::new(message).into()
        }
        r"Composer\EventDispatcher\ScriptExecutionException" => {
            ScriptExecutionException::new(message, code).into()
        }
        r"Composer\Json\JsonValidationException" => {
            JsonValidationException::new(message, strings("errors")).into()
        }
        r"Composer\Package\Loader\InvalidPackageException" => InvalidPackageException::new(
            strings("errors"),
            strings("warnings"),
            match property("data") {
                PhpMixed::Array(data) => data,
                _ => IndexMap::new(),
            },
        )
        .into(),
        r"Composer\Plugin\PluginBlockedException" => PluginBlockedException::new(message).into(),
        r"Composer\Repository\InvalidRepositoryException" => {
            InvalidRepositoryException::new(message).into()
        }
        r"Composer\Repository\RepositorySecurityException" => {
            RepositorySecurityException::new(message).into()
        }
        r"Symfony\Component\Console\Exception\CommandNotFoundException" => {
            shirabe_symfony_console::exception::CommandNotFoundException::new(
                message,
                strings("alternatives"),
                code,
            )
            .into()
        }
        r"Symfony\Component\Console\Exception\NamespaceNotFoundException" => {
            shirabe_symfony_console::exception::NamespaceNotFoundException::new(
                message,
                strings("alternatives"),
                code,
            )
            .into()
        }
        r"Symfony\Component\Console\Exception\InvalidArgumentException" => {
            shirabe_symfony_console::exception::InvalidArgumentException::new(message).into()
        }
        r"Symfony\Component\Console\Exception\InvalidOptionException" => {
            shirabe_symfony_console::exception::InvalidOptionException::new(message).into()
        }
        r"Symfony\Component\Console\Exception\LogicException" => {
            shirabe_symfony_console::exception::LogicException::new(message).into()
        }
        r"Symfony\Component\Console\Exception\MissingInputException" => {
            shirabe_symfony_console::exception::MissingInputException::new(message).into()
        }
        r"Symfony\Component\Console\Exception\RuntimeException" => {
            shirabe_symfony_console::exception::RuntimeException::new(message).into()
        }
        r"Symfony\Component\Process\Exception\InvalidArgumentException" => {
            shirabe_symfony_process::exception::InvalidArgumentException::new(message).into()
        }
        r"Symfony\Component\Process\Exception\LogicException" => {
            shirabe_symfony_process::exception::LogicException::new(message).into()
        }
        r"Symfony\Component\Process\Exception\RuntimeException" => {
            shirabe_symfony_process::exception::RuntimeException::new(message).into()
        }
        r"Symfony\Component\Filesystem\Exception\IOException" => {
            shirabe_symfony_filesystem::exception::IOException::new(message, code, None, None)
                .into()
        }
        // TODO(plugin): ProcessFailedException, ProcessSignaledException,
        // ProcessTimedOutException and SolverProblemsException are built from the Process or the
        // solver problems they describe, which do not cross, so they come back as their nearest
        // superclass under their own name.
        _ => return None,
    })
}

/// The `Throw` a ported exception crosses to the PHP worker as.
pub(crate) fn exception_into_throw(exception: &AnyThrowable, error: &anyhow::Error) -> PhpThrow {
    let mut throw = PhpThrow::new(
        exception.php_class_name(),
        exception.get_message().to_string(),
        exception.get_code(),
    );
    *throw.properties = throwable_properties(error);
    throw.file = exception.get_file().into();
    throw.line = exception.get_line();
    throw
}

/// The state a ported exception carries beyond `message` and `code`, keyed by the property names
/// its PHP class declares. The worker revives them onto the instance it rebuilds.
///
/// TODO(plugin): the state of ProcessFailedException, ProcessSignaledException and
/// SolverProblemsException is the Process or the solver problems, which do not cross, and an
/// exception of a class the port has no type for keeps none of its own.
fn throwable_properties(error: &anyhow::Error) -> IndexMap<bstr::BString, PluginValue> {
    let strings = |items: &[String]| {
        PluginValue::List(items.iter().cloned().map(PluginValue::string).collect())
    };
    let mut properties = IndexMap::new();
    if let Some(exception) = error.catch::<TransportException>() {
        properties.insert(
            "headers".into(),
            match exception.get_headers() {
                Some(headers) => strings(headers),
                None => PluginValue::Null,
            },
        );
        properties.insert(
            "response".into(),
            match exception.get_response() {
                Some(response) => PluginValue::string(response),
                None => PluginValue::Null,
            },
        );
        properties.insert(
            "statusCode".into(),
            match exception.get_status_code() {
                Some(status_code) => PluginValue::Int(status_code),
                None => PluginValue::Null,
            },
        );
        properties.insert(
            "responseInfo".into(),
            PluginValue::List(
                exception
                    .get_response_info()
                    .iter()
                    .map(PluginValue::from_php_mixed)
                    .collect(),
            ),
        );
    }
    if let Some(exception) = error.catch::<InvalidPackageException>() {
        properties.insert("errors".into(), strings(exception.get_errors()));
        properties.insert("warnings".into(), strings(exception.get_warnings()));
        properties.insert(
            "data".into(),
            PluginValue::from_php_mixed(&PhpMixed::Array(exception.get_data().clone())),
        );
    }
    if let Some(exception) = error.catch::<JsonValidationException>() {
        properties.insert("errors".into(), strings(exception.get_errors()));
    }
    if let Some(exception) =
        error.catch::<shirabe_symfony_console::exception::CommandNotFoundException>()
    {
        properties.insert("alternatives".into(), strings(exception.get_alternatives()));
    }
    properties
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throw(class: &str, parents: &[&str]) -> PhpThrow {
        let mut throw = PhpThrow::new(class.to_string(), "boom".to_string(), 7);
        throw.file = bstr::BString::from("/project/src/Thrower.php".to_string());
        throw.line = 12;
        throw.parents = parents.iter().map(|parent| parent.to_string()).collect();
        throw
    }

    #[test]
    fn a_ported_class_comes_back_as_itself() {
        let error = throw_into_error(throw(
            "InvalidArgumentException",
            &["LogicException", "Exception"],
        ));

        assert!(error.is_class::<InvalidArgumentException>());
        let exception = AnyThrowable::of(&error).unwrap();
        assert_eq!("boom", exception.get_message());
        assert_eq!(7, exception.get_code());
        assert_eq!("/project/src/Thrower.php", exception.get_file());
        assert_eq!(12, exception.get_line());
    }

    #[test]
    fn an_unported_class_comes_back_as_its_nearest_ported_superclass() {
        let error = throw_into_error(throw(
            r"Vendor\PluginException",
            &["UnexpectedValueException", "RuntimeException", "Exception"],
        ));

        let exception = AnyThrowable::of(&error).unwrap();
        assert_eq!(r"Vendor\PluginException", exception.php_class_name());
        assert_eq!(7, exception.get_code());
        assert!(error.is_instanceof::<UnexpectedValueException>());
        assert!(!error.is_instanceof::<LogicException>());

        let back = exception_into_throw(exception, &error);
        assert_eq!(r"Vendor\PluginException", back.exception_class);
        assert_eq!("/project/src/Thrower.php", back.file);
    }

    #[test]
    fn the_state_of_a_ported_class_crosses_both_ways() {
        let mut thrown = throw(
            r"Composer\Downloader\TransportException",
            &["RuntimeException", "Exception"],
        );
        thrown.properties.insert(
            bstr::BString::from("statusCode".to_string()),
            PluginValue::Int(404),
        );
        thrown.properties.insert(
            bstr::BString::from("response".to_string()),
            PluginValue::string("missing"),
        );
        let error = throw_into_error(thrown);

        let exception = error.catch::<TransportException>().unwrap();
        assert_eq!(Some(404), exception.get_status_code());
        assert_eq!(Some("missing"), exception.get_response());

        let back = exception_into_throw(AnyThrowable::of(&error).unwrap(), &error);
        assert_eq!(
            Some(&PluginValue::Int(404)),
            back.properties.get(bstr::B("statusCode"))
        );
    }
}
