//! ref: composer/src/Composer/Plugin/PrePoolCreateEvent.php sha1:57583afaa336d40050e18d93fedbaf6ed2726d1b

use crate::dependency_resolver::Request;
use crate::event_dispatcher::{Event, EventFlags, delegate_event_interface_to_inner};
use crate::package::BasePackageHandle;
use crate::repository::RepositoryInterfaceHandle;
use bstr::BString;
use indexmap::IndexMap;

#[derive(Debug)]
pub struct PrePoolCreateEvent {
    inner: Event,
    repositories: Vec<RepositoryInterfaceHandle>,
    request: Request,
    acceptable_stabilities: IndexMap<BString, i64>,
    stability_flags: IndexMap<BString, i64>,
    root_aliases: IndexMap<BString, IndexMap<BString, IndexMap<BString, BString>>>,
    root_references: IndexMap<BString, BString>,
    // TODO(type-model): PoolBuilder's package array keeps the keys of the entries it unset, which
    // a listener sees in PHP; the list here is reindexed.
    packages: Vec<BasePackageHandle>,
    unacceptable_fixed_packages: Vec<BasePackageHandle>,
}

impl PrePoolCreateEvent {
    pub fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<BString>,
        repositories: Vec<RepositoryInterfaceHandle>,
        request: Request,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        root_aliases: IndexMap<BString, IndexMap<BString, IndexMap<BString, BString>>>,
        root_references: IndexMap<BString, BString>,
        packages: Vec<BasePackageHandle>,
        unacceptable_fixed_packages: Vec<BasePackageHandle>,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], EventFlags::default()),
            repositories,
            request,
            acceptable_stabilities,
            stability_flags,
            root_aliases,
            root_references,
            packages,
            unacceptable_fixed_packages,
        }
    }

    pub fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    pub fn get_request(&self) -> &Request {
        &self.request
    }

    /// Hands back the request the event was built with. `PoolBuilder` lends the event the
    /// `Request` it is building the pool for, which PHP shares by reference, and takes it back
    /// once the listeners have run, so whatever they changed on it reaches the solver.
    ///
    /// TODO(plugin): the event is left holding an empty request, so a listener that keeps the
    /// event past its dispatch no longer reaches the request the solver goes on to use; sharing
    /// it needs the `Request` in shared ownership once it crosses the boundary at all.
    pub(crate) fn take_request(&mut self) -> Request {
        std::mem::replace(&mut self.request, Request::new(None))
    }

    pub fn get_acceptable_stabilities(&self) -> &IndexMap<BString, i64> {
        &self.acceptable_stabilities
    }

    pub fn get_stability_flags(&self) -> &IndexMap<BString, i64> {
        &self.stability_flags
    }

    pub fn get_root_aliases(
        &self,
    ) -> &IndexMap<BString, IndexMap<BString, IndexMap<BString, BString>>> {
        &self.root_aliases
    }

    pub fn get_root_references(&self) -> &IndexMap<BString, BString> {
        &self.root_references
    }

    pub fn get_packages(&self) -> &Vec<BasePackageHandle> {
        &self.packages
    }

    pub fn get_unacceptable_fixed_packages(&self) -> &Vec<BasePackageHandle> {
        &self.unacceptable_fixed_packages
    }

    pub fn set_packages(&mut self, packages: Vec<BasePackageHandle>) {
        self.packages = packages;
    }

    pub fn set_unacceptable_fixed_packages(&mut self, packages: Vec<BasePackageHandle>) {
        self.unacceptable_fixed_packages = packages;
    }
}

delegate_event_interface_to_inner!(PrePoolCreateEvent, inner);
