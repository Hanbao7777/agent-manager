use super::ToolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedDependency {
    Node,
    Npm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolInstallMethod {
    OfficialInstaller,
    Npm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolInstallStrategy {
    pub tool: ToolId,
    pub display_name: &'static str,
    pub command_name: &'static str,
    pub npm_package: Option<&'static str>,
    pub dependencies: &'static [SharedDependency],
    pub version_args: &'static [&'static str],
    pub method: ToolInstallMethod,
    pub fallback_npm_package: Option<&'static str>,
    pub fallback_dependencies: &'static [SharedDependency],
    pub fallback_method: Option<ToolInstallMethod>,
}

const NODE_NPM_DEPENDENCIES: &[SharedDependency] = &[SharedDependency::Node, SharedDependency::Npm];
const FALLBACK_NODE_NPM: &[SharedDependency] = &[SharedDependency::Node, SharedDependency::Npm];
const VERSION_ARGS: &[&str] = &["--version"];

const STRATEGIES: &[ToolInstallStrategy] = &[
    ToolInstallStrategy {
        tool: ToolId::Claude,
        display_name: "Claude Code",
        command_name: "claude",
        npm_package: None,
        dependencies: &[],
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::OfficialInstaller,
        fallback_npm_package: Some("@anthropic-ai/claude-code"),
        fallback_dependencies: FALLBACK_NODE_NPM,
        fallback_method: Some(ToolInstallMethod::Npm),
    },
    ToolInstallStrategy {
        tool: ToolId::Codex,
        display_name: "Codex",
        command_name: "codex",
        npm_package: Some("@openai/codex"),
        dependencies: NODE_NPM_DEPENDENCIES,
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::Npm,
        fallback_npm_package: None,
        fallback_dependencies: &[],
        fallback_method: None,
    },
    ToolInstallStrategy {
        tool: ToolId::Gemini,
        display_name: "Gemini CLI",
        command_name: "gemini",
        npm_package: Some("@google/gemini-cli"),
        dependencies: NODE_NPM_DEPENDENCIES,
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::Npm,
        fallback_npm_package: None,
        fallback_dependencies: &[],
        fallback_method: None,
    },
    ToolInstallStrategy {
        tool: ToolId::Opencode,
        display_name: "OpenCode",
        command_name: "opencode",
        npm_package: None,
        dependencies: &[],
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::OfficialInstaller,
        fallback_npm_package: Some("opencode-ai"),
        fallback_dependencies: FALLBACK_NODE_NPM,
        fallback_method: Some(ToolInstallMethod::Npm),
    },
    ToolInstallStrategy {
        tool: ToolId::Openclaw,
        display_name: "OpenClaw",
        command_name: "openclaw",
        npm_package: Some("openclaw"),
        dependencies: NODE_NPM_DEPENDENCIES,
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::Npm,
        fallback_npm_package: None,
        fallback_dependencies: &[],
        fallback_method: None,
    },
    ToolInstallStrategy {
        tool: ToolId::Hermes,
        display_name: "Hermes",
        command_name: "hermes",
        npm_package: None,
        dependencies: &[],
        version_args: VERSION_ARGS,
        method: ToolInstallMethod::OfficialInstaller,
        fallback_npm_package: None,
        fallback_dependencies: &[],
        fallback_method: None,
    },
];

pub trait ToolKey {
    fn tool_id(self) -> Option<ToolId>;
}

impl ToolKey for ToolId {
    fn tool_id(self) -> Option<ToolId> {
        Some(self)
    }
}

impl ToolKey for &str {
    fn tool_id(self) -> Option<ToolId> {
        match self {
            "claude" => Some(ToolId::Claude),
            "codex" => Some(ToolId::Codex),
            "gemini" => Some(ToolId::Gemini),
            "opencode" => Some(ToolId::Opencode),
            "openclaw" => Some(ToolId::Openclaw),
            "hermes" => Some(ToolId::Hermes),
            _ => None,
        }
    }
}

pub fn tool_strategy<K: ToolKey>(tool: K) -> Option<&'static ToolInstallStrategy> {
    let tool = tool.tool_id()?;
    STRATEGIES.iter().find(|strategy| strategy.tool == tool)
}

pub fn tool_id(strategy: &ToolInstallStrategy) -> ToolId {
    strategy.tool
}

#[cfg(test)]
mod tests {
    use super::{
        tool_strategy, SharedDependency, ToolId, ToolInstallMethod, ToolInstallStrategy, ToolKey,
    };

