//! Real Git fixtures. All configuration, worktrees and remotes live in a TempDir.
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{git, repo, status, worktree};
use crate::shell_env::ShellEnv;

pub(super) struct Fixture {
    pub dir: TempDir,
    pub env: ShellEnv,
    pub repo: PathBuf,
}

impl Fixture {
    pub fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        let resolved = ShellEnv {
            path: std::env::var("PATH").unwrap(),
        };
        let git_path = resolved.which("git").unwrap();
        let wrapper = bin.join("git");
        let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', "'\\''"));
        std::fs::write(&wrapper, format!(
            "#!/bin/sh\nunset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_CONFIG_COUNT\nexport GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null GIT_TERMINAL_PROMPT=0\nexec {} -c user.name=Test -c user.email=test@example.invalid -c commit.gpgsign=false -c core.hooksPath=/dev/null \"$@\"\n",
            quote(&git_path)
        )).unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let env = ShellEnv {
            path: format!("{}:{}", bin.display(), resolved.path),
        };
        let repo = dir.path().join("repo");
        repo::init_repo(&env, &repo, "main").unwrap();
        Self { dir, env, repo }
    }

    pub fn git(&self, cwd: &Path, args: &[&str]) -> String {
        git(&self.env, cwd, args).unwrap()
    }
    pub fn commit_file(&self, cwd: &Path, path: &str, content: &[u8]) {
        std::fs::write(cwd.join(path), content).unwrap();
        self.git(cwd, &["--literal-pathspecs", "add", "--", path]);
        self.git(cwd, &["commit", "-m", "fixture"]);
    }
    pub fn task(&self) -> PathBuf {
        let path = self.dir.path().join("task");
        worktree::add_worktree(&self.env, &self.repo, &path, "task", "main").unwrap();
        path
    }
    pub fn remote(&self) -> PathBuf {
        let remote = self.dir.path().join("remote.git");
        self.git(
            self.dir.path(),
            &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
        );
        self.git(
            &self.repo,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        repo::push(&self.env, &self.repo, "main", true).unwrap();
        remote
    }
}

#[test]
fn repo_initialization_and_default_branch() {
    let f = Fixture::new();
    let sub = f.repo.join("nested");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(
        repo::repo_root(&f.env, &sub).unwrap(),
        f.repo.canonicalize().unwrap()
    );
    assert_eq!(
        f.git(&f.repo, &["log", "-1", "--format=%s"]).trim(),
        "Initial commit"
    );
    assert_eq!(repo::default_branch(&f.env, &f.repo).unwrap(), "main");
    assert!(!repo::has_origin(&f.env, &f.repo).unwrap());
    assert!(repo::has_origin(&f.env, f.dir.path()).is_err());
    assert!(repo::init_repo(&f.env, &f.repo, "main").is_err());
    f.git(&f.repo, &["branch", "-m", "master"]);
    f.git(&f.repo, &["checkout", "-b", "feature"]);
    assert_eq!(repo::default_branch(&f.env, &f.repo).unwrap(), "master");
    f.git(
        &f.repo,
        &["update-ref", "refs/remotes/origin/release", "HEAD"],
    );
    f.git(
        &f.repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/release",
        ],
    );
    assert_eq!(repo::default_branch(&f.env, &f.repo).unwrap(), "release");
    f.git(
        &f.repo,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    );
    f.git(&f.repo, &["branch", "-D", "master"]);
    assert_eq!(repo::default_branch(&f.env, &f.repo).unwrap(), "feature");
    f.git(&f.repo, &["checkout", "--detach"]);
    assert_eq!(repo::current_branch(&f.env, &f.repo).unwrap(), None);
    assert!(repo::default_branch(&f.env, &f.repo).is_err());
}

