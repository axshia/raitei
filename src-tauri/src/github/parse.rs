//! `gh pr view --json` のパース（担当: WS-D）。純粋関数のみ。

use serde::Deserialize;
use serde_json::Value;

use super::types::{CheckRun, ChecksSummary, Mergeable, PrState, PullRequestStatus, Review};
use crate::error::{AppError, AppResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    url: String,
    title: String,
    state: String,
    is_draft: bool,
    head_ref_name: String,
    base_ref_name: String,
    mergeable: Option<String>,
    merge_state_status: Option<String>,
    review_decision: Option<String>,
    reviews: Option<Vec<GhReview>>,
    status_check_rollup: Option<Vec<Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReview {
    author: Option<GhAuthor>,
    state: String,
    submitted_at: Option<String>,
}

#[derive(Deserialize)]
struct GhAuthor {
    login: String,
}

/// `gh pr view --json <PR_JSON_FIELDS>` の出力を変換する。
pub fn parse_pr_view(json: &str) -> AppResult<PullRequestStatus> {
    let pr: GhPullRequest = serde_json::from_str(json)
        .map_err(|e| AppError::Gh(format!("PR の JSON を解析できません: {e}")))?;
    if pr.number == 0 {
        return Err(AppError::Gh("PR 番号が 0 です".into()));
    }
    let state = match pr.state.as_str() {
        "OPEN" => PrState::Open,
        "CLOSED" => PrState::Closed,
        "MERGED" => PrState::Merged,
        other => return Err(AppError::Gh(format!("未対応の PR state: {other}"))),
    };
    let mergeable = match pr.mergeable.as_deref() {
        Some("MERGEABLE") => Mergeable::Mergeable,
        Some("CONFLICTING") => Mergeable::Conflicting,
        _ => Mergeable::Unknown,
    };
    let merge_state_status = nonempty(pr.merge_state_status).unwrap_or_else(|| "UNKNOWN".into());
    let has_conflicts = mergeable == Mergeable::Conflicting || merge_state_status == "DIRTY";
    Ok(PullRequestStatus {
        number: pr.number,
        url: pr.url,
        title: pr.title,
        state,
        is_draft: pr.is_draft,
        head_branch: pr.head_ref_name,
        base_branch: pr.base_ref_name,
        mergeable,
        merge_state_status,
        has_conflicts,
        review_decision: nonempty(pr.review_decision),
        reviews: pr
            .reviews
            .unwrap_or_default()
            .into_iter()
            .map(|r| Review {
                author: r.author.map(|a| a.login).unwrap_or_else(|| "ghost".into()),
                state: r.state,
                submitted_at: nonempty(r.submitted_at),
            })
            .collect(),
        checks: summarize_checks(&Value::Array(pr.status_check_rollup.unwrap_or_default())),
    })
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn is_context(check: &Value) -> bool {
    string(check, "__typename") == "StatusContext"
        || (string(check, "__typename").is_empty()
            && check.get("status").is_none()
            && check.get("state").is_some())
}

fn check_status(check: &Value) -> &'static str {
    if is_context(check) {
        return match string(check, "state") {
            "EXPECTED" | "PENDING" => "pending",
            "SUCCESS" => "success",
            "FAILURE" | "ERROR" => "failure",
            _ => "unknown",
        };
    }
    match string(check, "__typename") {
        "CheckRun" | "" => {}
        _ => return "unknown",
    }
    match string(check, "status") {
        "QUEUED" | "IN_PROGRESS" | "PENDING" | "WAITING" | "REQUESTED" => "pending",
        "COMPLETED" => match string(check, "conclusion") {
            "SUCCESS" => "success",
            "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE" | "STALE" => "failure",
            "NEUTRAL" => "neutral",
            "SKIPPED" => "skipped",
            "CANCELLED" => "cancelled",
            _ => "unknown",
        },
        _ => "unknown",
    }
}

/// CheckRun / StatusContext を集計する。
/// neutral/skipped は passed、cancelled は failed、unknown は pending に含める。
/// 個別の status は保持し、未判定を成功表示せず total = passed + failed + pending を保つ。
pub fn summarize_checks(rollup: &Value) -> ChecksSummary {
    let mut summary = ChecksSummary::default();
    if let Some(checks) = rollup.as_array() {
        for check in checks {
            let status = check_status(check);
            match status {
                "success" | "neutral" | "skipped" => summary.passed += 1,
                "failure" | "cancelled" => summary.failed += 1,
                _ => summary.pending += 1,
            }
            let context = is_context(check);
            let name = string(check, if context { "context" } else { "name" });
            summary.checks.push(CheckRun {
                name: if name.is_empty() {
                    "unknown".into()
                } else {
                    name.into()
                },
                status: status.into(),
                url: nonempty(Some(
                    string(check, if context { "targetUrl" } else { "detailsUrl" }).into(),
                )),
            });
            summary.total += 1;
        }
    }
    summary
}

/// `gh pr create` の標準出力に含まれる PR URL から番号を取り出す。
pub(super) fn parse_created_pr_number(stdout: &str) -> AppResult<u64> {
    stdout.lines().rev().find_map(|line| {
        let url = line.trim();
        let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
        let parts: Vec<_> = rest.trim_end_matches('/').split('/').collect();
        if parts.len() != 5 || parts[..3].iter().any(|s| s.is_empty()) || parts[3] != "pull" {
            return None;
        }
        parts[4].parse::<u64>().ok().filter(|n| *n > 0)
    }).ok_or_else(|| AppError::Gh("PR 作成は成功しましたが、出力から PR 番号を取得できません。PR 状態を再取得してください".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const MIXED: &str = include_str!("fixtures/gh_pr_view_mixed.json");
    const EMPTY: &str = include_str!("fixtures/gh_pr_view_empty.json");

    #[test]
    fn parses_pr_reviews_and_mixed_checks() {
        let pr = parse_pr_view(MIXED).unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "PR 状態の表示");
        assert_eq!(pr.url, "https://github.example/team/project/pull/42");
        assert_eq!(pr.state, PrState::Open);
        assert_eq!(pr.head_branch, "feature/pr-status");
        assert_eq!(pr.base_branch, "main");
        assert!(!pr.is_draft);
        assert_eq!(pr.mergeable, Mergeable::Conflicting);
        assert!(pr.has_conflicts);
        assert_eq!(pr.review_decision.as_deref(), Some("CHANGES_REQUESTED"));
        assert_eq!(pr.reviews[0].author, "reviewer");
        assert_eq!(pr.reviews[0].state, "CHANGES_REQUESTED");
        assert_eq!(
            pr.reviews[0].submitted_at.as_deref(),
            Some("2026-09-19T00:00:00Z")
        );
        assert_eq!(pr.reviews[1].author, "ghost");
        assert_eq!(pr.reviews[1].submitted_at, None);
        assert_eq!(
            (
                pr.checks.total,
                pr.checks.passed,
                pr.checks.failed,
                pr.checks.pending
            ),
            (7, 3, 2, 2)
        );
        assert_eq!(
            pr.checks
                .checks
                .iter()
                .map(|c| c.status.as_str())
                .collect::<Vec<_>>(),
            [
                "success",
                "pending",
                "neutral",
                "skipped",
                "cancelled",
                "failure",
                "pending"
            ]
        );
        assert_eq!(
            pr.checks.checks[0].url.as_deref(),
            Some("https://github.example/team/project/actions/runs/1")
        );
        assert_eq!(pr.checks.checks[5].name, "external CI");
        assert_eq!(
            pr.checks.checks[5].url.as_deref(),
            Some("https://ci.example/42")
        );
        assert_eq!(pr.checks.checks[6].url, None);
    }

    #[test]
    fn null_and_empty_optional_fields_are_supported() {
        let pr = parse_pr_view(EMPTY).unwrap();
        assert!(pr.is_draft);
        assert_eq!(pr.mergeable, Mergeable::Unknown);
        assert_eq!(pr.review_decision, None);
        assert!(pr.reviews.is_empty());
        assert_eq!(pr.checks, ChecksSummary::default());
        assert!(!pr.has_conflicts);
        let mut value: Value = serde_json::from_str(EMPTY).unwrap();
        for field in ["mergeable", "mergeStateStatus", "reviewDecision", "reviews"] {
            value[field] = Value::Null;
        }
        let pr = parse_pr_view(&value.to_string()).unwrap();
        assert_eq!(pr.merge_state_status, "UNKNOWN");
        assert!(pr.reviews.is_empty());
    }

    #[test]
    fn maps_states_and_both_conflict_signals() {
        let mut value: Value = serde_json::from_str(EMPTY).unwrap();
        for (state, expected) in [
            ("OPEN", PrState::Open),
            ("CLOSED", PrState::Closed),
            ("MERGED", PrState::Merged),
        ] {
            value["state"] = json!(state);
            assert_eq!(parse_pr_view(&value.to_string()).unwrap().state, expected);
        }
        for (mergeable, status, conflicts) in [
            ("MERGEABLE", "CLEAN", false),
            ("UNKNOWN", "DIRTY", true),
            ("CONFLICTING", "UNKNOWN", true),
            ("FUTURE", "BLOCKED", false),
        ] {
            value["mergeable"] = json!(mergeable);
            value["mergeStateStatus"] = json!(status);
            assert_eq!(
                parse_pr_view(&value.to_string()).unwrap().has_conflicts,
                conflicts
            );
        }
    }

    #[test]
    fn normalizes_checkrun_conclusions_and_incomplete_statuses() {
        for (conclusion, expected) in [
            ("SUCCESS", "success"),
            ("FAILURE", "failure"),
            ("TIMED_OUT", "failure"),
            ("ACTION_REQUIRED", "failure"),
            ("STARTUP_FAILURE", "failure"),
            ("STALE", "failure"),
            ("NEUTRAL", "neutral"),
            ("SKIPPED", "skipped"),
            ("CANCELLED", "cancelled"),
            ("", "unknown"),
            ("FUTURE", "unknown"),
        ] {
            let summary = summarize_checks(
                &json!([{"__typename":"CheckRun", "status":"COMPLETED", "conclusion":conclusion}]),
            );
            assert_eq!(summary.checks[0].status, expected, "{conclusion}");
            assert_eq!(
                summary.total,
                summary.passed + summary.failed + summary.pending
            );
        }
        for status in ["QUEUED", "IN_PROGRESS", "PENDING", "WAITING", "REQUESTED"] {
            assert_eq!(
                check_status(&json!({"status":status,"conclusion":"SUCCESS"})),
                "pending"
            );
        }
    }

    #[test]
    fn normalizes_status_contexts_and_unknown_checks_conservatively() {
        for (state, expected) in [
            ("SUCCESS", "success"),
            ("PENDING", "pending"),
            ("EXPECTED", "pending"),
            ("FAILURE", "failure"),
            ("ERROR", "failure"),
            ("FUTURE", "unknown"),
        ] {
            assert_eq!(
                check_status(&json!({"__typename":"StatusContext","state":state})),
                expected
            );
        }
        let summary = summarize_checks(
            &json!([null, {}, {"__typename":"FutureCheck", "state":"SUCCESS"},
            {"status":"COMPLETED", "conclusion":null}]),
        );
        assert_eq!(summary.pending, 4);
        assert!(summary.checks.iter().all(|c| c.status == "unknown"));
        assert_eq!(summarize_checks(&Value::Null), ChecksSummary::default());
        assert_eq!(summarize_checks(&json!([])), ChecksSummary::default());
    }

    #[test]
    fn malformed_pr_data_is_a_gh_error() {
        for input in ["not json", "null", "[]", "{}"] {
            assert_eq!(parse_pr_view(input).unwrap_err().kind(), "gh");
        }
        for (key, bad) in [
            ("number", json!(0)),
            ("state", json!("FUTURE")),
            ("reviews", json!({})),
            ("statusCheckRollup", json!({})),
            ("isDraft", json!("false")),
        ] {
            let mut value: Value = serde_json::from_str(EMPTY).unwrap();
            value[key] = bad;
            assert!(parse_pr_view(&value.to_string()).is_err(), "{key}");
        }
    }

    #[test]
    fn parses_create_url_without_confusing_other_output_for_a_pr() {
        assert_eq!(
            parse_created_pr_number("https://github.com/team/project/pull/42\n").unwrap(),
            42
        );
        assert_eq!(
            parse_created_pr_number("notice\nhttps://github.example/team/project/pull/123/\n")
                .unwrap(),
            123
        );
        for output in [
            "",
            "42",
            "https://github.com/team/project/issues/42",
            "https://github.com/team/project/pull/0",
            "https://github.com/team/project/pull/nope",
            "https://github.com/team/project/pull/42/files",
        ] {
            assert!(parse_created_pr_number(output).is_err(), "{output}");
        }
    }
}
