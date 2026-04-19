use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Path, PathBuf},
};

pub const CRATE_NAME: &str = "stream-sync-config";

/// Server-side authentication settings loaded from TOML config.
///
/// This crate only owns the configuration shape and minimal TOML auth-section
/// parsing. It does not resolve secrets, validate tokens, or decide whether an
/// auth request is accepted.
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
#[derive(Clone, PartialEq, Eq)]
pub enum SharedTokenSecretRef {
    InlinePlaceholder(String),
    EnvironmentVariable(String),
}

impl fmt::Debug for SharedTokenSecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InlinePlaceholder(_) => formatter
                .debug_tuple("InlinePlaceholder")
                .field(&"<redacted>")
                .finish(),
            Self::EnvironmentVariable(name) => formatter
                .debug_tuple("EnvironmentVariable")
                .field(name)
                .finish(),
        }
    }
}

/// Boundary for future server auth config loading.
pub trait ServerAuthConfigSource {
    fn load_server_auth_config(&self) -> Result<ServerAuthConfig, ConfigLoadError>;
}

/// Boundary for loading server auth config from a TOML file.
///
/// This reads only the minimal auth whitelist/token shape. External secret
/// resolution is intentionally left for a later task.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerAuthConfigBoundary {
    path: PathBuf,
}

impl ServerAuthConfigBoundary {
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_from_str(input: &str) -> Result<ServerAuthConfig, ConfigLoadError> {
        let parsed = parse_auth_clients(input)?;

        if parsed.clients.is_empty() {
            return Err(ConfigLoadError::NoAuthClients);
        }

        let mut allowed_clients = Vec::with_capacity(parsed.clients.len());
        let mut shared_tokens = Vec::with_capacity(parsed.clients.len());

        for (client_id, client) in parsed.clients {
            let secret_ref = match (client.shared_token, client.shared_token_env) {
                (Some(_), Some(_)) => {
                    return Err(ConfigLoadError::ConflictingSharedTokenRefs {
                        client_id: client_id.clone(),
                    });
                }
                (Some(shared_token), None) => {
                    if shared_token.trim().is_empty() {
                        return Err(ConfigLoadError::MissingSharedToken { client_id });
                    }
                    SharedTokenSecretRef::InlinePlaceholder(shared_token)
                }
                (None, Some(environment_variable)) => {
                    if environment_variable.trim().is_empty() {
                        return Err(ConfigLoadError::MissingSharedToken { client_id });
                    }
                    SharedTokenSecretRef::EnvironmentVariable(environment_variable)
                }
                (None, None) => {
                    return Err(ConfigLoadError::MissingSharedToken {
                        client_id: client_id.clone(),
                    });
                }
            };

            allowed_clients.push(AllowedClientConfig {
                client_id: client_id.clone(),
                shared_token_id: client_id.clone(),
            });
            shared_tokens.push(SharedTokenConfig {
                token_id: client_id,
                secret_ref,
            });
        }

        Ok(ServerAuthConfig {
            allowed_clients,
            shared_tokens,
        })
    }
}

impl Default for ServerAuthConfigBoundary {
    fn default() -> Self {
        Self::from_path("configs/examples/server.example.toml")
    }
}

impl ServerAuthConfigSource for ServerAuthConfigBoundary {
    fn load_server_auth_config(&self) -> Result<ServerAuthConfig, ConfigLoadError> {
        let content = fs::read_to_string(&self.path).map_err(|error| ConfigLoadError::Io {
            path: self.path.clone(),
            message: error.to_string(),
        })?;
        Self::load_from_str(&content)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigLoadError {
    Io { path: PathBuf, message: String },
    InvalidTomlLine { line: usize, message: String },
    InvalidTomlString { line: usize, key: String },
    MissingAuthSection,
    NoAuthClients,
    MissingSharedToken { client_id: String },
    ConflictingSharedTokenRefs { client_id: String },
}

#[derive(Debug, Default)]
struct ParsedAuthToml {
    saw_auth_section: bool,
    clients: BTreeMap<String, ParsedAuthClientToml>,
}

#[derive(Debug, Default)]
struct ParsedAuthClientToml {
    shared_token: Option<String>,
    shared_token_env: Option<String>,
}

fn parse_auth_clients(input: &str) -> Result<ParsedAuthToml, ConfigLoadError> {
    let mut parsed = ParsedAuthToml::default();
    let mut current_client_id: Option<String> = None;

    for (index, raw_line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(section_name) = line
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            if section_name == "auth" || section_name == "auth.clients" {
                parsed.saw_auth_section = true;
                current_client_id = None;
                continue;
            }

            if let Some(client_id) = section_name.strip_prefix("auth.clients.") {
                parsed.saw_auth_section = true;
                if client_id.trim().is_empty() {
                    return Err(ConfigLoadError::InvalidTomlLine {
                        line: line_number,
                        message: "auth client table name is empty".to_string(),
                    });
                }
                let client_id = client_id.trim().to_string();
                parsed.clients.entry(client_id.clone()).or_default();
                current_client_id = Some(client_id);
                continue;
            }

            current_client_id = None;
            continue;
        }

        let Some((key, raw_value)) = line.split_once('=') else {
            return Err(ConfigLoadError::InvalidTomlLine {
                line: line_number,
                message: "expected key = value".to_string(),
            });
        };
        let key = key.trim();
        let raw_value = raw_value.trim();

        let Some(client_id) = current_client_id.as_deref() else {
            continue;
        };

        if key == "shared_token" {
            let value = parse_toml_string(raw_value, line_number, key)?;
            parsed
                .clients
                .entry(client_id.to_string())
                .or_default()
                .shared_token = Some(value);
        } else if key == "shared_token_env" {
            let value = parse_toml_string(raw_value, line_number, key)?;
            parsed
                .clients
                .entry(client_id.to_string())
                .or_default()
                .shared_token_env = Some(value);
        }
    }

    if !parsed.saw_auth_section {
        return Err(ConfigLoadError::MissingAuthSection);
    }

    Ok(parsed)
}

fn parse_toml_string(value: &str, line: usize, key: &str) -> Result<String, ConfigLoadError> {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(ConfigLoadError::InvalidTomlString {
            line,
            key: key.to_string(),
        });
    };

