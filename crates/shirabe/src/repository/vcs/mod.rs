pub mod forgejo_driver;
pub mod fossil_driver;
pub mod git_bitbucket_driver;
pub mod git_driver;
pub mod github_driver;
pub mod gitlab_driver;
pub mod hg_driver;
pub mod perforce_driver;
pub mod svn_driver;
pub mod vcs_driver;
pub mod vcs_driver_interface;

pub use forgejo_driver::*;
pub use fossil_driver::*;
pub use git_bitbucket_driver::*;
pub use git_driver::*;
pub use github_driver::*;
pub use gitlab_driver::*;
pub use hg_driver::*;
pub use perforce_driver::*;
pub use svn_driver::*;
pub use vcs_driver::*;
pub use vcs_driver_interface::*;

use crate::config::Config;
use crate::io::IOInterface;
use crate::util::{HttpDownloader, ProcessExecutor};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcsDriverKind {
    GitHub,
    GitLab,
    GitBitbucket,
    Forgejo,
    Git,
    Hg,
    Perforce,
    Fossil,
    Svn,
}

impl VcsDriverKind {
    pub fn instantiate(
        self,
        repo_config: IndexMap<String, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Box<dyn VcsDriverInterface> {
        match self {
            VcsDriverKind::GitHub => Box::new(GitHubDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::GitLab => Box::new(GitLabDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::GitBitbucket => Box::new(GitBitbucketDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Forgejo => Box::new(ForgejoDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Git => Box::new(GitDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Hg => Box::new(HgDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Perforce => Box::new(PerforceDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Fossil => Box::new(FossilDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
            VcsDriverKind::Svn => Box::new(SvnDriver::new(
                repo_config,
                io,
                config,
                http_downloader,
                process,
            )),
        }
    }

    pub fn supports(
        self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        match self {
            VcsDriverKind::GitHub => GitHubDriver::supports(io, config, url, deep),
            VcsDriverKind::GitLab => GitLabDriver::supports(io, config, url, deep),
            VcsDriverKind::GitBitbucket => GitBitbucketDriver::supports(io, config, url, deep),
            VcsDriverKind::Forgejo => ForgejoDriver::supports(io, config, url, deep),
            VcsDriverKind::Git => GitDriver::supports(io, config, url, deep),
            VcsDriverKind::Hg => HgDriver::supports(io, config, url, deep),
            VcsDriverKind::Perforce => PerforceDriver::supports(io, config, url, deep),
            VcsDriverKind::Fossil => FossilDriver::supports(io, config, url, deep),
            VcsDriverKind::Svn => SvnDriver::supports(io, config, url, deep),
        }
    }

    /// PHP fully-qualified `class-string`, used as the fallback driver name in `getRepoName()`.
    pub fn php_class_name(self) -> &'static str {
        match self {
            VcsDriverKind::GitHub => "Composer\\Repository\\Vcs\\GitHubDriver",
            VcsDriverKind::GitLab => "Composer\\Repository\\Vcs\\GitLabDriver",
            VcsDriverKind::GitBitbucket => "Composer\\Repository\\Vcs\\GitBitbucketDriver",
            VcsDriverKind::Forgejo => "Composer\\Repository\\Vcs\\ForgejoDriver",
            VcsDriverKind::Git => "Composer\\Repository\\Vcs\\GitDriver",
            VcsDriverKind::Hg => "Composer\\Repository\\Vcs\\HgDriver",
            VcsDriverKind::Perforce => "Composer\\Repository\\Vcs\\PerforceDriver",
            VcsDriverKind::Fossil => "Composer\\Repository\\Vcs\\FossilDriver",
            VcsDriverKind::Svn => "Composer\\Repository\\Vcs\\SvnDriver",
        }
    }
}
