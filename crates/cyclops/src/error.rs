use thiserror::Error;

pub type Result<T> = anyhow::Result<T>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CyclopsError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("model error: {0}")]
    Model(String),

    #[error("stream error: {0}")]
    Stream(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("filesystem error: {0}")]
    FileSystem(String),

    #[error("operation cancelled: {0}")]
    Cancelled(String),
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    fn fail_with_specific_error() -> Result<()> {
        Err(CyclopsError::Tool("read failed".to_string()).into())
    }

    #[test]
    fn display_and_debug_include_variant_context() {
        let error = CyclopsError::Config("missing --model".to_string());

        assert_eq!(error.to_string(), "configuration error: missing --model");
        assert_eq!(format!("{error:?}"), "Config(\"missing --model\")");
    }

    #[test]
    fn anyhow_conversion_preserves_specific_error() {
        let error: anyhow::Error =
            CyclopsError::FileSystem("path escaped worktree".to_string()).into();

        assert_eq!(error.to_string(), "filesystem error: path escaped worktree");
        assert_eq!(
            error.downcast_ref::<CyclopsError>(),
            Some(&CyclopsError::FileSystem(
                "path escaped worktree".to_string()
            ))
        );
    }

    #[test]
    fn result_alias_supports_context_and_downcast_usage() {
        let error = fail_with_specific_error()
            .context("tool dispatch failed")
            .unwrap_err();

        assert_eq!(error.to_string(), "tool dispatch failed");
        assert_eq!(
            error.chain().map(ToString::to_string).collect::<Vec<_>>(),
            vec![
                "tool dispatch failed".to_string(),
                "tool error: read failed".to_string()
            ]
        );
        assert_eq!(
            error.downcast_ref::<CyclopsError>(),
            Some(&CyclopsError::Tool("read failed".to_string()))
        );
    }
}
