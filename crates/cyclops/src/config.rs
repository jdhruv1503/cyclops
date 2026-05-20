use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub task: TaskSource,
    pub worktree: PathBuf,
    pub model: String,
    pub max_turns: u32,
    pub litellm_url: Option<String>,
    pub litellm_key: Option<String>,
    pub data_dir: PathBuf,
    pub deadline: Option<String>,
    pub stdout_mode: StdoutMode,
    pub speculative: bool,
    pub compaction: bool,
    pub thinking_budget: u32,
    pub emit_thinking: bool,
    pub memory_impl: Option<MemoryImpl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskSource {
    Inline(String),
    File(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StdoutMode {
    Blocking,
    Coalesced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum MemoryImpl {
    Hybrid,
    RollingSummary,
    LlmExtract,
}

#[derive(Debug, Parser)]
#[command(
    name = "cyclops",
    about = "Single-binary Rust coding-agent harness",
    version
)]
struct Cli {
    #[arg(value_name = "TASK", conflicts_with = "task_file")]
    task: Option<String>,

    #[arg(long, value_name = "PATH", required_unless_present = "task")]
    task_file: Option<PathBuf>,

    #[arg(long, value_name = "PATH")]
    worktree: PathBuf,

    #[arg(long, value_name = "NAME")]
    model: String,

    #[arg(long, value_name = "N", default_value_t = 50)]
    max_turns: u32,

    #[arg(
        long,
        value_name = "URL",
        env = "CYCLOPS_LITELLM_URL",
        hide_env_values = true
    )]
    litellm_url: Option<String>,

    #[arg(
        long,
        value_name = "KEY",
        env = "CYCLOPS_LITELLM_KEY",
        hide_env_values = true
    )]
    litellm_key: Option<String>,

    #[arg(long, value_name = "PATH", default_value_os_t = default_data_dir())]
    data_dir: PathBuf,

    #[arg(long, value_name = "DURATION")]
    deadline: Option<String>,

    #[arg(long, value_name = "MODE", value_enum, default_value_t = StdoutMode::Coalesced)]
    stdout_mode: StdoutMode,

    #[arg(long)]
    no_speculative: bool,

    #[arg(long)]
    no_compaction: bool,

    #[arg(long, value_name = "N", default_value_t = 0)]
    thinking_budget: u32,

    #[arg(long)]
    emit_thinking: bool,

    #[arg(long, value_name = "NAME", value_enum)]
    memory_impl: Option<MemoryImpl>,
}

impl Config {
    pub fn parse() -> Self {
        Cli::parse().into()
    }

    pub fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        Cli::try_parse_from(args).map(Into::into)
    }
}

impl From<Cli> for Config {
    fn from(cli: Cli) -> Self {
        let task = match (cli.task, cli.task_file) {
            (Some(task), None) => TaskSource::Inline(task),
            (None, Some(path)) => TaskSource::File(path),
            _ => unreachable!("clap enforces exactly one task source"),
        };

        Self {
            task,
            worktree: cli.worktree,
            model: cli.model,
            max_turns: cli.max_turns,
            litellm_url: cli.litellm_url,
            litellm_key: cli.litellm_key,
            data_dir: cli.data_dir,
            deadline: cli.deadline,
            stdout_mode: cli.stdout_mode,
            speculative: !cli.no_speculative,
            compaction: !cli.no_compaction,
            thinking_budget: cli.thinking_budget,
            emit_thinking: cli.emit_thinking,
            memory_impl: cli.memory_impl,
        }
    }
}

