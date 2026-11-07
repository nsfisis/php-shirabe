//! The methods a `Composer\Util\Filesystem` entity answers over RPC.

use crate::plugin::job_registry;
use crate::plugin::job_registry::JobOwner;
use crate::plugin::rpc::codec::arg;
use crate::plugin::rpc::dispatch::{error_throw, unavailable_method};
use crate::plugin::rpc::jobs::{poll_once, resolved_promise};
use crate::util::Filesystem;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{bformat, path_from_bytes};

pub(crate) fn dispatch_filesystem_method(
    fs: &std::rc::Rc<std::cell::RefCell<Filesystem>>,
    method_name: &BStr,
    args: &[PluginValue],
) -> Result<PluginValue, PhpThrow> {
    let path_arg = |position: usize| {
        arg::<BString>(method_name, args, position)
            .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
    };
    let failed = |error: anyhow::Error| error_throw(bformat!("{method_name} failed"), &error);
    match method_name.as_bytes() {
        b"remove" => Ok(fs
            .borrow_mut()
            .remove(path_arg(0)?)
            .map_err(failed)?
            .to_wire()),
        b"isDirEmpty" => Ok(fs.borrow().is_dir_empty(&path_arg(0)?).to_wire()),
        b"emptyDirectory" => {
            fs.borrow_mut()
                .empty_directory(&path_arg(0)?, arg::<bool>(method_name, args, 1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"removeDirectory" => Ok(fs
            .borrow_mut()
            .remove_directory(path_arg(0)?)
            .map_err(failed)?
            .to_wire()),
        b"removeDirectoryAsync" => {
            let directory = path_arg(0)?;
            let executor = fs.borrow_mut().get_process_handle();
            let fs = fs.clone();
            let mut future: std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<bool>>>,
            > = Box::pin(
                async move { Filesystem::remove_directory_async_via(&fs, &directory).await },
            );
            // PHP settles the edge cases and starts the removal process before the call returns,
            // and raises what goes wrong up to there out of the call.
            match poll_once(&mut future) {
                std::task::Poll::Ready(removed) => {
                    resolved_promise(removed.map_err(failed)?.to_wire())
                }
                std::task::Poll::Pending => Ok(job_registry::register(
                    JobOwner::process_executor(&executor),
                    executor,
                    std::rc::Rc::new(std::cell::Cell::new(true)),
                    PhpThrow::runtime("Aborted process"),
                    Box::pin(async move {
                        future
                            .await
                            .map(|removed| removed.to_wire())
                            .map_err(|error| error_throw(b"removeDirectoryAsync failed", &error))
                    }),
                )),
            }
        }
        b"removeDirectoryPhp" => Ok(fs
            .borrow_mut()
            .remove_directory_php(&path_arg(0)?)
            .map_err(failed)?
            .to_wire()),
        b"ensureDirectoryExists" => {
            fs.borrow_mut()
                .ensure_directory_exists(&path_arg(0)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"unlink" => Ok(fs.borrow().unlink(path_arg(0)?).map_err(failed)?.to_wire()),
        b"rmdir" => Ok(fs.borrow().rmdir(path_arg(0)?).map_err(failed)?.to_wire()),
        b"copyThenRemove" => {
            fs.borrow_mut()
                .copy_then_remove(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"copy" => Ok(fs
            .borrow_mut()
            .copy(&path_arg(0)?, &path_arg(1)?)
            .map_err(failed)?
            .to_wire()),
        b"rename" => {
            fs.borrow_mut()
                .rename(path_arg(0)?, path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"findShortestPath" => Ok(fs
            .borrow()
            .find_shortest_path(
                &path_arg(0)?,
                &path_arg(1)?,
                arg::<bool>(method_name, args, 2)?,
                arg::<bool>(method_name, args, 3)?,
            )
            .map_err(failed)?
            .to_wire()),
        b"findShortestPathCode" => Ok(fs
            .borrow()
            .find_shortest_path_code(
                &path_arg(0)?,
                &path_arg(1)?,
                arg::<bool>(method_name, args, 2)?,
                arg::<bool>(method_name, args, 3)?,
                arg::<bool>(method_name, args, 4)?,
            )
            .map_err(failed)?
            .to_wire()),
        b"isAbsolutePath" => Ok(fs.borrow().is_absolute_path(&path_arg(0)?).to_wire()),
        b"size" => Ok(fs.borrow().size(path_arg(0)?).map_err(failed)?.to_wire()),
        b"normalizePath" => Ok(fs.borrow().normalize_path(&path_arg(0)?).to_wire()),
        b"relativeSymlink" => Ok(fs
            .borrow()
            .relative_symlink(&path_arg(0)?, &path_arg(1)?)
            .map_err(failed)?
            .to_wire()),
        b"isSymlinkedDirectory" => Ok(fs.borrow().is_symlinked_directory(&path_arg(0)?).to_wire()),
        b"junction" => {
            fs.borrow_mut()
                .junction(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        b"isJunction" => Ok(fs.borrow().is_junction(&path_arg(0)?).to_wire()),
        b"removeJunction" => Ok(fs
            .borrow_mut()
            .remove_junction(&path_arg(0)?)
            .map_err(failed)?
            .to_wire()),
        b"filePutContentsIfModified" => Ok(fs
            .borrow()
            .file_put_contents_if_modified(&path_arg(0)?, arg::<BString>(method_name, args, 1)?)
            .map_err(failed)?
            .to_wire()),
        b"safeCopy" => {
            fs.borrow()
                .safe_copy(&path_arg(0)?, &path_arg(1)?)
                .map_err(failed)?;
            Ok(PluginValue::Null)
        }
        other => Err(unavailable_method("Filesystem", other)),
    }
}
