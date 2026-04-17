pub const CRATE_NAME: &str = "stream-sync-config";

/// Server-side authentication settings as loaded from future TOML config.
///
/// This crate only owns the configuration shape. It does not read files yet,
/// resolve secrets, validate tokens, or decide whether an auth request is
/// accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAuthConfig {
    pub allowed_clients: Vec<AllowedClientConfig>,
    pub shared_tokens: Vec<SharedTokenConfig>,
}

/// One client entry from the server whitelist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedClientConfig {
    pub client_id: String,
    pub shared_token_id: String,
}

/// One configured shared token entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedTokenConfig {
    pub token_id: String,
    pub secret_ref: SharedTokenSecretRef,
}

/// Reference to token material that future verification code may resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedTokenSecretRef {
    InlinePlaceholder(String),
    EnvironmentVariable(String),
}

/// Boundary for future server auth config loading.
pub trait ServerAuthConfigSource {
    fn load_server_auth_config(&self) -> Result<ServerAuthConfig, ConfigLoadError>;
}

/// Placeholder config loader boundary.
///
/// The real TOML file loading and secret resolution are intentionally left for a
/// later task.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ServerAuthConfigBoundary;

impl ServerAuthConfigSource for ServerAuthConfigBoundary {
    fn load_server_auth_config(&self) -> Result<ServerAuthConfig, ConfigLoadError> {
        Err(ConfigLoadError::NotImplemented)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoadError {
    NotImplemented,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_auth_config_boundary_does_not_load_real_config_yet() {
        let boundary = ServerAuthConfigBoundary;

        let result = boundary.load_server_auth_config();

        assert_eq!(result, Err(ConfigLoadError::NotImplemented));
    }
}
