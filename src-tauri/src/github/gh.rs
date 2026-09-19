//! gh コマンド実行（担当: WS-D）。

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::shell_env::{run, CmdOutput, ShellEnv};

use super::parse::{parse_created_pr_number, parse_pr_view};
use super::types::{MergeMethod, PullRequestStatus};
use super::PR_JSON_FIELDS;

/// `gh auth status` が成功するか。
pub fn is_authenticated(env: &ShellEnv) -> bool {
    run(env, "gh", &["auth", "status"], Path::new("/"))
        .map(|o| o.success())
        .unwrap_or(false)
}

fn gh(env: &ShellEnv, repo: &Path, args: &[&str]) -> AppResult<CmdOutput> {
    run(env, "gh", args, repo).map_err(|e| AppError::Gh(e.to_string()))
}

fn checked_stdout(output: CmdOutput, operation: &str) -> AppResult<String> {
    if output.success() {
        Ok(output.stdout)
    } else {
        let detail = if output.stderr.trim().is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        // Do not echo argv: create arguments contain the PR body.
        Err(AppError::Gh(format!(
            "gh pr {operation} 失敗 (exit {}): {}",
            output.status,
            detail.trim()
        )))
    }
}

fn validate_selector(value: &str) -> AppResult<()> {
    if value.trim().is_empty() || value.starts_with('-') || value.contains('\0') {
        return Err(AppError::InvalidInput("PR のブランチ名が不正です".into()));
    }
    Ok(())
}

/// ブランチに紐づく PR（open 優先、なければ最新）。無ければ None。
/// `gh pr view <branch> --json <PR_JSON_FIELDS>`（"no pull requests found" は None）。
pub fn pr_for_branch(
    env: &ShellEnv,
    repo: &Path,
    branch: &str,
) -> AppResult<Option<PullRequestStatus>> {
    validate_selector(branch)?;
    let out = gh(env, repo, &["pr", "view", branch, "--json", PR_JSON_FIELDS])?;
    // Only this known lookup error denotes absence. Authentication/transport errors must surface.
    if !out.success()
        && out
            .stderr
            .trim()
            .to_ascii_lowercase()
            .starts_with("no pull requests found for branch ")
    {
        return Ok(None);
    }
    parse_pr_view(&checked_stdout(out, "view")?).map(Some)
}

/// PR 番号で取得。
pub fn pr_view(env: &ShellEnv, repo: &Path, number: u64) -> AppResult<PullRequestStatus> {
    validate_number(number)?;
    let out = gh(
        env,
        repo,
        &["pr", "view", &number.to_string(), "--json", PR_JSON_FIELDS],
    )?;
    parse_pr_view(&checked_stdout(out, "view")?)
}

/// command 層でも push より前に同じ検証を行う。
pub(crate) fn validate_create_input(
    head: &str,
    base: &str,
    title: &str,
    body: &str,
) -> AppResult<()> {
    validate_selector(head)?;
    validate_selector(base)?;
    if title.trim().is_empty() || title.contains('\0') || body.contains('\0') {
        return Err(AppError::InvalidInput(
            "PR タイトルは必須です。タイトル・本文に NUL は使えません".into(),
        ));
    }
    Ok(())
}

