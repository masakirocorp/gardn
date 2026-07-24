mod config;
#[cfg(test)]
mod config_tests;
mod discovery;
mod status;
#[cfg(test)]
mod test_support;

pub(crate) use self::{
    discovery::git_repo_root,
    status::{git_work_summary, git_work_summary_for_root},
};
pub use self::{
    discovery::{derive_label_from_cwd, derive_label_from_location, git_branch},
    status::{git_status_cache_key, git_status_snapshot_for_cwd, GitStatusCacheEntry},
};

#[cfg(test)]
pub(super) use self::status::git_ahead_behind;
