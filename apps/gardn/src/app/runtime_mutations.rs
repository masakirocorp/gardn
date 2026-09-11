use crate::api::schema::Method;

use super::App;

impl App {
    /// Dispatch a mutation through the same API authority used by socket clients.
    pub(crate) fn dispatch_runtime_mutation(&mut self, id: &'static str, method: Method) -> String {
        self.dispatch_api_request(id, method)
    }
}
