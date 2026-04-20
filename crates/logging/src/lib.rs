use std::path::PathBuf;

pub const CRATE_NAME: &str = "stream-sync-logging";

/// Config-level selection for a JSON Lines sink.
///
/// This is a planning/config boundary only. It does not open files, create
/// directories, buffer records, rotate logs, or own a process-wide logger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonLinesSinkConfig {
    pub destination: JsonLinesSinkDestination,
}

impl JsonLinesSinkConfig {
    pub fn stderr() -> Self {
        Self {
            destination: JsonLinesSinkDestination::Stderr,
        }
    }

    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self {
            destination: JsonLinesSinkDestination::File(JsonLinesFileSinkConfig {
                path: path.into(),
                open_mode: JsonLinesFileOpenMode::AppendCreate,
                create_parent_dirs: false,
            }),
        }
    }
}

/// Where JSON Lines records should be written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JsonLinesSinkDestination {
    Stderr,
    File(JsonLinesFileSinkConfig),
}

/// File sink config shape for future TOML/server config wiring.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonLinesFileSinkConfig {
    pub path: PathBuf,
    pub open_mode: JsonLinesFileOpenMode,
    pub create_parent_dirs: bool,
}

/// Initial file open policy for future file sink implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonLinesFileOpenMode {
    AppendCreate,
}

/// Planned sink behavior after config normalization.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonLinesSinkPlan {
    pub destination: JsonLinesSinkDestination,
    pub buffering: JsonLinesBufferingPolicy,
    pub rotation: JsonLinesRotationPolicy,
}

impl JsonLinesSinkPlan {
    pub fn is_file_sink(&self) -> bool {
        matches!(self.destination, JsonLinesSinkDestination::File(_))
    }
}

/// Buffering policy placeholder for JSON Lines sinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonLinesBufferingPolicy {
    CallerOwnedWrite,
}

/// Rotation policy placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonLinesRotationPolicy {
    NotImplemented,
}

/// Boundary that turns sink config into a normalized plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct JsonLinesSinkPlanBoundary;

impl JsonLinesSinkPlanBoundary {
    pub fn plan(&self, config: JsonLinesSinkConfig) -> JsonLinesSinkPlan {
        JsonLinesSinkPlan {
            destination: config.destination,
            buffering: JsonLinesBufferingPolicy::CallerOwnedWrite,
            rotation: JsonLinesRotationPolicy::NotImplemented,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_lines_sink_plan_keeps_stderr_as_default_sink() {
        let boundary = JsonLinesSinkPlanBoundary;

        let plan = boundary.plan(JsonLinesSinkConfig::stderr());

        assert_eq!(plan.destination, JsonLinesSinkDestination::Stderr);
        assert_eq!(plan.buffering, JsonLinesBufferingPolicy::CallerOwnedWrite);
        assert_eq!(plan.rotation, JsonLinesRotationPolicy::NotImplemented);
        assert!(!plan.is_file_sink());
    }

    #[test]
    fn json_lines_sink_plan_accepts_file_destination_without_opening_it() {
        let boundary = JsonLinesSinkPlanBoundary;

        let plan = boundary.plan(JsonLinesSinkConfig::file("logs/server-auth.jsonl"));

        let JsonLinesSinkDestination::File(file) = plan.destination else {
            panic!("expected file sink");
        };
        assert_eq!(file.path, PathBuf::from("logs/server-auth.jsonl"));
        assert_eq!(file.open_mode, JsonLinesFileOpenMode::AppendCreate);
        assert!(!file.create_parent_dirs);
        assert_eq!(plan.rotation, JsonLinesRotationPolicy::NotImplemented);
    }
}
