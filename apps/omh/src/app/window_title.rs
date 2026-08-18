//! Outer terminal window title.
//!
//! Oh My Herdr is a terminal emulator, so `OSC 0`/`OSC 2` written by a pane
//! stops at Oh My Herdr and never reaches the terminal Oh My Herdr itself runs
//! in. Without this the host window title keeps whatever the shell or `ssh`
//! left behind, which is what window managers show in tab and group bars.
//!
//! The title is rendered on the server so `{hostname}` names the host the panes
//! actually live on, not the machine a thin remote client runs on. The server
//! pushes the result to the foreground client, which writes the `OSC 0`.

use super::App;
use crate::config::{WindowTitlePart, WindowTitleTemplate, WindowTitleToken};

impl App {
    pub(crate) fn configure_window_title(&mut self, template: &str) {
        self.window_title_template =
            WindowTitleTemplate::parse(template)
                .ok()
                .flatten()
                .map(|template| {
                    // Resolve the hostname once here rather than per render.
                    let hostname = if template.uses(WindowTitleToken::Hostname) {
                        crate::platform::hostname().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    (template, hostname)
                });
    }

    /// Whether `ui.window_title` asks Oh My Herdr to own the outer terminal
    /// title at all. When it does not, Oh My Herdr leaves whatever the shell
    /// or `ssh` set.
    pub(crate) fn window_title_configured(&self) -> bool {
        self.window_title_template.is_some()
    }

    /// Whether the title depends on the focused pane's own terminal title,
    /// which is the one input that arrives through PTY parsing rather than app
    /// state.
    pub(crate) fn window_title_uses_terminal_title(&self) -> bool {
        self.window_title_template
            .as_ref()
            .is_some_and(|(template, _)| template.uses(WindowTitleToken::TerminalTitle))
    }

    /// Renders the configured outer window title, or `None` when window titles
    /// are disabled or every token resolved empty.
    pub(crate) fn window_title(&self) -> Option<String> {
        let (template, hostname) = self.window_title_template.as_ref()?;
        let workspace = self
            .state
            .active
            .and_then(|ws_idx| self.state.workspaces.get(ws_idx));

        let mut title = String::new();
        for part in template.parts() {
            match part {
                WindowTitlePart::Literal(literal) => title.push_str(literal),
                WindowTitlePart::Token(WindowTitleToken::Hostname) => title.push_str(hostname),
                WindowTitlePart::Token(WindowTitleToken::Workspace) => {
                    if let Some(workspace) = workspace {
                        title.push_str(
                            &workspace
                                .display_name_from(&self.state.terminals, &self.terminal_runtimes),
                        );
                    }
                }
                WindowTitlePart::Token(WindowTitleToken::Tab) => {
                    if let Some(name) = workspace.and_then(|ws| ws.active_tab_display_name()) {
                        title.push_str(&name);
                    }
                }
                WindowTitlePart::Token(WindowTitleToken::Pane) => {
                    if let Some(label) = self
                        .focused_terminal_state()
                        .and_then(|terminal| terminal.manual_label.as_deref())
                    {
                        title.push_str(label);
                    }
                }
                WindowTitlePart::Token(WindowTitleToken::TerminalTitle) => {
                    if let Some(terminal_title) = self.focused_terminal_title_stripped() {
                        title.push_str(&terminal_title);
                    }
                }
            }
        }

        Some(title)
    }

    /// Consumes the focused pane's terminal-title dirty flag. PTY parsing
    /// marks it when an `OSC 0`/`OSC 2` changes the retained title, so the
    /// event loop can re-sync the outer title without polling every pane.
    pub(crate) fn take_focused_terminal_title_dirty(&self) -> bool {
        let Some(ws_idx) = self.state.active else {
            return false;
        };
        let Some(workspace) = self.state.workspaces.get(ws_idx) else {
            return false;
        };
        let Some(pane_id) = workspace.focused_pane_id() else {
            return false;
        };
        self.state
            .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)
            .is_some_and(|runtime| runtime.take_agent_osc_title_dirty())
    }

    fn focused_terminal_state(&self) -> Option<&crate::terminal::TerminalState> {
        let workspace = self.state.workspaces.get(self.state.active?)?;
        let terminal_id = workspace.terminal_id(workspace.focused_pane_id()?)?;
        self.state.terminals.get(terminal_id)
    }

    fn focused_terminal_title_stripped(&self) -> Option<String> {
        let ws_idx = self.state.active?;
        let workspace = self.state.workspaces.get(ws_idx)?;
        let pane_id = workspace.focused_pane_id()?;
        let runtime =
            self.state
                .runtime_for_pane_in_workspace(&self.terminal_runtimes, ws_idx, pane_id)?;
        crate::terminal::stripped_terminal_title(&runtime.agent_osc_title())
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::config::Config;
    use crate::workspace::Workspace;

    fn test_app() -> App {
        let event_hub = crate::api::EventHub::default();
        let (_api_tx, api_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(&Config::default(), true, None, api_rx, event_hub);
        app.state.workspaces = vec![Workspace::test_new("herd")];
        app.state.active = Some(0);
        app.state.ensure_test_terminals();
        app
    }

    #[test]
    fn renders_workspace_and_tab_names() {
        let mut app = test_app();
        app.configure_window_title("{workspace}/{tab}");

        assert_eq!(app.window_title().as_deref(), Some("herd/1"));

        app.state.workspaces[0].tabs[0].custom_name = Some("build".into());
        assert_eq!(app.window_title().as_deref(), Some("herd/build"));
    }

    #[tokio::test]
    async fn renders_focused_pane_label_and_terminal_title() {
        let mut app = test_app();
        app.configure_window_title("{pane}|{terminal_title}");

        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app
            .state
            .terminals
            .get_mut(&terminal_id)
            .expect("focused terminal");
        terminal.manual_label = Some("api".into());

        let runtime = crate::terminal::TerminalRuntime::test_with_screen_bytes(80, 24, b"");
        runtime.test_process_pty_bytes(pane_id, "\x1b]0;⠋ building\x07".as_bytes());
        app.terminal_runtimes.insert(terminal_id.clone(), runtime);

        assert_eq!(app.window_title().as_deref(), Some("api|building"));

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }

    #[test]
    fn empty_template_disables_window_titles() {
        let mut app = test_app();
        app.configure_window_title("");

        assert_eq!(app.window_title(), None);
    }

    #[test]
    fn invalid_template_disables_window_titles() {
        let mut app = test_app();
        app.configure_window_title("{nope}");

        assert_eq!(app.window_title(), None);
    }

    #[test]
    fn unset_tokens_render_empty() {
        let mut app = test_app();
        app.configure_window_title("[{pane}]");

        assert_eq!(app.window_title().as_deref(), Some("[]"));
    }

    #[tokio::test]
    async fn focused_terminal_title_dirty_flag_tracks_osc_changes() {
        let mut app = test_app();
        app.configure_window_title("{terminal_title}");

        let pane_id = app.state.workspaces[0].tabs[0].root_pane;
        let terminal_id = app.state.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let runtime = crate::terminal::TerminalRuntime::test_with_screen_bytes(80, 24, b"");
        app.terminal_runtimes.insert(terminal_id.clone(), runtime);

        assert!(!app.take_focused_terminal_title_dirty());
        app.terminal_runtimes
            .get(&terminal_id)
            .expect("runtime")
            .test_process_pty_bytes(pane_id, b"\x1b]0;building\x07");
        assert!(app.take_focused_terminal_title_dirty());
        assert!(!app.take_focused_terminal_title_dirty());

        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }
}
