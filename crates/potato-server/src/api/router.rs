use axum::{
    Router,
    routing::{get, post},
};

use super::endpoints;
use crate::app_manager::AppManager;

pub fn management_app(manager: AppManager) -> Router<()> {
    Router::new()
        .route("/activate", post(endpoints::activate::handler))
        .route("/apps", get(endpoints::list_apps::handler))
        .with_state(manager)
}
