use std::path::PathBuf;

pub(crate) fn managed_plugins_dir() -> PathBuf {
    crate::config::config_dir().join("plugins")
}

pub(crate) fn managed_checkout_path(plugin_id: &str) -> PathBuf {
    managed_plugins_dir()
        .join("github")
        .join(crate::api::schema::plugin_managed_path_component(plugin_id))
}
