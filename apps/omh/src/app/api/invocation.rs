use crate::app::view_state::ClientViewState;

/// Originating view/client and focus policy for one API operation.
///
/// Ambient invocations (`view = None`) apply shared AppState focus. View-scoped
/// invocations keep client-local effects on the provided `ClientViewState`.
pub(crate) struct ApiInvocationContext<'a> {
    view: Option<&'a mut ClientViewState>,
}

impl<'a> ApiInvocationContext<'a> {
    pub(crate) fn ambient() -> Self {
        Self { view: None }
    }

    pub(crate) fn for_view(view: &'a mut ClientViewState) -> Self {
        Self { view: Some(view) }
    }

    pub(crate) fn is_client_local(&self) -> bool {
        self.view.is_some()
    }

    pub(crate) fn client_view_id(&self) -> Option<u64> {
        self.view.as_ref().map(|view| view.id())
    }

    pub(crate) fn view(&self) -> Option<&ClientViewState> {
        self.view.as_deref()
    }

    pub(crate) fn view_mut(&mut self) -> Option<&mut ClientViewState> {
        self.view.as_deref_mut()
    }
}
