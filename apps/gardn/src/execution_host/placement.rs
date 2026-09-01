use super::ResourceLocation;

/// Explicit location wins, then group default, else the local fallback path.
pub(crate) fn resolve_workspace_creation(
    explicit: Option<ResourceLocation>,
    group_default: Option<ResourceLocation>,
    local_fallback: ResourceLocation,
) -> ResourceLocation {
    if let Some(location) = explicit {
        return location;
    }
    if let Some(location) = group_default {
        return location;
    }
    local_fallback
}

/// Explicit location wins. A focused live terminal wins only when it is on the
/// same execution host as the workspace default, so a changed Space default
/// actually moves new tabs. Split still follows the source pane.
pub(crate) fn resolve_tab_creation(
    explicit: Option<ResourceLocation>,
    invoking_client_terminal: Option<ResourceLocation>,
    workspace_default: ResourceLocation,
) -> ResourceLocation {
    if let Some(location) = explicit {
        return location;
    }
    if let Some(location) = invoking_client_terminal {
        if location.execution_host_id == workspace_default.execution_host_id {
            return location;
        }
    }
    workspace_default
}

/// Explicit location wins, otherwise the split source terminal location.
pub(crate) fn resolve_split_creation(
    explicit: Option<ResourceLocation>,
    source_terminal: ResourceLocation,
) -> ResourceLocation {
    if let Some(location) = explicit {
        return location;
    }
    source_terminal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::{ExecutionHostId, HostPath};

    fn location(host: &str, path: &str) -> ResourceLocation {
        ResourceLocation::new(
            ExecutionHostId::new(host).expect("host id"),
            HostPath::new(path).expect("path"),
        )
    }

    #[test]
    fn workspace_creation_prefers_group_default_over_local_fallback() {
        let group_default = location("ssh:workbox:1", "/srv/group");
        let local_fallback = location("local", "/tmp/local");
        let resolved =
            resolve_workspace_creation(None, Some(group_default.clone()), local_fallback);
        assert_eq!(resolved, group_default);
    }

    #[test]
    fn workspace_creation_prefers_explicit_over_group_default() {
        let explicit = location("ssh:other:1", "/srv/explicit");
        let group_default = location("ssh:workbox:1", "/srv/group");
        let local_fallback = location("local", "/tmp/local");
        let resolved =
            resolve_workspace_creation(Some(explicit.clone()), Some(group_default), local_fallback);
        assert_eq!(resolved, explicit);
    }

    #[test]
    fn tab_creation_follows_focused_terminal_on_the_workspace_host() {
        let invoking = location("ssh:workbox:1", "/srv/focused");
        let workspace_default = location("ssh:workbox:1", "/srv/workspace");
        let resolved = resolve_tab_creation(None, Some(invoking.clone()), workspace_default);
        assert_eq!(resolved, invoking);
    }

    #[test]
    fn tab_creation_uses_workspace_default_when_focused_host_differs() {
        let invoking = location("local", "/tmp/focused");
        let workspace_default = location("ssh:eva-01:1", "~/projects");
        let resolved = resolve_tab_creation(None, Some(invoking), workspace_default.clone());
        assert_eq!(resolved, workspace_default);
    }

    #[test]
    fn split_creation_uses_source_terminal_without_explicit() {
        let source = location("ssh:workbox:1", "/srv/source");
        let resolved = resolve_split_creation(None, source.clone());
        assert_eq!(resolved, source);
    }
}
