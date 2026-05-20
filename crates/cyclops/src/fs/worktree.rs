use std::fs;
use std::path::{Path, PathBuf};

use crate::{CyclopsError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeRoot {
    root: PathBuf,
}

impl WorktreeRoot {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = canonicalize(root.as_ref(), "canonicalize worktree root")?;
        let metadata = fs::metadata(&root).map_err(|error| {
            CyclopsError::FileSystem(format!("stat worktree root {}: {error}", root.display()))
        })?;

        if !metadata.is_dir() {
            return Err(CyclopsError::FileSystem(format!(
                "worktree root is not a directory: {}",
                root.display()
            ))
            .into());
        }

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn resolve_existing(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let resolved = canonicalize(&candidate, "canonicalize worktree path")?;

        if !resolved.starts_with(&self.root) {
            return Err(CyclopsError::FileSystem(format!(
                "path escaped worktree: {}",
                path.display()
            ))
            .into());
        }

        Ok(resolved)
    }
}

fn canonicalize(path: &Path, action: &str) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|error| {
        CyclopsError::FileSystem(format!("{action} {}: {error}", path.display())).into()
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let unique = format!(
                "cyclops-worktree-test-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "contents\n").unwrap();
    }

    fn assert_escaped(error: anyhow::Error) {
        assert!(matches!(
            error.downcast_ref::<CyclopsError>(),
            Some(CyclopsError::FileSystem(message)) if message.starts_with("path escaped worktree:")
        ));
    }

    #[test]
    fn resolves_absolute_path_inside_worktree() {
        let temp = TestDir::new();
        let file = temp.path().join("src/main.rs");
        write_file(&file);

        let worktree = WorktreeRoot::new(temp.path()).unwrap();
        let resolved = worktree.resolve_existing(&file).unwrap();

        assert_eq!(resolved, fs::canonicalize(&file).unwrap());
    }

    #[test]
    fn rejects_absolute_path_outside_worktree() {
        let worktree_dir = TestDir::new();
        let outside_dir = TestDir::new();
        let outside = outside_dir.path().join("hosts");
        write_file(&outside);

        let worktree = WorktreeRoot::new(worktree_dir.path()).unwrap();
        let error = worktree.resolve_existing(&outside).unwrap_err();

        assert_escaped(error);
    }

    #[test]
    fn resolves_relative_path_inside_worktree() {
        let temp = TestDir::new();
        let file = temp.path().join("src/lib.rs");
        write_file(&file);

        let worktree = WorktreeRoot::new(temp.path()).unwrap();
        let resolved = worktree.resolve_existing("src/lib.rs").unwrap();

        assert_eq!(resolved, fs::canonicalize(&file).unwrap());
    }

    #[test]
    fn rejects_dot_dot_traversal_outside_worktree() {
        let temp = TestDir::new();
        let root = temp.path().join("worktree");
        let outside = temp.path().join("outside/secret.txt");
        fs::create_dir_all(&root).unwrap();
        write_file(&outside);

        let worktree = WorktreeRoot::new(&root).unwrap();
        let error = worktree
            .resolve_existing("../outside/secret.txt")
            .unwrap_err();

        assert_escaped(error);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let worktree_dir = TestDir::new();
        let outside_dir = TestDir::new();
        let outside = outside_dir.path().join("secret.txt");
        write_file(&outside);
        symlink(&outside, worktree_dir.path().join("link")).unwrap();

        let worktree = WorktreeRoot::new(worktree_dir.path()).unwrap();
        let error = worktree.resolve_existing("link").unwrap_err();

        assert_escaped(error);
    }
}
