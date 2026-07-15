pub fn parse_command_version_output(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|part| {
        let version = part.strip_prefix('v').unwrap_or(part);
        let mut components = version.split('.');
        let major = components.next()?;
        let minor = components.next()?;
        let patch = components.next()?;
        if components.next().is_some()
            || major.is_empty()
            || minor.is_empty()
            || patch.is_empty()
            || ![major, minor, patch]
                .iter()
                .all(|component| component.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
        Some(version.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_node_and_npm_version_output() {
        assert_eq!(
            parse_command_version_output("v24.15.0\n"),
            Some("24.15.0".into())
        );
        assert_eq!(
            parse_command_version_output("11.12.1\n"),
            Some("11.12.1".into())
        );
        assert_eq!(
            parse_command_version_output("node version v22.12.0\n"),
            Some("22.12.0".into())
        );
        assert_eq!(parse_command_version_output("not-a-version\n"), None);
        assert_eq!(parse_command_version_output("v24\n"), None);
    }
}
