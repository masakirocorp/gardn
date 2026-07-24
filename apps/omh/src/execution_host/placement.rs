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

/// Explicit location wins, then the invoking client's focused terminal, else
/// the workspace default location.
pub(crate) fn resolve_tab_creation(
    explicit: Option<ResourceLocation>,
    invoking_client_terminal: Option<ResourceLocation>,
    workspace_default: ResourceLocation,
) -> ResourceLocation {
    if let Some(location) = explicit {
        return location;
    }
    if let Some(location) = invoking_client_terminal {
        return location;
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
    fn tab_creation_prefers_invoking_client_terminal_over_workspace_default() {
        let invoking = location("ssh:workbox:1", "/srv/focused");
        let workspace_default = location("local", "/tmp/workspace");
        let resolved = resolve_tab_creation(None, Some(invoking.clone()), workspace_default);
        assert_eq!(resolved, invoking);
    }

    #[test]
    fn split_creation_uses_source_terminal_without_explicit() {
        let source = location("ssh:workbox:1", "/srv/source");
        let resolved = resolve_split_creation(None, source.clone());
        assert_eq!(resolved, source);
    }
}