fn default_data_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".cyclops"),
        None => PathBuf::from(".cyclops"),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard};

    use clap::error::ErrorKind;

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard<'a> {
        _lock: MutexGuard<'a, ()>,
        old_url: Option<OsString>,
        old_key: Option<OsString>,
    }

    impl Drop for EnvGuard<'_> {
        fn drop(&mut self) {
            restore_env("CYCLOPS_LITELLM_URL", self.old_url.take());
            restore_env("CYCLOPS_LITELLM_KEY", self.old_key.take());
        }
    }

    fn with_litellm_env(url: Option<&str>, key: Option<&str>) -> EnvGuard<'static> {
        let guard = EnvGuard {
            _lock: ENV_LOCK.lock().unwrap(),
            old_url: std::env::var_os("CYCLOPS_LITELLM_URL"),
            old_key: std::env::var_os("CYCLOPS_LITELLM_KEY"),
        };

        match url {
            Some(value) => std::env::set_var("CYCLOPS_LITELLM_URL", value),
            None => std::env::remove_var("CYCLOPS_LITELLM_URL"),
        }
        match key {
            Some(value) => std::env::set_var("CYCLOPS_LITELLM_KEY", value),
            None => std::env::remove_var("CYCLOPS_LITELLM_KEY"),
        }

        guard
    }

    fn restore_env(name: &str, value: Option<OsString>) {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn parses_inline_task_with_required_flags_and_defaults() {
        let _env = with_litellm_env(None, None);
        let config = Config::try_parse_from([
            "cyclops",
            "fix the failing test",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
        ])
        .unwrap();

        assert_eq!(
            config.task,
            TaskSource::Inline("fix the failing test".to_string())
        );
        assert_eq!(config.worktree, PathBuf::from("/tmp/wt"));
        assert_eq!(config.model, "claude-sonnet-4-7");
        assert_eq!(config.max_turns, 50);
        assert_eq!(config.litellm_url, None);
        assert_eq!(config.litellm_key, None);
        assert_eq!(config.data_dir, default_data_dir());
        assert_eq!(config.deadline, None);
        assert_eq!(config.stdout_mode, StdoutMode::Coalesced);
        assert!(config.speculative);
        assert!(config.compaction);
        assert_eq!(config.thinking_budget, 0);
        assert!(!config.emit_thinking);
        assert_eq!(config.memory_impl, None);
    }

    #[test]
    fn parses_task_file_and_explicit_options() {
        let config = Config::try_parse_from([
            "cyclops",
            "--task-file",
            "task.md",
            "--worktree",
            "/tmp/wt",
            "--model",
            "fireworks/accounts/fireworks/models/kimi-k2-instruct",
            "--max-turns",
            "5",
            "--litellm-url",
            "http://localhost:4000",
            "--litellm-key",
            "secret",
            "--data-dir",
            "/tmp/cyclops-data",
            "--deadline",
            "30m",
            "--stdout-mode",
            "blocking",
            "--no-speculative",
            "--no-compaction",
            "--thinking-budget",
            "1024",
            "--emit-thinking",
            "--memory-impl",
            "rolling_summary",
        ])
        .unwrap();

        assert_eq!(config.task, TaskSource::File(PathBuf::from("task.md")));
        assert_eq!(config.max_turns, 5);
        assert_eq!(config.litellm_url.as_deref(), Some("http://localhost:4000"));
        assert_eq!(config.litellm_key.as_deref(), Some("secret"));
        assert_eq!(config.data_dir, PathBuf::from("/tmp/cyclops-data"));
        assert_eq!(config.deadline.as_deref(), Some("30m"));
        assert_eq!(config.stdout_mode, StdoutMode::Blocking);
        assert!(!config.speculative);
        assert!(!config.compaction);
        assert_eq!(config.thinking_budget, 1024);
        assert!(config.emit_thinking);
        assert_eq!(config.memory_impl, Some(MemoryImpl::RollingSummary));
    }

    #[test]
    fn reads_litellm_defaults_from_environment() {
        let _env = with_litellm_env(Some("http://localhost:4000"), Some("env-secret"));
        let config = Config::try_parse_from([
            "cyclops",
            "fix it",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
        ])
        .unwrap();

        assert_eq!(config.litellm_url.as_deref(), Some("http://localhost:4000"));
        assert_eq!(config.litellm_key.as_deref(), Some("env-secret"));
    }

    #[test]
    fn rejects_unknown_flags() {
        let err = Config::try_parse_from([
            "cyclops",
            "fix it",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
            "--bogus",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn rejects_invalid_enum_values() {
        let err = Config::try_parse_from([
            "cyclops",
            "fix it",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
            "--stdout-mode",
            "immediate",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn help_does_not_print_env_secret_values() {
        let _env = with_litellm_env(Some("http://localhost:4000"), Some("env-secret"));
        let err = Config::try_parse_from(["cyclops", "--help"]).unwrap_err();
        let help = err.to_string();

        assert!(!help.contains("env-secret"));
        assert!(!help.contains("http://localhost:4000"));
        assert!(help.contains("CYCLOPS_LITELLM_KEY"));
        assert!(help.contains("CYCLOPS_LITELLM_URL"));
    }

    #[test]
    fn rejects_missing_task_source() {
        let err = Config::try_parse_from([
            "cyclops",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rejects_inline_task_with_task_file() {
        let err = Config::try_parse_from([
            "cyclops",
            "fix it",
            "--task-file",
            "task.md",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
        ])
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
    }
}
