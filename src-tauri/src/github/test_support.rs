//! Isolated fake executables: tests never invoke installed gh/git or contact a remote.
use crate::shell_env::ShellEnv;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(crate) struct FakeCli {
    dir: tempfile::TempDir,
    pub env: ShellEnv,
}

impl FakeCli {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let script = r#"#!/bin/sh
# cwd must be the isolated worktree; all outputs/logs are relative to it.
printf '%s\0' "$@" >> calls
printf '\0' >> calls
if [ -f "fail-$2" ]; then
    /bin/cat "fail-$2" >&2
    exit 1
fi
case "$1 $2" in
  'pr view') /bin/cat response.json ;;
  'pr create') printf '%s\n' 'https://github.example/team/project/pull/42' ;;
  'pr merge') exit 0 ;;
  'push origin') exit 0 ;;
  *) exit 90 ;;
esac
"#;
        for program in ["gh", "git"] {
            let path = bin.join(program);
            fs::write(&path, script).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(
            dir.path().join("response.json"),
            include_str!("fixtures/gh_pr_view_mixed.json"),
        )
        .unwrap();
        Self {
            env: ShellEnv {
                path: bin.to_str().unwrap().into(),
            },
            dir,
        }
    }

    pub fn repo(&self) -> &Path {
        self.dir.path()
    }

    pub fn fail(&self, operation: &str, message: &str) {
        fs::write(self.repo().join(format!("fail-{operation}")), message).unwrap();
    }

    pub fn response(&self, json: &str) {
        fs::write(self.repo().join("response.json"), json).unwrap();
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        let bytes = fs::read(self.repo().join("calls")).unwrap_or_default();
        let text = String::from_utf8(bytes).unwrap();
        text.split("\0\0")
            .filter(|call| !call.is_empty())
            .map(|call| call.split('\0').map(String::from).collect())
            .collect()
    }
}
