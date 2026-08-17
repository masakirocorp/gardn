# Managed SSH worker lifecycle

Oh My Herdr now installs and updates versioned SSH execution workers automatically when a connection starts. Protocol-compatible workers with live runtimes stay active until they are unused. After a coordinator restart, restored remote panes reconnect their saved SSH connection and re-adopt the live worker runtime. Worker bridges use dedicated SSH transports so a stopped coordinator cannot leave orphaned remote bridge processes that block the next reconnect.

Development builds now install SSH workers from matching local build cohorts instead of published release assets. `just install-dev` installs `omh-dev` and both Linux worker sidecars as one operation. If a sidecar is missing, Oh My Herdr names that command instead of exposing the internal build cohort and filesystem path. Oh My Herdr verifies the staged worker's source identity, target, protocols, lifecycle, and capabilities before it publishes the worker manifest.

Removing an SSH connection now shows its impact across all sessions before confirmation. The removal fences new work, drains remote panes, updates dormant session placement, removes only worker bindings owned by that connection, and keeps durable retry state if any step cannot finish safely. When remote cleanup is unavailable, the failure screen warns that remote work might remain and offers local removal, retry, or cancel.
