//! The methods a `Composer\Util\Filesystem` entity answers over RPC.

use crate::php_worker::job;
use crate::php_worker::job::JobOwner;
use crate::php_worker::job::rpc::{poll_once, resolved_promise};
use crate::php_worker::rpc::dispatch::error_throw;
use crate::php_worker::rpc::entity::{Call, Crossing, Entity};
use crate::php_worker::rpc::forwarding;
use crate::util::{Filesystem, ProcessExecutor};
use bstr::BString;
use shirabe_php_rpc::{PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::path_from_bytes;

impl Entity for std::rc::Rc<std::cell::RefCell<Filesystem>> {
    const EXPECTED: &'static [u8] = b"a Filesystem";

    fn address(&self) -> *const () {
        std::rc::Rc::as_ptr(self) as *const ()
    }

    fn class(&self) -> Result<&'static Crossing<Self>, PhpThrow> {
        Ok(&forwarding::util::filesystem::CROSSING)
    }
}

fn path(call: &Call<'_>, position: usize) -> Result<std::path::PathBuf, PhpThrow> {
    call.arg::<BString>(position)
        .map(|path| std::path::PathBuf::from(path_from_bytes(&path)))
}

impl forwarding::util::filesystem::Forwarding for forwarding::util::filesystem::PhpClass {
    type Handle = std::rc::Rc<std::cell::RefCell<Filesystem>>;

    // A Filesystem shares no state with the object graph beyond the process executor it runs
    // subprocesses through, so a plugin-built one is a complete instance rather than a second
    // view on a Rust-side service.
    fn __construct(call: &mut Call<'_>) -> Result<Self::Handle, PhpThrow> {
        let executor =
            call.optional_entity::<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>(0)?;
        Ok(std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(
            executor,
        ))))
    }

    fn remove(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow_mut()
            .remove(path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn is_dir_empty(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs.borrow().is_dir_empty(&path(call, 0)?).to_wire())
    }

    fn empty_directory(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        fs.borrow_mut()
            .empty_directory(&path(call, 0)?, call.arg::<bool>(1)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn remove_directory(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow_mut()
            .remove_directory(path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn remove_directory_async(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        let directory = path(call, 0)?;
        let executor = fs.borrow_mut().get_process_handle();
        let fs = fs.clone();
        let mut future: std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<bool>>>> =
            Box::pin(async move { Filesystem::remove_directory_async_via(&fs, &directory).await });
        // PHP settles the edge cases and starts the removal process before the call returns, and
        // raises what goes wrong up to there out of the call.
        match poll_once(&mut future) {
            std::task::Poll::Ready(removed) => {
                resolved_promise(removed.map_err(call.failed())?.to_wire())
            }
            std::task::Poll::Pending => Ok(job::register(
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

    fn remove_directory_php(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow_mut()
            .remove_directory_php(&path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn ensure_directory_exists(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        fs.borrow_mut()
            .ensure_directory_exists(&path(call, 0)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn unlink(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .unlink(path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn rmdir(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .rmdir(path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn copy_then_remove(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        fs.borrow_mut()
            .copy_then_remove(&path(call, 0)?, &path(call, 1)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn copy(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow_mut()
            .copy(&path(call, 0)?, &path(call, 1)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn rename(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        fs.borrow_mut()
            .rename(path(call, 0)?, path(call, 1)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn find_shortest_path(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .find_shortest_path(
                &path(call, 0)?,
                &path(call, 1)?,
                call.arg::<bool>(2)?,
                call.arg::<bool>(3)?,
            )
            .map_err(call.failed())?
            .to_wire())
    }

    fn find_shortest_path_code(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .find_shortest_path_code(
                &path(call, 0)?,
                &path(call, 1)?,
                call.arg::<bool>(2)?,
                call.arg::<bool>(3)?,
                call.arg::<bool>(4)?,
            )
            .map_err(call.failed())?
            .to_wire())
    }

    fn is_absolute_path(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs.borrow().is_absolute_path(&path(call, 0)?).to_wire())
    }

    fn size(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .size(path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn normalize_path(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs.borrow().normalize_path(&path(call, 0)?).to_wire())
    }

    fn relative_symlink(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .relative_symlink(&path(call, 0)?, &path(call, 1)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn is_symlinked_directory(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .is_symlinked_directory(&path(call, 0)?)
            .to_wire())
    }

    fn junction(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        fs.borrow_mut()
            .junction(&path(call, 0)?, &path(call, 1)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }

    fn is_junction(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs.borrow().is_junction(&path(call, 0)?).to_wire())
    }

    fn remove_junction(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow_mut()
            .remove_junction(&path(call, 0)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn file_put_contents_if_modified(
        fs: &Self::Handle,
        call: &mut Call<'_>,
    ) -> Result<PluginValue, PhpThrow> {
        Ok(fs
            .borrow()
            .file_put_contents_if_modified(&path(call, 0)?, call.arg::<BString>(1)?)
            .map_err(call.failed())?
            .to_wire())
    }

    fn safe_copy(fs: &Self::Handle, call: &mut Call<'_>) -> Result<PluginValue, PhpThrow> {
        fs.borrow()
            .safe_copy(&path(call, 0)?, &path(call, 1)?)
            .map_err(call.failed())?;
        Ok(PluginValue::Null)
    }
}