#[test]
fn branch_validation() {
    for name in [
        "", "HEAD", "@", "-f", "x..y", "/a", "a/", "a//b", ".a", "a/.b", "a.lock/b", "a.", "a@{b",
        "a:b", "a*b", "a?b", "a[b", "a\\b", "a~b", "a^b", "a\nb", "a b",
    ] {
        assert!(!repo::is_valid_branch_name(name), "{name:?}");
    }
    for name in ["main", "feat/login", "日本語", "a.b", "a@b"] {
        assert!(repo::is_valid_branch_name(name));
    }
}

#[test]
fn worktree_lifecycle_existing_branch_and_dirty_removal() {
    let f = Fixture::new();
    let path = f.task();
    let list = worktree::list_worktrees(&f.env, &f.repo).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].is_main);
    assert_eq!(list[1].branch.as_deref(), Some("task"));
    std::fs::write(path.join("untracked"), "keep").unwrap();
    assert!(worktree::remove_worktree(&f.env, &f.repo, &path, false).is_err());
    assert!(path.join("untracked").exists());
    worktree::remove_worktree(&f.env, &f.repo, &path, true).unwrap();
    worktree::add_worktree(&f.env, &f.repo, &path, "task", "main").unwrap();
    f.git(
        &f.repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "test",
            path.to_str().unwrap(),
        ],
    );
    assert!(worktree::list_worktrees(&f.env, &f.repo).unwrap()[1].locked);
    f.git(&f.repo, &["worktree", "unlock", path.to_str().unwrap()]);
    f.commit_file(&path, "task.txt", b"task");
    worktree::remove_worktree(&f.env, &f.repo, &path, false).unwrap();
    assert!(worktree::delete_branch(&f.env, &f.repo, "task", false).is_err());
    worktree::delete_branch(&f.env, &f.repo, "task", true).unwrap();
    assert_eq!(worktree::list_worktrees(&f.env, &f.repo).unwrap().len(), 1);
}

#[test]
fn local_remote_push_fetch_and_tracking_status() {
    let f = Fixture::new();
    let remote = f.remote();
    assert!(repo::has_origin(&f.env, &f.repo).unwrap());
    let task = f.task();
    repo::push(&f.env, &task, "task", true).unwrap();
    let st = status::status(&f.env, &task).unwrap();
    assert_eq!(st.upstream.as_deref(), Some("origin/task"));
    f.commit_file(&task, "ahead", b"ahead");
    assert_eq!(status::status(&f.env, &task).unwrap().ahead, 1);
    repo::push(&f.env, &task, "task", false).unwrap();
    assert_eq!(status::status(&f.env, &task).unwrap().ahead, 0);
    assert_eq!(
        f.git(&task, &["rev-parse", "HEAD"]),
        f.git(&remote, &["rev-parse", "task"])
    );
    f.git(&f.repo, &["update-ref", "-d", "refs/remotes/origin/task"]);
    repo::fetch(&f.env, &f.repo, "task").unwrap();
    assert_eq!(
        f.git(&f.repo, &["rev-parse", "origin/task"]),
        f.git(&task, &["rev-parse", "HEAD"])
    );
}

#[test]
fn status_entries_and_quoted_paths() {
    let f = Fixture::new();
    let name = "日本語 space\t\n\".txt";
    f.commit_file(&f.repo, name, b"original");
    std::fs::write(f.repo.join(name), b"modified").unwrap();
    std::fs::write(f.repo.join("untracked"), b"new").unwrap();
    let st = status::status(&f.env, &f.repo).unwrap();
    assert!(st.files.iter().any(|v| v.path == name && v.status == " M"));
    assert!(st
        .files
        .iter()
        .any(|v| v.path == "untracked" && v.status == "??"));
    f.git(&f.repo, &["restore", "--", name]);
    f.git(&f.repo, &["mv", "--", name, "new name.txt"]);
    assert!(status::status(&f.env, &f.repo)
        .unwrap()
        .files
        .iter()
        .any(|v| v.path == "new name.txt" && v.status == "R "));
}

