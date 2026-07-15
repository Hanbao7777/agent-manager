use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSource {
    ProcessPath,
    RefreshedEnvironment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPairPaths {
    pub node: PathBuf,
    pub npm: PathBuf,
    pub source: PairSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PairResolutionFailure {
    Missing,
    Multiple,
    Incoherent,
}

pub fn resolve_pair_with_refresh<F>(
    process_path: &[PathBuf],
    refresh: F,
) -> Result<ResolvedPairPaths, String>
where
    F: FnOnce() -> Result<Vec<PathBuf>, String>,
{
    match resolve_pair(process_path, PairSource::ProcessPath) {
        Ok(pair) => Ok(pair),
        Err(PairResolutionFailure::Missing) => {
            let refreshed = refresh()?;
            resolve_pair(&refreshed, PairSource::RefreshedEnvironment).map_err(pair_failure_message)
        }
        Err(failure) => Err(pair_failure_message(failure)),
    }
}

fn resolve_pair(
    entries: &[PathBuf],
    source: PairSource,
) -> Result<ResolvedPairPaths, PairResolutionFailure> {
    let nodes = resolve_commands("node", entries);
    let npms = resolve_commands("npm", entries);
    if nodes.is_empty() && npms.is_empty() {
        return Err(PairResolutionFailure::Missing);
    }
    if nodes.len() != 1 || npms.len() != 1 {
        return Err(PairResolutionFailure::Multiple);
    }
    let node = nodes.into_iter().next().expect("one node path");
    let npm = npms.into_iter().next().expect("one npm path");
    if node.parent() != npm.parent() {
        return Err(PairResolutionFailure::Incoherent);
    }
    Ok(ResolvedPairPaths { node, npm, source })
}

fn resolve_commands(name: &str, entries: &[PathBuf]) -> Vec<PathBuf> {
    let mut commands = entries
        .iter()
        .filter_map(|entry| {
            command_candidates(entry, name)
                .into_iter()
                .find(|candidate| candidate.is_file())
        })
        .collect::<Vec<_>>();
    commands.sort();
    commands.dedup();
    commands
}

fn command_candidates(entry: &Path, name: &str) -> [PathBuf; 3] {
    #[cfg(target_os = "windows")]
    {
        [
            entry.join(format!("{name}.exe")),
            entry.join(format!("{name}.cmd")),
            entry.join(name),
        ]
    }
    #[cfg(not(target_os = "windows"))]
    {
        [
            entry.join(name),
            entry.join(format!("{name}.exe")),
            entry.join(format!("{name}.cmd")),
        ]
    }
}

fn pair_failure_message(failure: PairResolutionFailure) -> String {
    match failure {
        PairResolutionFailure::Missing | PairResolutionFailure::Multiple => {
            "a single coherent Node.js/npm installation is required".into()
        }
        PairResolutionFailure::Incoherent => {
            "resolved Node.js and npm do not share an installation directory".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refreshes_environment_when_process_path_has_no_node() {
        let root =
            std::env::temp_dir().join(format!("agent-manager-node-pair-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("node"), b"").unwrap();
        std::fs::write(root.join("npm"), b"").unwrap();

        let pair = resolve_pair_with_refresh(&[], || Ok::<_, String>(vec![root.clone()])).unwrap();

        assert_eq!(pair.node, root.join("node"));
        assert_eq!(pair.npm, root.join("npm"));
        assert_eq!(pair.source, PairSource::RefreshedEnvironment);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_directory_shims_are_one_installation() {
        let root = std::env::temp_dir().join(format!(
            "agent-manager-node-pair-shims-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("node"), b"").unwrap();
        std::fs::write(root.join("node.exe"), b"").unwrap();
        std::fs::write(root.join("npm"), b"").unwrap();
        std::fs::write(root.join("npm.cmd"), b"").unwrap();

        let pair = resolve_pair(&[root.clone()], PairSource::ProcessPath).unwrap();

        #[cfg(target_os = "windows")]
        {
            assert_eq!(pair.node, root.join("node.exe"));
            assert_eq!(pair.npm, root.join("npm.cmd"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(pair.node, root.join("node"));
            assert_eq!(pair.npm, root.join("npm"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
