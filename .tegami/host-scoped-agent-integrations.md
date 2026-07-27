# Manage agent integrations on SSH hosts

The Integrations settings can now inspect, install, update, and uninstall agent integrations on Local or a configured SSH execution host.

Remote agent panes now send lifecycle reports through a restricted, token-authenticated worker endpoint. The coordinator Local API socket and unrelated profile arguments or environment variables are not exposed to the remote pane.