#[test]
fn porcelain_parsers_cover_headers_flags_and_malformed_lines() {
    let st = status::parse_status_porcelain_v2("# branch.head (detached)\n# branch.upstream origin/main\n# branch.ab +2 -3\n1 A. N... 0 0 0 a b file with space\n2 R. N... 0 0 0 a b R100 new\told\nu UU N... 0 0 0 0 a b c conflict\n? new\n! ignored\n1 broken\n");
    assert_eq!(st.branch, None);
    assert_eq!((st.ahead, st.behind), (2, 3));
    assert_eq!(st.files.len(), 4);
    assert_eq!(st.files[0].path, "file with space");
    assert_eq!(st.files[2].status, "UU");
    let trees = worktree::parse_worktree_porcelain("worktree /bare\nbare\n\nworktree \"/with\\tspace\"\nHEAD abc\ndetached\nlocked reason\nprunable reason\n");
    assert!(trees[0].is_bare);
    assert_eq!(trees[1].path, "/with\tspace");
    assert!(trees[1].locked && trees[1].prunable && trees[1].is_detached);
}

use super::conflict;
use super::types::{ConflictKind, ConflictResolution, ConflictState};

fn conflicting(f: &Fixture, name: &str) -> PathBuf {
    f.commit_file(&f.repo, name, b"original\n");
    let task = f.task();
    f.commit_file(&task, name, b"ours\n");
    f.commit_file(&f.repo, name, b"theirs\n");
    task
}

#[test]
fn conflict_read_choose_each_side_and_commit() {
    for (resolution, expected) in [
        (ConflictResolution::Ours, "ours\n"),
        (ConflictResolution::Theirs, "theirs\n"),
    ] {
        let f = Fixture::new();
        let name = "日本語 [x] space\t\n.txt";
        let task = conflicting(&f, name);
        let cs = conflict::merge_base_into(&f.env, &task, "main").unwrap();
        assert!(cs.merge_in_progress && !cs.ready_to_commit);
        assert_eq!(cs.base_ref.as_deref(), Some("main"));
        assert_eq!(cs.files[0].path, name);
        assert_eq!(cs.files[0].kind, ConflictKind::BothModified);
        assert!(status::status(&f.env, &task).unwrap().merge_in_progress);
        assert_eq!(status::status(&f.env, &task).unwrap().files[0].status, "UU");
        assert_eq!(conflict::conflict_state(&f.env, &task).unwrap(), cs);
        let content = conflict::read_conflict_file(&f.env, &task, name).unwrap();
        assert_eq!(content.ours.as_deref(), Some("ours\n"));
        assert_eq!(content.theirs.as_deref(), Some("theirs\n"));
        assert!(content.working.unwrap().contains("<<<<<<<"));
        assert!(matches!(
            conflict::commit_merge(&f.env, &task),
            Err(crate::error::AppError::InvalidInput(_))
        ));
        assert!(conflict::merge_base_into(&f.env, &task, "main").is_err());
        let ready = conflict::resolve_file(&f.env, &task, name, resolution).unwrap();
        assert!(ready.ready_to_commit && ready.files.is_empty());
        assert_eq!(std::fs::read_to_string(task.join(name)).unwrap(), expected);
        conflict::commit_merge(&f.env, &task).unwrap();
        assert_eq!(
            conflict::conflict_state(&f.env, &task).unwrap(),
            ConflictState::default()
        );
        assert_eq!(
            f.git(&task, &["rev-list", "--parents", "-n", "1", "HEAD"])
                .split_whitespace()
                .count(),
            3
        );
        assert_eq!(
            conflict::conflict_state(&f.env, &f.repo).unwrap(),
            ConflictState::default()
        );
    }
}

