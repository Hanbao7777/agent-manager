use std::path::{Path, PathBuf};

use super::{
    classify_process_failure, redact_diagnostic, InstallFailure, InstallStage, ToolInstallResult,
    ToolInstallStatus, ToolInstallStrategy,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CleanEnvironment {
    pub path_entries: Vec<PathBuf>,
}

impl CleanEnvironment {
    pub fn with_dependency_paths(&self, node: &[PathBuf], npm: &[PathBuf]) -> Self {
        let mut path_entries = Vec::new();
        for executable in node.iter().chain(npm) {
            if let Some(parent) = executable.parent() {
                if !path_entries.iter().any(|entry| entry == parent) {
                    path_entries.push(parent.to_path_buf());
                }
            }
        }
        for entry in &self.path_entries {
            if !path_entries.iter().any(|existing| existing == entry) {
                path_entries.push(entry.clone());
            }
        }
        Self { path_entries }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ToolCommandOutput {
    fn success(version: &str) -> Self {
        Self {
            exit_code: Some(0),
            stdout: version.into(),
            stderr: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableBaseline {
    pub path: PathBuf,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyCandidates {
    node: Vec<PathBuf>,
    npm: Vec<PathBuf>,
    selected: Option<ResolvedNodeNpmPair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNodeNpmPair {
    pub node: PathBuf,
    pub npm: PathBuf,
    pub source: &'static str,
}

impl DependencyCandidates {
    pub fn new(
        node: Vec<PathBuf>,
        npm: Vec<PathBuf>,
        selected: Option<ResolvedNodeNpmPair>,
    ) -> Result<Self, &'static str> {
        if let Some(pair) = &selected {
            if pair.source.is_empty() || !node.contains(&pair.node) || !npm.contains(&pair.npm) {
                return Err("selected Node/npm pair is not coherent");
            }
        }
        Ok(Self {
            node,
            npm,
            selected,
        })
    }

    pub fn node_candidates(&self) -> &[PathBuf] {
        &self.node
    }

    pub fn npm_candidates(&self) -> &[PathBuf] {
        &self.npm
    }

    pub fn selected_pair(&self) -> Option<&ResolvedNodeNpmPair> {
        self.selected.as_ref()
    }
}

pub trait ToolRunner: Send + Sync {
    fn clean_environment(&self) -> CleanEnvironment;
    fn resolve_command(
        &self,
        name: &str,
        environment: &CleanEnvironment,
    ) -> Result<Vec<PathBuf>, InstallFailure>;
    fn run_version(
        &self,
        path: &Path,
        args: &[&str],
        environment: &CleanEnvironment,
    ) -> Result<ToolCommandOutput, InstallFailure>;

    fn resolve_dependencies(
        &self,
        _environment: &CleanEnvironment,
    ) -> Result<Option<DependencyCandidates>, InstallFailure> {
        Ok(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolVerification {
    pub result: ToolInstallResult,
    pub candidates: Vec<String>,
    pub selected_path: Option<String>,
    pub conflicts: Vec<String>,
    pub clean_environment: CleanEnvironment,
    pub dependency_candidates: DependencyCandidates,
    pub baseline_match: bool,
    pub baseline_version_match: bool,
}

pub fn verify_tool<R: ToolRunner>(
    runner: &R,
    strategy: &'static ToolInstallStrategy,
    baseline: Option<&ExecutableBaseline>,
) -> ToolInstallResult {
    verify_tool_detailed(runner, strategy, baseline).result
}

pub fn verify_tool_detailed<R: ToolRunner>(
    runner: &R,
    strategy: &'static ToolInstallStrategy,
    baseline: Option<&ExecutableBaseline>,
) -> ToolVerification {
    let environment = runner.clean_environment();
    let dependencies = if strategy.dependencies.is_empty() {
        DependencyCandidates::default()
    } else {
        match runner.resolve_dependencies(&environment) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => DependencyCandidates::default(),
            Err(error) => {
                return verification_failure(runner, strategy, environment, normalize_error(error));
            }
        }
    };
    if !strategy.dependencies.is_empty() && dependencies.selected.is_none() {
        return verification_failure(
            runner,
            strategy,
            environment,
            failure("ambiguous Node/npm installations", None),
        );
    }
    let selected_node = dependencies
        .selected_pair()
        .map(|pair| vec![pair.node.clone()])
        .unwrap_or_default();
    let selected_npm = dependencies
        .selected_pair()
        .map(|pair| vec![pair.npm.clone()])
        .unwrap_or_default();
    let environment = environment.with_dependency_paths(&selected_node, &selected_npm);
    let candidates = match runner.resolve_command(strategy.command_name, &environment) {
        Ok(candidates) => candidates,
        Err(error) => {
            return ToolVerification {
                result: result(
                    strategy,
                    ToolInstallStatus::InstalledNotRunnable,
                    None,
                    None,
                    Some(normalize_error(error)),
                ),
                candidates: Vec::new(),
                selected_path: None,
                conflicts: Vec::new(),
                clean_environment: environment,
                dependency_candidates: dependencies,
                baseline_match: false,
                baseline_version_match: false,
            }
        }
    };
    let candidate_strings: Vec<String> = candidates
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let conflicts = candidate_strings.iter().skip(1).cloned().collect();
    let selected = candidates.first().cloned();
    let result_value = if let Some(path) = selected.as_ref() {
        match runner.run_version(path, strategy.version_args, &environment) {
            Err(error) => result(
                strategy,
                ToolInstallStatus::InstalledNotRunnable,
                None,
                Some(path),
                Some(normalize_error(error)),
            ),
            Ok(output) => {
                let diagnostic = format!("{}\n{}", output.stdout, output.stderr);
                let version = parse_semver_output(&diagnostic);
                if output.exit_code == Some(0) && version.is_some() {
                    result(
                        strategy,
                        ToolInstallStatus::Succeeded,
                        version,
                        Some(path),
                        None,
                    )
                } else {
                    result(
                        strategy,
                        ToolInstallStatus::InstalledNotRunnable,
                        version,
                        Some(path),
                        Some(failure(&diagnostic, output.exit_code)),
                    )
                }
            }
        }
    } else {
        result(
            strategy,
            ToolInstallStatus::InstalledNotRunnable,
            None,
            None,
            Some(failure("command unavailable", None)),
        )
    };
    let baseline_match = baseline.is_some_and(|baseline| selected.as_ref() == Some(&baseline.path));
    let baseline_version_match = baseline
        .and_then(|baseline| baseline.version.as_deref())
        .zip(result_value.version.as_deref())
        .is_some_and(|(before, after)| before == after);
    ToolVerification {
        result: result_value,
        candidates: candidate_strings,
        selected_path: selected.map(|path| path.to_string_lossy().into_owned()),
        conflicts,
        clean_environment: environment,
        dependency_candidates: dependencies,
        baseline_match,
        baseline_version_match,
    }
}

fn verification_failure<R: ToolRunner>(
    _runner: &R,
    strategy: &'static ToolInstallStrategy,
    environment: CleanEnvironment,
    error: InstallFailure,
) -> ToolVerification {
    ToolVerification {
        result: result(
            strategy,
            ToolInstallStatus::InstalledNotRunnable,
            None,
            None,
            Some(error),
        ),
        candidates: Vec::new(),
        selected_path: None,
        conflicts: Vec::new(),
        clean_environment: environment,
        dependency_candidates: DependencyCandidates::default(),
        baseline_match: false,
        baseline_version_match: false,
    }
}

fn parse_semver_output(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let index = tokens.iter().position(|token| {
            let candidate = token.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && !matches!(c, '.' | '-' | '+')
            });
            candidate
                .strip_prefix('v')
                .unwrap_or(candidate)
                .contains('.')
        })?;
        if index + 1 != tokens.len() {
            return None;
        }
        let token = tokens[index];
        let token = token
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '.' | '-' | '+'));
        let token = token.strip_prefix('v').unwrap_or(token);
        let core_end = token.find(['-', '+']).unwrap_or(token.len());
        let (core, suffix) = token.split_at(core_end);
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part.chars().all(|c| c.is_ascii_digit())
                    || (part.len() > 1 && part.starts_with('0'))
            })
        {
            return None;
        }
        if !valid_suffix(suffix) {
            return None;
        }
        Some(token.to_string())
    })
}

fn valid_suffix(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let marker = suffix.as_bytes()[0];
    let suffix = &suffix[1..];
    if suffix.is_empty() {
        return false;
    }
    if marker == b'+' {
        return valid_identifiers(suffix, false);
    }
    let mut sections = suffix.split('+');
    let pre = sections.next().unwrap_or_default();
    let build = sections.next();
    if sections.next().is_some() {
        return false;
    }
    valid_identifiers(pre, true) && build.map_or(true, |value| valid_identifiers(value, false))
}

fn valid_identifiers(value: &str, numeric_rules: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                && (!numeric_rules
                    || identifier.len() == 1
                    || !identifier.chars().all(|c| c.is_ascii_digit())
                    || !identifier.starts_with('0'))
        })
}

