pub mod config;
pub mod error;
pub mod events;
pub mod fs;

pub const CRATE_NAME: &str = "cyclops";

pub use error::{CyclopsError, Result};

pub fn run(config: config::Config) -> i32 {
    let _config = config;
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_stub_is_ready() {
        assert_eq!(CRATE_NAME, "cyclops");
        let config = config::Config::try_parse_from([
            "cyclops",
            "fix it",
            "--worktree",
            "/tmp/wt",
            "--model",
            "claude-sonnet-4-7",
        ])
        .unwrap();
        assert_eq!(run(config), 0);
    }
}