#[test]
fn manual_resolution_and_abort_preserve_original_head() {
    let f = Fixture::new();
    let task = conflicting(&f, "file");
    let head = f.git(&task, &["rev-parse", "HEAD"]);
    conflict::merge_base_into(&f.env, &task, "main").unwrap();
    std::fs::write(task.join("file"), "combined\n").unwrap();
    let cs =
        conflict::resolve_file(&f.env, &task, "file", ConflictResolution::MarkResolved).unwrap();
    assert!(cs.ready_to_commit);
    conflict::abort_merge(&f.env, &task).unwrap();
    assert_eq!(
        std::fs::read_to_string(task.join("file")).unwrap(),
        "ours\n"
    );
    assert_eq!(head, f.git(&task, &["rev-parse", "HEAD"]));
    assert!(
        !conflict::conflict_state(&f.env, &task)
            .unwrap()
            .merge_in_progress
    );
    conflict::abort_merge(&f.env, &task).unwrap();
    conflict::merge_base_into(&f.env, &task, "main").unwrap();
    std::fs::write(task.join("file"), "combined\n").unwrap();
    conflict::resolve_file(&f.env, &task, "file", ConflictResolution::MarkResolved).unwrap();
    conflict::commit_merge(&f.env, &task).unwrap();
    assert_eq!(f.git(&task, &["show", "HEAD:file"]), "combined\n");
}

#[test]
fn delete_modify_conflicts_support_deleted_and_present_sides() {
    for deleted_by_us in [true, false] {
        for choose_deleted in [true, false] {
            let f = Fixture::new();
            f.commit_file(&f.repo, "file", b"original\n");
            let task = f.task();
            let (deleted, modified) = if deleted_by_us {
                (&task, &f.repo)
            } else {
                (&f.repo, &task)
            };
            f.git(deleted, &["rm", "file"]);
            f.git(deleted, &["commit", "-m", "delete"]);
            f.commit_file(modified, "file", b"modified\n");
            let cs = conflict::merge_base_into(&f.env, &task, "main").unwrap();
            assert_eq!(
                cs.files[0].kind,
                if deleted_by_us {
                    ConflictKind::DeletedByUs
                } else {
                    ConflictKind::DeletedByThem
                }
            );
            let content = conflict::read_conflict_file(&f.env, &task, "file").unwrap();
            assert_eq!(content.ours.is_none(), deleted_by_us);
            assert_eq!(content.theirs.is_none(), !deleted_by_us);
            let resolution = if choose_deleted == deleted_by_us {
                ConflictResolution::Ours
            } else {
                ConflictResolution::Theirs
            };
            assert!(
                conflict::resolve_file(&f.env, &task, "file", resolution)
                    .unwrap()
                    .ready_to_commit
            );
            assert_eq!(task.join("file").exists(), !choose_deleted);
            conflict::commit_merge(&f.env, &task).unwrap();
        }
    }
}

#[test]
fn manual_deletion_binary_and_add_add_conflicts() {
    let f = Fixture::new();
    let task = f.task();
    f.commit_file(&task, "binary", &[0, 1, 255]);
    f.commit_file(&f.repo, "binary", &[0, 2, 254]);
    let cs = conflict::merge_base_into(&f.env, &task, "main").unwrap();
    assert_eq!(cs.files[0].kind, ConflictKind::BothAdded);
    let content = conflict::read_conflict_file(&f.env, &task, "binary").unwrap();
    assert!(content.working.is_none() && content.ours.is_none() && content.theirs.is_none());
    std::fs::remove_file(task.join("binary")).unwrap();
    assert!(conflict::read_conflict_file(&f.env, &task, "binary")
        .unwrap()
        .working
        .is_none());
    assert!(
        conflict::resolve_file(&f.env, &task, "binary", ConflictResolution::MarkResolved)
            .unwrap()
            .ready_to_commit
    );
    conflict::commit_merge(&f.env, &task).unwrap();
    assert!(!task.join("binary").exists());
}