    Ok(inner.to_string())
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#')
        .map(|(before_comment, _)| before_comment)
        .unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_server_auth_config_from_example_toml_shape() {
        let config = ServerAuthConfigBoundary::load_from_str(
            r#"
[auth]
enabled = true
require_known_clients = true

[auth.clients.player1]
display_name = "Player 1"
shared_token = "token-1"

[auth.clients.player2]
display_name = "Player 2"
shared_token = "token-2"
"#,
        )
        .expect("auth config should parse");

        assert_eq!(
            config.allowed_clients,
            vec![
                AllowedClientConfig {
                    client_id: "player1".to_string(),
                    shared_token_id: "player1".to_string(),
                },
                AllowedClientConfig {
                    client_id: "player2".to_string(),
                    shared_token_id: "player2".to_string(),
                },
            ]
        );
        assert_eq!(
            config.shared_tokens,
            vec![
                SharedTokenConfig {
                    token_id: "player1".to_string(),
                    secret_ref: SharedTokenSecretRef::InlinePlaceholder("token-1".to_string()),
                },
                SharedTokenConfig {
                    token_id: "player2".to_string(),
                    secret_ref: SharedTokenSecretRef::InlinePlaceholder("token-2".to_string()),
                },
            ]
        );
    }

    #[test]
    fn loads_repository_server_example_auth_config() {
        let config = ServerAuthConfigBoundary::load_from_str(include_str!(
            "../../../configs/examples/server.example.toml"
        ))
        .expect("repository example server config should parse");

        assert_eq!(config.allowed_clients.len(), 4);
        assert_eq!(config.shared_tokens.len(), 4);
        assert_eq!(config.allowed_clients[0].client_id, "player1");
        assert_eq!(config.allowed_clients[0].shared_token_id, "player1");
        assert_eq!(config.shared_tokens[0].token_id, "player1");
        assert_eq!(
            config.shared_tokens[0].secret_ref,
            SharedTokenSecretRef::InlinePlaceholder("replace-with-shared-token-1".to_string())
        );
    }

    #[test]
    fn loads_server_auth_config_from_file_boundary() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../configs/examples/server.example.toml");
        let boundary = ServerAuthConfigBoundary::from_path(path);

        let config = boundary
            .load_server_auth_config()
            .expect("repository example server config should load from file");

        assert_eq!(config.allowed_clients.len(), 4);
        assert_eq!(config.shared_tokens.len(), 4);
    }

    #[test]
    fn rejects_missing_auth_section() {
        let result = ServerAuthConfigBoundary::load_from_str("[server]\nbind_port = 5000\n");

        assert_eq!(result, Err(ConfigLoadError::MissingAuthSection));
    }

    #[test]
    fn rejects_empty_auth_clients() {
        let result = ServerAuthConfigBoundary::load_from_str("[auth]\nenabled = true\n");

        assert_eq!(result, Err(ConfigLoadError::NoAuthClients));
    }

    #[test]
    fn rejects_empty_shared_token() {
        let result = ServerAuthConfigBoundary::load_from_str(
            r#"
[auth.clients.player1]
shared_token = " "
"#,
        );

        assert_eq!(
            result,
            Err(ConfigLoadError::MissingSharedToken {
                client_id: "player1".to_string()
            })
        );
    }

    #[test]
    fn loads_environment_variable_secret_ref() {
        let config = ServerAuthConfigBoundary::load_from_str(
            r#"
[auth.clients.player1]
shared_token_env = "STREAMSYNC_PLAYER1_TOKEN"
"#,
        )
        .expect("auth config should parse env secret ref");

        assert_eq!(
            config.shared_tokens,
            vec![SharedTokenConfig {
                token_id: "player1".to_string(),
                secret_ref: SharedTokenSecretRef::EnvironmentVariable(
                    "STREAMSYNC_PLAYER1_TOKEN".to_string()
                ),
            }]
        );
    }

    #[test]
    fn rejects_conflicting_shared_token_refs() {
        let result = ServerAuthConfigBoundary::load_from_str(
            r#"
[auth.clients.player1]
shared_token = "token"
shared_token_env = "STREAMSYNC_PLAYER1_TOKEN"
"#,
        );

        assert_eq!(
            result,
            Err(ConfigLoadError::ConflictingSharedTokenRefs {
                client_id: "player1".to_string()
            })
        );
    }

    #[test]
    fn redacts_inline_secret_ref_in_debug_output() {
        let secret_ref = SharedTokenSecretRef::InlinePlaceholder("do-not-print".to_string());

        assert_eq!(
            format!("{secret_ref:?}"),
            "InlinePlaceholder(\"<redacted>\")"
        );
    }
}
