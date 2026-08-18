use super::App;

fn is_system_theme(name: &str) -> bool {
    name.eq_ignore_ascii_case("system")
}

impl App {
    pub(super) fn query_host_terminal_theme(&self) {
        #[cfg(test)]
        self.host_terminal_theme_query_count
            .set(self.host_terminal_theme_query_count.get() + 1);
        use std::io::Write;

        let _ = std::io::stdout()
            .write_all(crate::terminal_theme::HOST_COLOR_QUERY_SEQUENCE.as_bytes());
        let _ = std::io::stdout().flush();
    }

    pub(crate) async fn refresh_host_terminal_theme_for(&mut self, timeout: std::time::Duration) {
        self.query_host_terminal_theme();

        let deadline = std::time::Instant::now() + timeout;
        let mut idle_deadline: Option<std::time::Instant> = None;

        loop {
            if host_terminal_theme_complete(self.state.host_terminal_theme) {
                break;
            }

            let now = std::time::Instant::now();
            if now >= deadline || idle_deadline.is_some_and(|idle| now >= idle) {
                break;
            }

            let wait_until = idle_deadline.unwrap_or(deadline).min(deadline);
            let Some(rx) = self.input_rx.as_mut() else {
                break;
            };

            match tokio::time::timeout_at(tokio::time::Instant::from_std(wait_until), rx.recv())
                .await
            {
                Ok(Some(event)) => {
                    if self.handle_host_terminal_theme_event(&event) {
                        idle_deadline =
                            Some(std::time::Instant::now() + std::time::Duration::from_millis(80));
                    }
                }
                Ok(None) => {
                    self.input_rx = None;
                    break;
                }
                Err(_) => break,
            }
        }
    }

    fn handle_host_terminal_theme_event(
        &mut self,
        event: &crate::raw_input::RawInputEvent,
    ) -> bool {
        match event {
            crate::raw_input::RawInputEvent::HostDefaultColor { kind, color } => {
                self.update_host_terminal_theme(*kind, *color)
            }
            crate::raw_input::RawInputEvent::HostPaletteColor { index, color } => {
                self.update_host_terminal_palette_color(*index, *color)
            }
            crate::raw_input::RawInputEvent::HostCursorColor { color } => {
                self.update_host_terminal_cursor_color(*color)
            }
            _ => false,
        }
    }

    pub(super) fn update_host_terminal_theme(
        &mut self,
        kind: crate::terminal_theme::DefaultColorKind,
        color: crate::terminal_theme::RgbColor,
    ) -> bool {
        let next_theme = self.state.host_terminal_theme.with_color(kind, color);
        self.set_host_terminal_theme(next_theme)
    }

    pub(super) fn update_host_terminal_palette_color(
        &mut self,
        index: u8,
        color: crate::terminal_theme::RgbColor,
    ) -> bool {
        let next_theme = self
            .state
            .host_terminal_theme
            .with_palette_color(index, color);
        self.set_host_terminal_theme(next_theme)
    }

    pub(super) fn update_host_terminal_cursor_color(
        &mut self,
        color: crate::terminal_theme::RgbColor,
    ) -> bool {
        let next_theme = self.state.host_terminal_theme.with_cursor_color(color);
        self.set_host_terminal_theme(next_theme)
    }

    pub(crate) fn set_host_terminal_theme(
        &mut self,
        theme: crate::terminal_theme::TerminalTheme,
    ) -> bool {
        if theme.is_empty() || theme == self.state.host_terminal_theme {
            return false;
        }
        self.state.host_terminal_theme = theme;
        if self.state.global_theme_mode == crate::config::ThemeMode::System
            || is_system_theme(&self.state.theme_name)
            || is_system_theme(&self.state.global_light_theme_name)
            || is_system_theme(&self.state.global_dark_theme_name)
        {
            self.state.refresh_global_palette();
            self.state.apply_effective_theme();
        }
        self.apply_host_terminal_theme_to_panes();
        true
    }

    fn apply_host_terminal_theme_to_panes(&self) {
        if self.state.host_terminal_theme.is_empty() {
            return;
        }

        let appearance = self.state.host_terminal_theme.appearance();
        for runtime in self.terminal_runtimes.values() {
            runtime.apply_host_terminal_theme(self.state.host_terminal_theme);
            runtime.apply_host_terminal_appearance(appearance);
        }

        self.render_dirty.request_generic();
        self.render_notify.notify_one();
    }
}
fn host_terminal_theme_complete(theme: crate::terminal_theme::TerminalTheme) -> bool {
    theme.foreground.is_some()
        && theme.background.is_some()
        && theme.palette.iter().all(Option::is_some)
}