#[test]
fn clean_merge_no_op_retry_and_invalid_input() {
    let f = Fixture::new();
    let task = f.task();
    assert_eq!(
        conflict::merge_base_into(&f.env, &task, "main").unwrap(),
        ConflictState::default()
    );
    conflict::commit_merge(&f.env, &task).unwrap();
    assert!(conflict::merge_base_into(&f.env, &task, "missing").is_err());
    assert!(conflict::merge_base_into(&f.env, &task, "--help").is_err());
    f.commit_file(&f.repo, "base", b"new\n");
    let cs = conflict::merge_base_into(&f.env, &task, "main").unwrap();
    assert!(cs.merge_in_progress && cs.ready_to_commit && cs.files.is_empty());
    conflict::commit_merge(&f.env, &task).unwrap();
    let head = f.git(&task, &["rev-parse", "HEAD"]);
    std::fs::write(task.join("unrelated"), b"do not commit").unwrap();
    f.git(&task, &["add", "unrelated"]);
    conflict::commit_merge(&f.env, &task).unwrap();
    assert_eq!(head, f.git(&task, &["rev-parse", "HEAD"]));
    assert!(conflict::merge_base_into(&f.env, &task, "main").is_err());
    assert!(task.join("unrelated").exists());
}

#[test]
fn remote_base_merge_fetches_new_commit_and_pushes_result() {
    let f = Fixture::new();
    let remote = f.remote();
    let task = f.task();
    f.commit_file(&f.repo, "remote-change", b"remote\n");
    repo::push(&f.env, &f.repo, "main", false).unwrap();
    // Reset local main so only origin/main contains the change.
    f.git(&f.repo, &["reset", "--hard", "HEAD~1"]);
    let cs = conflict::merge_base_into(&f.env, &task, "main").unwrap();
    assert_eq!(cs.base_ref.as_deref(), Some("origin/main"));
    assert!(cs.ready_to_commit);
    assert!(task.join("remote-change").exists());
    conflict::commit_merge(&f.env, &task).unwrap();
    repo::push(&f.env, &task, "task", true).unwrap();
    assert_eq!(
        f.git(&remote, &["rev-parse", "task"]),
        f.git(&task, &["rev-parse", "HEAD"])
    );
}

#[test]
fn conflict_operations_reject_paths_outside_unmerged_set() {
    let f = Fixture::new();
    let task = conflicting(&f, "file");
    conflict::merge_base_into(&f.env, &task, "main").unwrap();
    for path in [
        "",
        "../file",
        "/etc/passwd",
        ":(glob)*",
        "*",
        "untracked",
        "file\0",
    ] {
        assert!(
            conflict::read_conflict_file(&f.env, &task, path).is_err(),
            "{path:?}"
        );
        assert!(
            conflict::resolve_file(&f.env, &task, path, ConflictResolution::Ours).is_err(),
            "{path:?}"
        );
    }
    // Reading a replacement symlink must not disclose its target.
    std::fs::remove_file(task.join("file")).unwrap();
    std::os::unix::fs::symlink(f.repo.join("file"), task.join("file")).unwrap();
    assert!(conflict::read_conflict_file(&f.env, &task, "file")
        .unwrap()
        .working
        .is_none());
    conflict::abort_merge(&f.env, &task).unwrap();
}

#[test]
fn unmerged_parser_and_agent_prompt() {
    let files = conflict::parse_unmerged(
        "UU a\nAA b\nDU c\nUD d\nAU e\nUA f\nDD g\n M skip\n日\nUU \"tab\\tname\"\n",
    );
    assert_eq!(files.len(), 8);
    assert_eq!(files[7].path, "tab\tname");
    assert_eq!(files[4].kind, ConflictKind::Other);
    let prompt = conflict::build_agent_prompt("origin/main", &files);
    assert!(prompt.contains("origin/main") && prompt.contains("- a"));
    assert!(prompt.contains("git add") && prompt.contains("コミットはしない"));
}
