//! Helpers for constructing OpAMP messages.

use crate::pb;
use sha2::{Digest, Sha256};

/// Compute the SHA-256 hash a server uses for `AgentRemoteConfig.config_hash`.
pub fn config_hash(body: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(body);
    hasher.finalize().to_vec()
}

/// Wrap a YAML document as a single-entry [`pb::AgentConfigMap`].
pub fn yaml_config_map(yaml: &str) -> pb::AgentConfigMap {
    let mut config_map = std::collections::HashMap::new();
    config_map.insert(
        String::new(),
        pb::AgentConfigFile {
            body: yaml.as_bytes().to_vec(),
            content_type: "text/yaml".to_string(),
        },
    );
    pb::AgentConfigMap { config_map }
}