fn result(
    strategy: &ToolInstallStrategy,
    status: ToolInstallStatus,
    version: Option<String>,
    path: Option<&PathBuf>,
    failure: Option<InstallFailure>,
) -> ToolInstallResult {
    ToolInstallResult {
        tool: super::tools::tool_id(strategy),
        status,
        version,
        path: path.map(|path| path.to_string_lossy().into_owned()),
        failure,
    }
}

fn failure(detail: &str, exit_code: Option<i32>) -> InstallFailure {
    classify_process_failure(InstallStage::Verifying, exit_code, "", detail)
}

fn normalize_error(error: InstallFailure) -> InstallFailure {
    InstallFailure {
        stage: InstallStage::Verifying,
        detail: error.detail.map(|detail| redact_diagnostic(&detail)),
        ..error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installer::{
        tool_strategy, InstallFailureCode, RecommendedAction, SharedDependency, ToolId,
        ToolInstallMethod,
    };
    use std::{collections::HashMap, sync::Mutex};

    struct Fake {
        paths: Vec<PathBuf>,
        outputs: HashMap<PathBuf, Result<ToolCommandOutput, InstallFailure>>,
        dependencies: Option<DependencyCandidates>,
        dependency_error: Option<InstallFailure>,
        env: CleanEnvironment,
        observed: Mutex<Vec<(Option<PathBuf>, Vec<String>, Vec<PathBuf>)>>,
    }
    impl ToolRunner for Fake {
        fn clean_environment(&self) -> CleanEnvironment {
            self.env.clone()
        }
        fn resolve_command(
            &self,
            name: &str,
            env: &CleanEnvironment,
        ) -> Result<Vec<PathBuf>, InstallFailure> {
            self.observed
                .lock()
                .unwrap()
                .push((None, vec![name.into()], env.path_entries.clone()));
            Ok(self.paths.clone())
        }
        fn resolve_dependencies(
            &self,
            environment: &CleanEnvironment,
        ) -> Result<Option<DependencyCandidates>, InstallFailure> {
            self.observed.lock().unwrap().push((
                None,
                vec!["resolve_dependencies".into()],
                environment.path_entries.clone(),
            ));
            self.dependency_error
                .clone()
                .map_or_else(|| Ok(self.dependencies.clone()), Err)
        }
        fn run_version(
            &self,
            path: &Path,
            args: &[&str],
            env: &CleanEnvironment,
        ) -> Result<ToolCommandOutput, InstallFailure> {
            self.observed.lock().unwrap().push((
                Some(path.into()),
                args.iter().map(|arg| (*arg).into()).collect(),
                env.path_entries.clone(),
            ));
            self.outputs
                .get(path)
                .cloned()
                .unwrap_or_else(|| Err(test_error("missing output")))
        }
    }
    fn test_error(detail: &str) -> InstallFailure {
        InstallFailure {
            code: InstallFailureCode::VerificationFailure,
            stage: InstallStage::Verifying,
            exit_code: None,
            retryable: false,
            requires_user_action: true,
            message_key: "test".into(),
            recommended_action: RecommendedAction::ViewDiagnostics,
            detail: Some(detail.into()),
        }
    }
    fn fake(paths: &[&str], output: Result<ToolCommandOutput, InstallFailure>) -> Fake {
        let path = PathBuf::from(paths[0]);
        Fake {
            paths: paths.iter().map(PathBuf::from).collect(),
            outputs: [(path, output)].into_iter().collect(),
            dependencies: Some(DependencyCandidates {
                node: vec![PathBuf::from("C:/node/node")],
                npm: vec![PathBuf::from("C:/node/npm")],
                selected: Some(ResolvedNodeNpmPair {
                    node: PathBuf::from("C:/node/node"),
                    npm: PathBuf::from("C:/node/npm"),
                    source: "test",
                }),
            }),
            dependency_error: None,
            env: CleanEnvironment {
                path_entries: vec![PathBuf::from("C:/clean/bin")],
            },
            observed: Mutex::new(Vec::new()),
        }
    }

    #[test]
    fn canonical_table_and_fallbacks() {
        for (name, id) in [
            ("claude", ToolId::Claude),
            ("codex", ToolId::Codex),
            ("gemini", ToolId::Gemini),
            ("opencode", ToolId::Opencode),
            ("openclaw", ToolId::Openclaw),
            ("hermes", ToolId::Hermes),
        ] {
            assert_eq!(tool_strategy(name).unwrap().tool, id);
        }
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
    }
    #[test]
    fn explicit_clean_environment_is_observed() {
        let fake = fake(
            &["C:/a.cmd"],
            Ok(ToolCommandOutput {
                exit_code: Some(0),
                stdout: "1.2.3".into(),
                stderr: String::new(),
            }),
        );
        verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        assert_eq!(
            fake.observed.lock().unwrap().as_slice(),
            &[
                (
                    None,
                    vec!["resolve_dependencies".into()],
                    vec![PathBuf::from("C:/clean/bin")]
                ),
                (
                    None,
                    vec!["codex".into()],
                    vec![PathBuf::from("C:/node"), PathBuf::from("C:/clean/bin")]
                ),
                (
                    Some(PathBuf::from("C:/a.cmd")),
                    vec!["--version".into()],
                    vec![PathBuf::from("C:/node"), PathBuf::from("C:/clean/bin")]
                )
            ]
        );
    }

    #[test]
    fn official_primary_ignores_unrelated_dependency_resolution_error() {
        let mut fake = fake(&["C:/claude"], Ok(ToolCommandOutput::success("1.2.3")));
        fake.dependency_error = Some(test_error("ambiguous unrelated node installs"));
        let result = verify_tool(&fake, tool_strategy(ToolId::Claude).unwrap(), None);
        assert_eq!(result.status, ToolInstallStatus::Succeeded);
    }

    #[test]
    fn npm_dependency_resolver_error_is_normalized_and_redacted() {
        let mut fake = fake(&["C:/codex"], Ok(ToolCommandOutput::success("1.2.3")));
        fake.dependencies = None;
        fake.dependency_error = Some(test_error("API_KEY=secret"));
        let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        let error = result.failure.unwrap();
        assert_eq!(error.code, InstallFailureCode::VerificationFailure);
        assert_eq!(error.stage, InstallStage::Verifying);
        assert!(!error.detail.unwrap().contains("secret"));
    }

    #[test]
    fn verification_uses_process_failure_classification() {
        let fake = fake(
            &["C:/codex"],
            Ok(ToolCommandOutput {
                exit_code: Some(127),
                stdout: String::new(),
                stderr: "npm: command not found".into(),
            }),
        );

        let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        let failure = result.failure.unwrap();
        assert_eq!(failure.code, InstallFailureCode::DependencyMissing);
        assert_eq!(
            failure.recommended_action,
            RecommendedAction::RepairDependencies
        );
    }

    #[test]
    fn official_strategies_resolve_declared_dependencies() {
        let fake = fake(&["C:/tool"], Ok(ToolCommandOutput::success("1.2.3")));
        let strategy = Box::leak(Box::new(ToolInstallStrategy {
            tool: ToolId::Claude,
            display_name: "Test tool",
            command_name: "tool",
            npm_package: None,
            dependencies: &[SharedDependency::Node, SharedDependency::Npm],
            version_args: &["--version"],
            method: ToolInstallMethod::OfficialInstaller,
            fallback_npm_package: None,
            fallback_dependencies: &[],
            fallback_method: None,
        }));

        assert_eq!(
            verify_tool(&fake, strategy, None).status,
            ToolInstallStatus::Succeeded
        );
        assert_eq!(
            fake.observed.lock().unwrap()[0].1,
            vec!["resolve_dependencies".to_string()]
        );
    }

    #[test]
    fn selected_dependency_pair_avoids_ambiguous_candidates_and_paths() {
        let mut fake = fake(&["C:/tool"], Ok(ToolCommandOutput::success("1.2.3")));
        fake.dependencies = Some(
            DependencyCandidates::new(
                vec!["C:/selected/node/node".into(), "C:/other/node/node".into()],
                vec!["C:/selected/npm/npm".into(), "C:/other/npm/npm".into()],
                Some(ResolvedNodeNpmPair {
                    node: "C:/selected/node/node".into(),
                    npm: "C:/selected/npm/npm".into(),
                    source: "test",
                }),
            )
            .unwrap(),
        );

        let verification = verify_tool_detailed(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        assert_eq!(verification.result.status, ToolInstallStatus::Succeeded);
        assert_eq!(
            verification.clean_environment.path_entries,
            vec![
                PathBuf::from("C:/selected/node"),
                PathBuf::from("C:/selected/npm"),
                PathBuf::from("C:/clean/bin"),
            ]
        );
        assert!(!verification
            .clean_environment
            .path_entries
            .contains(&PathBuf::from("C:/other/node")));

        fake.dependencies = Some(
            DependencyCandidates::new(
                vec!["C:/selected/node/node".into(), "C:/other/node/node".into()],
                vec!["C:/selected/npm/npm".into(), "C:/other/npm/npm".into()],
                None,
            )
            .unwrap(),
        );
        assert_eq!(
            verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None).status,
            ToolInstallStatus::InstalledNotRunnable
        );
    }

    #[test]
    fn normalize_error_preserves_preclassified_code_action_and_redacts_detail() {
        let mut fake = fake(&["C:/codex"], Ok(ToolCommandOutput::success("1.2.3")));
        fake.dependencies = None;
        fake.dependency_error = Some(InstallFailure {
            code: InstallFailureCode::PermissionDenied,
            stage: InstallStage::InstallingTools,
            exit_code: Some(13),
            retryable: false,
            requires_user_action: true,
            message_key: "installer.failure.permission_denied".into(),
            recommended_action: RecommendedAction::GrantPermission,
            detail: Some("API_KEY=secret permission denied".into()),
        });

        let failure = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None)
            .failure
            .unwrap();
        assert_eq!(failure.code, InstallFailureCode::PermissionDenied);
        assert_eq!(
            failure.recommended_action,
            RecommendedAction::GrantPermission
        );
        assert_eq!(failure.stage, InstallStage::Verifying);
        assert!(!failure.detail.unwrap().contains("secret"));
    }

    #[test]
    fn diagnostics_redact_assignments_bearer_urls_queries_and_proxy_credentials() {
        let diagnostic = "API_KEY=secret Bearer abcdef https://user:pass@example.test/?token=secret HTTP_PROXY=http://name:pwd@example.test";
        let redacted = redact_diagnostic(diagnostic);
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("abcdef"));
        assert!(!redacted.contains("user:pass"));
        assert!(!redacted.contains("name:pwd"));
    }
    #[test]
    fn every_unavailable_error_nonzero_and_no_semver_is_not_runnable() {
        let cases = [
            fake(&["C:/a.cmd"], Err(test_error("TOKEN=secret"))),
            fake(
                &["C:/a.cmd"],
                Ok(ToolCommandOutput {
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr: "TOKEN=secret".into(),
                }),
            ),
            fake(
                &["C:/a.cmd"],
                Ok(ToolCommandOutput {
                    exit_code: Some(0),
                    stdout: "not semver".into(),
                    stderr: String::new(),
                }),
            ),
        ];
        for fake in cases {
            let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
            assert_eq!(result.status, ToolInstallStatus::InstalledNotRunnable);
            assert_eq!(
                result.failure.as_ref().unwrap().code,
                InstallFailureCode::VerificationFailure
            );
            assert!(!result.failure.unwrap().detail.unwrap().contains("secret"));
        }
        let missing = Fake {
            paths: Vec::new(),
            outputs: HashMap::new(),
            dependencies: None,
            dependency_error: None,
            env: CleanEnvironment::default(),
            observed: Mutex::new(Vec::new()),
        };
        assert_eq!(
            verify_tool(&missing, tool_strategy(ToolId::Codex).unwrap(), None).status,
            ToolInstallStatus::InstalledNotRunnable
        );
    }
    #[test]
    fn strict_semver_and_executable_baseline_are_preserved() {
        let fake = fake(
            &["C:/a.cmd"],
            Ok(ToolCommandOutput {
                exit_code: Some(0),
                stdout: "tool v1.2.3-beta+build".into(),
                stderr: String::new(),
            }),
        );
        let baseline = ExecutableBaseline {
            path: PathBuf::from("C:/a.cmd"),
            version: Some("1.2.3-beta+build".into()),
        };
        let verification = verify_tool_detailed(
            &fake,
            tool_strategy(ToolId::Codex).unwrap(),
            Some(&baseline),
        );
        assert_eq!(
            verification.result.version.as_deref(),
            Some("1.2.3-beta+build")
        );
        assert!(verification.baseline_match);
        assert!(verification.baseline_version_match);
    }
    #[test]
    fn per_path_outputs_preserve_nonfatal_conflicts() {
        let mut fake = fake(
            &["C:/one.cmd", "C:/two.cmd"],
            Ok(ToolCommandOutput {
                exit_code: Some(0),
                stdout: "1.2.3".into(),
                stderr: String::new(),
            }),
        );
        fake.outputs.insert(
            PathBuf::from("C:/two.cmd"),
            Ok(ToolCommandOutput {
                exit_code: Some(0),
                stdout: "9.9.9".into(),
                stderr: String::new(),
            }),
        );
        let verification = verify_tool_detailed(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        assert_eq!(verification.result.status, ToolInstallStatus::Succeeded);
        assert_eq!(verification.conflicts, vec!["C:/two.cmd"]);
    }

    #[test]
    fn gui_path_absence_uses_dynamic_homebrew_and_nvm_candidates() {
        for (source, node, npm) in [
            (
                "homebrew_arm",
                "/opt/homebrew/Cellar/node/24/bin/node",
                "/opt/homebrew/Cellar/node/24/bin/npm",
            ),
            (
                "homebrew_intel",
                "/usr/local/Cellar/node/24/bin/node",
                "/usr/local/Cellar/node/24/bin/npm",
            ),
            ("official_pkg", "/usr/local/bin/node", "/usr/local/bin/npm"),
            (
                "nvm",
                "/Users/test/.nvm/versions/node/v24/bin/node",
                "/Users/test/.nvm/versions/node/v24/bin/npm",
            ),
            (
                "fnm",
                "/Users/test/.fnm/node-versions/v24/installation/bin/node",
                "/Users/test/.fnm/node-versions/v24/installation/bin/npm",
            ),
            (
                "volta",
                "/Users/test/.volta/bin/node",
                "/Users/test/.volta/bin/npm",
            ),
            (
                "mise",
                "/Users/test/.local/share/mise/installs/node/24/bin/node",
                "/Users/test/.local/share/mise/installs/node/24/bin/npm",
            ),
        ] {
            let mut fake = fake(&["/tools/codex"], Ok(ToolCommandOutput::success("1.2.3")));
            fake.dependencies = Some(DependencyCandidates {
                node: vec![node.into()],
                npm: vec![npm.into()],
                selected: Some(ResolvedNodeNpmPair {
                    node: node.into(),
                    npm: npm.into(),
                    source,
                }),
            });
            let verification =
                verify_tool_detailed(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
            assert_eq!(verification.result.status, ToolInstallStatus::Succeeded);
            assert_eq!(
                verification
                    .dependency_candidates
                    .selected
                    .as_ref()
                    .unwrap()
                    .source,
                source
            );
            assert_eq!(
                verification.clean_environment.path_entries[0],
                PathBuf::from(node).parent().unwrap()
            );
            assert!(verification
                .clean_environment
                .path_entries
                .contains(&PathBuf::from(npm).parent().unwrap().to_path_buf()));
            assert_eq!(
                verification.clean_environment.path_entries.last(),
                Some(&PathBuf::from("C:/clean/bin"))
            );
            let observed = fake.observed.lock().unwrap();
            assert_eq!(
                observed[1],
                (
                    None,
                    vec!["codex".into()],
                    verification.clean_environment.path_entries.clone()
                )
            );
            assert_eq!(
                observed[2],
                (
                    Some(PathBuf::from("/tools/codex")),
                    vec!["--version".into()],
                    verification.clean_environment.path_entries.clone()
                )
            );
        }
    }

    #[test]
    fn ambiguous_dependency_candidates_are_not_selected_silently() {
        let mut fake = fake(&["C:/codex.cmd"], Ok(ToolCommandOutput::success("1.2.3")));
        fake.dependencies = Some(DependencyCandidates {
            node: vec!["a/node".into(), "b/node".into()],
            npm: vec!["a/npm".into()],
            selected: None,
        });
        let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        assert_eq!(result.status, ToolInstallStatus::InstalledNotRunnable);
        assert_eq!(
            result.failure.unwrap().code,
            InstallFailureCode::VerificationFailure
        );
    }

    #[test]
    fn broken_default_does_not_choose_healthy_alternate() {
        let mut fake = fake(
            &["C:/broken.cmd", "C:/healthy.cmd"],
            Ok(ToolCommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "broken".into(),
            }),
        );
        fake.outputs.insert(
            PathBuf::from("C:/healthy.cmd"),
            Ok(ToolCommandOutput::success("2.3.4")),
        );
        let verification = verify_tool_detailed(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        assert_eq!(
            verification.result.status,
            ToolInstallStatus::InstalledNotRunnable
        );
        assert_eq!(verification.result.path.as_deref(), Some("C:/broken.cmd"));
        assert_eq!(verification.conflicts, vec!["C:/healthy.cmd"]);
    }

    #[test]
    fn invalid_semver_variants_are_not_accepted() {
        for value in [
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "123",
            "1.2.3garbage",
            "1.2.3+a+b",
            "1.2.3-alpha..1",
            "1.2.3-01",
            "v1.2.3+",
            "1.2.3-",
            "1.2.3+build bad",
        ] {
            let fake = fake(&["C:/a.cmd"], Ok(ToolCommandOutput::success(value)));
            let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
            assert_eq!(
                result.status,
                ToolInstallStatus::InstalledNotRunnable,
                "{value}"
            );
        }
    }

    #[test]
    fn valid_bare_prefixed_and_build_versions_are_accepted() {
        for value in ["1.2.3", "tool v1.2.3", "1.2.3-alpha.1+build.7", "1.2.3+001"] {
            let fake = fake(&["C:/a.cmd"], Ok(ToolCommandOutput::success(value)));
            assert_eq!(
                verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None).status,
                ToolInstallStatus::Succeeded,
                "{value}"
            );
        }
    }

    #[test]
    fn validated_pair_accessors_expose_reusable_resolution_only() {
        let pair = ResolvedNodeNpmPair {
            node: PathBuf::from("/runtime/node/bin/node"),
            npm: PathBuf::from("/runtime/npm/bin/npm"),
            source: "official_pkg",
        };
        let candidates = DependencyCandidates::new(
            vec![pair.node.clone()],
            vec![pair.npm.clone()],
            Some(pair.clone()),
        )
        .unwrap();
        assert_eq!(
            candidates.node_candidates(),
            &[PathBuf::from("/runtime/node/bin/node")]
        );
        assert_eq!(
            candidates.npm_candidates(),
            &[PathBuf::from("/runtime/npm/bin/npm")]
        );
        assert_eq!(candidates.selected_pair(), Some(&pair));
        assert!(DependencyCandidates::new(
            vec![PathBuf::from("a/node")],
            vec![PathBuf::from("b/npm")],
            Some(pair)
        )
        .is_err());
    }

    #[test]
    fn nonzero_exit_code_is_preserved_and_failure_is_redacted() {
        let fake = fake(
            &["C:/a.cmd"],
            Ok(ToolCommandOutput {
                exit_code: Some(17),
                stdout: "API_KEY=secret".into(),
                stderr: "Bearer abcdef".into(),
            }),
        );
        let result = verify_tool(&fake, tool_strategy(ToolId::Codex).unwrap(), None);
        let failure = result.failure.unwrap();
        assert_eq!(failure.exit_code, Some(17));
        assert!(!failure.detail.unwrap().contains("secret"));
    }

    #[test]
    fn baseline_and_repeated_verification_are_deterministic() {
        let fake = fake(&["C:/a.cmd"], Ok(ToolCommandOutput::success("1.2.3")));
        let baseline = ExecutableBaseline {
            path: PathBuf::from("C:/a.cmd"),
            version: Some("1.2.3".into()),
        };
        let first = verify_tool_detailed(
            &fake,
            tool_strategy(ToolId::Codex).unwrap(),
            Some(&baseline),
        );
        let second = verify_tool_detailed(
            &fake,
            tool_strategy(ToolId::Codex).unwrap(),
            Some(&baseline),
        );
        assert_eq!(first, second);
        assert!(first.baseline_match);
        assert!(first.baseline_version_match);
    }
}
