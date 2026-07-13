pub mod failure;
mod model;
pub mod platform;
mod policy;
pub mod probe;
pub mod repair;
pub mod tools;
pub mod verifier;

pub use failure::{classify_process_failure, redact_diagnostic};
pub use model::*;
pub use platform::{
    cleanup_task_temp, download_and_verify_node, install_node, install_node_release,
    refresh_environment, resolve_node_release, selected_environment, NodeRelease, PlatformAdapter,
    TaskTempGuard,
};
pub use policy::{node_policy, NodePolicy};
pub use probe::{CommandProbe, ProbeConfig, ProbeRunner, SystemProbe};
pub use repair::build_repair_plan;
pub use tools::{tool_strategy, SharedDependency, ToolInstallMethod, ToolInstallStrategy, ToolKey};
pub use verifier::{
    verify_tool, verify_tool_detailed, CleanEnvironment, DependencyCandidates, ExecutableBaseline,
    ResolvedNodeNpmPair, ToolCommandOutput, ToolRunner, ToolVerification,
};