    #[test]
    fn codex_strategy_declares_node_and_npm() {
        let strategy = tool_strategy("codex").unwrap();
        assert_eq!(strategy.command_name, "codex");
        assert_eq!(strategy.npm_package, Some("@openai/codex"));
        assert_eq!(
            strategy.dependencies,
            &[SharedDependency::Node, SharedDependency::Npm]
        );
        assert_eq!(strategy.method, ToolInstallMethod::Npm);
    }

    #[test]
    fn all_six_supported_tools_have_strategies() {
        for tool in [
            "claude", "codex", "gemini", "opencode", "openclaw", "hermes",
        ] {
            let strategy = tool_strategy(tool).unwrap();
            assert_eq!(strategy.tool, tool.tool_id().unwrap());
            assert_eq!(strategy.version_args, &["--version"]);
            assert!(!strategy.command_name.is_empty());
        }
        assert!(tool_strategy("unknown").is_none());
        assert_eq!(
            tool_strategy(ToolId::Claude).unwrap().fallback_npm_package,
            Some("@anthropic-ai/claude-code")
        );
        assert_eq!(
            tool_strategy(ToolId::Opencode)
                .unwrap()
                .fallback_npm_package,
            Some("opencode-ai")
        );
        assert_eq!(
            tool_strategy(ToolId::Claude).unwrap().fallback_method,
            Some(ToolInstallMethod::Npm)
        );
        assert_eq!(
            tool_strategy(ToolId::Opencode).unwrap().fallback_method,
            Some(ToolInstallMethod::Npm)
        );
        for id in [
            ToolId::Codex,
            ToolId::Gemini,
            ToolId::Openclaw,
            ToolId::Hermes,
        ] {
            assert_eq!(tool_strategy(id).unwrap().fallback_method, None);
        }
        for id in [ToolId::Claude, ToolId::Opencode] {
            let strategy = tool_strategy(id).unwrap();
            assert!(strategy.dependencies.is_empty());
            assert_eq!(
                strategy.fallback_dependencies,
                &[SharedDependency::Node, SharedDependency::Npm]
            );
        }
    }

    #[test]
    fn exact_six_row_metadata_includes_every_field() {
        let rows = [
            (
                ToolId::Claude,
                "Claude Code",
                "claude",
                None,
                &[][..],
                ToolInstallMethod::OfficialInstaller,
                Some("@anthropic-ai/claude-code"),
                &[SharedDependency::Node, SharedDependency::Npm][..],
                Some(ToolInstallMethod::Npm),
            ),
            (
                ToolId::Codex,
                "Codex",
                "codex",
                Some("@openai/codex"),
                &[SharedDependency::Node, SharedDependency::Npm][..],
                ToolInstallMethod::Npm,
                None,
                &[][..],
                None,
            ),
            (
                ToolId::Gemini,
                "Gemini CLI",
                "gemini",
                Some("@google/gemini-cli"),
                &[SharedDependency::Node, SharedDependency::Npm][..],
                ToolInstallMethod::Npm,
                None,
                &[][..],
                None,
            ),
            (
                ToolId::Opencode,
                "OpenCode",
                "opencode",
                None,
                &[][..],
                ToolInstallMethod::OfficialInstaller,
                Some("opencode-ai"),
                &[SharedDependency::Node, SharedDependency::Npm][..],
                Some(ToolInstallMethod::Npm),
            ),
            (
                ToolId::Openclaw,
                "OpenClaw",
                "openclaw",
                Some("openclaw"),
                &[SharedDependency::Node, SharedDependency::Npm][..],
                ToolInstallMethod::Npm,
                None,
                &[][..],
                None,
            ),
            (
                ToolId::Hermes,
                "Hermes",
                "hermes",
                None,
                &[][..],
                ToolInstallMethod::OfficialInstaller,
                None,
                &[][..],
                None,
            ),
        ];
        for (
            id,
            display,
            command,
            package,
            deps,
            method,
            fallback,
            fallback_deps,
            fallback_method,
        ) in rows
        {
            let actual = tool_strategy(id).unwrap();
            let expected = ToolInstallStrategy {
                tool: id,
                display_name: display,
                command_name: command,
                npm_package: package,
                dependencies: deps,
                version_args: &["--version"],
                method,
                fallback_npm_package: fallback,
                fallback_dependencies: fallback_deps,
                fallback_method,
            };
            assert_eq!(*actual, expected);
        }
    }
}
