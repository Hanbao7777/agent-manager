#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeInstallerPlatform {
    Windows,
    Macos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeInstallerArchitecture {
    X64,
    Arm64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAssetDescriptor {
    pub index_file: &'static str,
    pub package_name: String,
}

pub fn node_asset_descriptor(
    platform: NodeInstallerPlatform,
    architecture: NodeInstallerArchitecture,
    version: &str,
) -> NodeAssetDescriptor {
    match platform {
        NodeInstallerPlatform::Windows => {
            let architecture = match architecture {
                NodeInstallerArchitecture::X64 => "x64",
                NodeInstallerArchitecture::Arm64 => "arm64",
            };
            NodeAssetDescriptor {
                index_file: match architecture {
                    "x64" => "win-x64-msi",
                    "arm64" => "win-arm64-msi",
                    _ => unreachable!(),
                },
                package_name: format!("node-{version}-{architecture}.msi"),
            }
        }
        NodeInstallerPlatform::Macos => NodeAssetDescriptor {
            // Node publishes one universal PKG and lists it under the historical
            // osx-x64-pkg index capability for both supported architectures.
            index_file: "osx-x64-pkg",
            package_name: format!("node-{version}.pkg"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_index_identifiers_map_to_downloadable_packages() {
        let windows = node_asset_descriptor(
            NodeInstallerPlatform::Windows,
            NodeInstallerArchitecture::X64,
            "v24.18.0",
        );
        assert_eq!(windows.index_file, "win-x64-msi");
        assert_eq!(windows.package_name, "node-v24.18.0-x64.msi");

        let macos_x64 = node_asset_descriptor(
            NodeInstallerPlatform::Macos,
            NodeInstallerArchitecture::X64,
            "v24.18.0",
        );
        assert_eq!(macos_x64.index_file, "osx-x64-pkg");
        assert_eq!(macos_x64.package_name, "node-v24.18.0.pkg");

        let macos_arm64 = node_asset_descriptor(
            NodeInstallerPlatform::Macos,
            NodeInstallerArchitecture::Arm64,
            "v24.18.0",
        );
        assert_eq!(macos_arm64.index_file, "osx-x64-pkg");
        assert_eq!(macos_arm64.package_name, "node-v24.18.0.pkg");
    }
}
