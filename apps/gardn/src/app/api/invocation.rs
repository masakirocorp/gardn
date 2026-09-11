use crate::app::view_state::ClientViewState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApiInvocationOrigin {
    Ambient,
    ClientLocal,
}

pub(crate) struct ApiInvocationContext<'a> {
    origin: ApiInvocationOrigin,
    view: &'a mut ClientViewState,
}

impl<'a> ApiInvocationContext<'a> {
    pub(crate) fn with_origin(origin: ApiInvocationOrigin, view: &'a mut ClientViewState) -> Self {
        Self { origin, view }
    }

    pub(crate) fn ambient(view: &'a mut ClientViewState) -> Self {
        Self {
            origin: ApiInvocationOrigin::Ambient,
            view,
        }
    }

    pub(crate) fn is_client_local(&self) -> bool {
        self.origin == ApiInvocationOrigin::ClientLocal
    }

    pub(crate) fn client_view_id(&self) -> u64 {
        self.view.id()
    }

    pub(crate) fn view(&self) -> &ClientViewState {
        self.view
    }

    pub(crate) fn view_mut(&mut self) -> &mut ClientViewState {
        self.view
    }
}
