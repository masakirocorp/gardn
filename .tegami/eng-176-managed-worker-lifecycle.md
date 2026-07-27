# Managed SSH worker lifecycle

Oh My Herdr now installs and updates versioned SSH execution workers automatically when a connection starts. Compatible workers with live runtimes stay active until they are unused.

Removing an SSH connection now shows its impact across all sessions before confirmation. The removal fences new work, drains remote panes, updates dormant session placement, removes only worker bindings owned by that connection, and keeps durable retry state if any step cannot finish safely. When remote cleanup is unavailable, a separate confirmed local-only forget removes local state without claiming to stop remote work.
