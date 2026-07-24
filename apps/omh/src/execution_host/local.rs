use super::operations::{self, HostOperationError};
use super::protocol::PortSnapshot;
use super::ResourceLocation;

/// Local-only host probe for the coordinator process.
///
/// Terminal runtimes stay on the honest `TerminalRuntimeRegistry` owned by App.
/// This module only validates local ResourceLocation boundaries for in-process
/// host operations.
pub(crate) fn observe_ports(
    location: &ResourceLocation,
) -> Result<Vec<PortSnapshot>, HostOperationError> {
    operations::validate_local_location(location)?;
    Ok(operations::local_ports())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_host::{ExecutionHostId, HostPath};

    #[test]
    fn local_operations_reject_remote_locations() {
        let remote = ResourceLocation::new(
            ExecutionHostId::new("ssh:workbox").expect("remote host id"),
            HostPath::new("/srv/work").expect("remote path"),
        );
        let error =
            observe_ports(&remote).expect_err("local adapter must not inspect a remote path");
        assert!(matches!(error, HostOperationError::InvalidLocation(_)));
    }

    #[test]
    fn local_operations_accept_local_locations() {
        let local = ResourceLocation::local("/tmp").expect("local location");
        // May return empty on restricted environments; must not reject location.
        let _ = observe_ports(&local).expect("local location should be accepted");
    }
}
