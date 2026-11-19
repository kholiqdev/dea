//! What every command can reach.

use std::path::Path;

use dea_engine::store::Store;

pub struct AppState {
    pub store: Store,
}

impl AppState {
    pub fn new(app_data: &Path) -> Self {
        Self {
            store: Store::new(app_data),
        }
    }
}
