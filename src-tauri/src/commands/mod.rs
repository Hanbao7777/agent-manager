#![allow(non_snake_case)]

mod misc;

pub use misc::*;

use std::sync::Arc;
use tokio::sync::RwLock;

pub struct CodexOAuthState(
    pub Arc<RwLock<crate::proxy::providers::codex_oauth_auth::CodexOAuthManager>>,
);

pub struct CopilotAuthState(
    pub Arc<RwLock<crate::proxy::providers::copilot_auth::CopilotAuthManager>>,
);