/// `gh pr create --head <head> --base <base> --title --body [--draft]`。
/// push 済みであること（コマンド層が事前に `git::repo::push` する）。
pub fn create_pr(
    env: &ShellEnv,
    repo: &Path,
    head: &str,
    base: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> AppResult<PullRequestStatus> {
    validate_create_input(head, base, title, body)?;
    let mut args = vec![
        "pr", "create", "--head", head, "--base", base, "--title", title, "--body", body,
    ];
    if draft {
        args.push("--draft");
    }
    let stdout = checked_stdout(gh(env, repo, &args)?, "create")?;
    let number = parse_created_pr_number(&stdout)?;
    pr_view(env, repo, number).map_err(|e| AppError::Gh(format!(
        "PR #{number} は作成済みですが、状態を取得できません。再作成せず PR 状態を再取得してください: {e}"
    )))
}

fn validate_number(number: u64) -> AppResult<()> {
    if number == 0 {
        Err(AppError::InvalidInput(
            "PR 番号は 1 以上で指定してください".into(),
        ))
    } else {
        Ok(())
    }
}

/// `gh pr merge <number> --merge|--squash|--rebase`。
/// `--delete-branch` は worktree のローカルブランチ操作を伴うため使わない。
/// 成功しても merge queue 待ちの場合があるため、command 層で状態を再取得する。
pub fn merge_pr(env: &ShellEnv, repo: &Path, number: u64, method: MergeMethod) -> AppResult<()> {
    validate_number(number)?;
    let flag = match method {
        MergeMethod::Merge => "--merge",
        MergeMethod::Squash => "--squash",
        MergeMethod::Rebase => "--rebase",
    };
    checked_stdout(
        gh(env, repo, &["pr", "merge", &number.to_string(), flag])?,
        "merge",
    )?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::github::test_support::FakeCli;

    #[test]
    fn view_uses_worktree_and_exact_fields() {
        let cli = FakeCli::new();
        let pr = pr_for_branch(&cli.env, cli.repo(), "feature/pr-status")
            .unwrap()
            .unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(
            cli.calls(),
            vec![vec![
                "pr",
                "view",
                "feature/pr-status",
                "--json",
                PR_JSON_FIELDS
            ]]
        );
    }

    #[test]
    fn malformed_output_and_missing_executable_are_gh_errors() {
        let cli = FakeCli::new();
        cli.response("not JSON");
        assert_eq!(pr_view(&cli.env, cli.repo(), 42).unwrap_err().kind(), "gh");
        let empty_bin = tempfile::tempdir().unwrap();
        let env = ShellEnv {
            path: empty_bin.path().to_str().unwrap().into(),
        };
        assert_eq!(pr_view(&env, cli.repo(), 42).unwrap_err().kind(), "gh");
    }

    #[test]
    fn only_missing_branch_pr_is_none() {
        let cli = FakeCli::new();
        cli.fail(
            "view",
            "no pull requests found for branch \"feature/pr-status\"",
        );
        assert!(pr_for_branch(&cli.env, cli.repo(), "feature/pr-status")
            .unwrap()
            .is_none());
        assert_eq!(pr_view(&cli.env, cli.repo(), 42).unwrap_err().kind(), "gh");
        cli.fail("view", "HTTP 401: Bad credentials");
        let error = pr_for_branch(&cli.env, cli.repo(), "feature/pr-status").unwrap_err();
        assert_eq!(error.kind(), "gh");
        assert!(error.to_string().contains("HTTP 401"));
        cli.fail("view", "network unavailable");
        assert!(pr_for_branch(&cli.env, cli.repo(), "feature/pr-status").is_err());
    }

    #[test]
    fn create_preserves_literal_text_and_fetches_created_number() {
        for draft in [false, true] {
            let cli = FakeCli::new();
            let title = "--title with 'quotes'";
            let body = "日本語\n`code` $HOME $(touch unwanted)\nsecond line";
            let pr = create_pr(
                &cli.env,
                cli.repo(),
                "feature/pr-status",
                "main",
                title,
                body,
                draft,
            )
            .unwrap();
            assert_eq!(pr.number, 42);
            let mut create = vec![
                "pr",
                "create",
                "--head",
                "feature/pr-status",
                "--base",
                "main",
                "--title",
                title,
                "--body",
                body,
            ];
            if draft {
                create.push("--draft");
            }
            assert_eq!(
                cli.calls(),
                vec![create, vec!["pr", "view", "42", "--json", PR_JSON_FIELDS]]
            );
            assert!(!cli.repo().join("unwanted").exists());
        }
    }

    #[test]
    fn create_failures_do_not_retry_or_claim_success() {
        let cli = FakeCli::new();
        cli.fail("create", "already exists");
        assert!(create_pr(
            &cli.env,
            cli.repo(),
            "feature/pr-status",
            "main",
            "title",
            "body",
            false
        )
        .is_err());
        assert_eq!(cli.calls().len(), 1);
        let cli = FakeCli::new();
        cli.fail("view", "network unavailable");
        let error = create_pr(
            &cli.env,
            cli.repo(),
            "feature/pr-status",
            "main",
            "title",
            "body",
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("#42 は作成済み"));
        assert_eq!(cli.calls().len(), 2);
    }

    #[test]
    fn merge_methods_never_request_local_branch_deletion() {
        for (method, flag) in [
            (MergeMethod::Merge, "--merge"),
            (MergeMethod::Squash, "--squash"),
            (MergeMethod::Rebase, "--rebase"),
        ] {
            let cli = FakeCli::new();
            merge_pr(&cli.env, cli.repo(), 42, method).unwrap();
            assert_eq!(cli.calls(), vec![vec!["pr", "merge", "42", flag]]);
        }
        let cli = FakeCli::new();
        cli.fail("merge", "required check failed");
        assert_eq!(
            merge_pr(&cli.env, cli.repo(), 42, MergeMethod::Merge)
                .unwrap_err()
                .kind(),
            "gh"
        );
    }

    #[test]
    fn invalid_inputs_do_not_launch_commands() {
        let cli = FakeCli::new();
        for selector in ["", " ", "--web", "bad\0branch"] {
            assert!(pr_for_branch(&cli.env, cli.repo(), selector).is_err());
        }
        assert!(pr_view(&cli.env, cli.repo(), 0).is_err());
        assert!(merge_pr(&cli.env, cli.repo(), 0, MergeMethod::Merge).is_err());
        assert!(create_pr(&cli.env, cli.repo(), "feature", "main", " ", "", false).is_err());
        assert!(create_pr(
            &cli.env,
            cli.repo(),
            "feature",
            "main",
            "title",
            "\0",
            false
        )
        .is_err());
        assert!(cli.calls().is_empty());
    }
}
