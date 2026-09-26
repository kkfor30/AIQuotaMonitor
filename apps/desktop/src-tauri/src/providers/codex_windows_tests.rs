use super::*;
use crate::domain::refresh::{CapabilityData, RefreshError, SourceRefreshOutput};
use rusqlite::params;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const EXTRA_SOURCE: &str = "openai-codex-extra-window-regression";
const EXTRA_ACCOUNT: &str = "openai-window-regression-account";
const CAPTURED_AT: i64 = 1_700_000_000_000;

struct TempDatabaseFiles(PathBuf);

impl Drop for TempDatabaseFiles {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.0.as_os_str().to_os_string();
            path.push(suffix);
            let _ = std::fs::remove_file(PathBuf::from(path));
        }
    }
}

struct Fixture {
    database: Database,
    _files: TempDatabaseFiles,
}

impl Fixture {
    fn new() -> Self {
        let files = TempDatabaseFiles(std::env::temp_dir().join(format!(
            "aqm-codex-windows-{}-{}.db",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        let database = Database::initialize_at(files.0.clone()).unwrap();
        database.add_user_platform("openai", "GPT", None).unwrap();
        database
            .ensure_account_source_with_adapter(
                EXTRA_ACCOUNT,
                "openai",
                "additional",
                EXTRA_SOURCE,
                codex::SOURCE_ID,
                "oauth",
                "额外 Codex",
                "测试额外账号",
            )
            .unwrap();
        Self {
            database,
            _files: files,
        }
    }

    fn refresh(&self, source_id: &str, output: SourceRefreshOutput) {
        let run = self
            .database
            .begin_refresh_run("openai", &[source_id.into()])
            .unwrap();
        let generation = self.database.begin_source_refresh(source_id).unwrap();
        let source = self.database.source(source_id).unwrap();
        assert!(self
            .database
            .complete_source_refresh(&run, &source, generation, &output)
            .unwrap());
        self.database.finish_refresh_run(&run).unwrap();

        // Force every generation, including failures, to the same millisecond.
        // A timestamp comparison must neither retain removed windows nor mark failures fresh.
        let connection = self.database.connect().unwrap();
        connection
            .execute(
                "UPDATE capability_snapshots SET captured_at = ?2 WHERE source_id = ?1",
                params![source_id, CAPTURED_AT],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE sources SET last_validated_at = ?2,
                last_success_at = CASE WHEN last_success_at IS NULL THEN NULL ELSE ?2 END
             WHERE id = ?1",
                params![source_id, CAPTURED_AT],
            )
            .unwrap();
    }

    fn fail(&self, source_id: &str) {
        self.refresh(
            source_id,
            SourceRefreshOutput::failure(RefreshError::new(
                "network_error",
                "测试网络失败",
                false,
                true,
            )),
        );
    }

    fn snapshot_count(&self, source_id: &str) -> i64 {
        self.database
            .connect()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM capability_snapshots WHERE source_id = ?1",
                [source_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn assert_windows(&self, source_id: &str, expected: &[(i64, u8)], freshness: DataFreshness) {
        let platform = platform_summaries(&self.database)
            .unwrap()
            .into_iter()
            .find(|platform| platform.provider_id == "openai")
            .unwrap();
        let account_id = self.database.source(source_id).unwrap().account_id;
        let windows = platform
            .capabilities
            .iter()
            .filter(|capability| {
                capability.source_id == source_id
                    && capability.capability_id.starts_with("quota_window_")
            })
            .collect::<Vec<_>>();
        let mut expected_ids = expected
            .iter()
            .map(|(seconds, _)| window_id(*seconds))
            .collect::<Vec<_>>();
        expected_ids.sort();
        let mut actual_ids = windows
            .iter()
            .map(|window| window.capability_id.clone())
            .collect::<Vec<_>>();
        actual_ids.sort();
        assert_eq!(
            actual_ids, expected_ids,
            "window membership for {source_id}"
        );

        let mut source_ids = platform
            .sources
            .iter()
            .find(|source| source.source_id == source_id)
            .unwrap()
            .capability_ids
            .iter()
            .filter(|id| id.starts_with("quota_window_"))
            .cloned()
            .collect::<Vec<_>>();
        source_ids.sort();
        assert_eq!(source_ids, expected_ids, "source coverage for {source_id}");

        for (seconds, remaining) in expected {
            let id = window_id(*seconds);
            let window = windows
                .iter()
                .find(|window| window.capability_id == id)
                .unwrap();
            assert_eq!(window.account_id, account_id);
            assert_eq!(window.freshness, freshness, "freshness of {source_id}/{id}");
            assert_eq!(
                window.value.primary.as_deref(),
                Some(format!("{remaining}%").as_str())
            );
            assert_eq!(window.value.progress, Some(f64::from(*remaining) / 100.0));
            assert_eq!(window.captured_at, Some(CAPTURED_AT as u64));
            assert_eq!(
                window.last_good_at,
                (freshness == DataFreshness::Stale).then_some(CAPTURED_AT as u64)
            );
        }
    }
}

fn window_id(seconds: i64) -> String {
    match seconds {
        18_000 => "quota_window_5h".into(),
        604_800 => "quota_window_7d".into(),
        2_592_000 => "quota_window_30d".into(),
        _ => format!("quota_window_{seconds}s"),
    }
}

fn success(plan: &str, windows: &[(i64, u8)]) -> SourceRefreshOutput {
    let mut capabilities = vec![CapabilityData {
        capability_id: "plan_level".into(),
        display_name: "订阅计划".into(),
        value_kind: "text".into(),
        primary_value: Some(plan.into()),
        secondary_value: None,
        progress: None,
        trend: vec![],
        window_seconds: None,
        reset_at: None,
    }];
    capabilities.extend(windows.iter().map(|(seconds, remaining)| CapabilityData {
        capability_id: window_id(*seconds),
        display_name: format!("{seconds} 秒窗口"),
        value_kind: "percent".into(),
        primary_value: Some(format!("{remaining}%")),
        secondary_value: None,
        progress: Some(f64::from(*remaining) / 100.0),
        trend: vec![],
        window_seconds: Some(*seconds),
        reset_at: Some(CAPTURED_AT + *seconds * 1_000),
    }));
    SourceRefreshOutput::success(capabilities)
}

#[test]
fn codex_plan_changes_replace_windows_per_account_and_keep_only_current_cache() {
    let stages: &[(&str, &[(i64, u8)])] = &[
        ("free", &[(2_592_000, 83)]),
        ("plus", &[(18_000, 61), (604_800, 72)]),
        ("plus", &[(604_800, 44)]),
        ("free", &[(2_592_000, 19)]),
        ("free", &[]),
    ];
    for (source_id, other_source) in [
        (codex::SOURCE_ID, EXTRA_SOURCE),
        (EXTRA_SOURCE, codex::SOURCE_ID),
    ] {
        let fixture = Fixture::new();
        let other_windows = [(604_800, 37)];
        fixture.refresh(other_source, success("plus", &other_windows));
        let mut history_count = 0;
        for (plan, windows) in stages {
            fixture.refresh(source_id, success(plan, windows));
            history_count += windows.len() as i64 + 1;
            fixture.assert_windows(source_id, windows, DataFreshness::Fresh);
            fixture.assert_windows(other_source, &other_windows, DataFreshness::Fresh);
            assert_eq!(
                fixture.snapshot_count(source_id),
                history_count,
                "success retains history"
            );

            fixture.database.begin_source_refresh(source_id).unwrap();
            fixture.assert_windows(source_id, windows, DataFreshness::Stale);
            fixture.assert_windows(other_source, &other_windows, DataFreshness::Fresh);
            fixture.fail(source_id);
            fixture.assert_windows(source_id, windows, DataFreshness::Stale);
            fixture.assert_windows(other_source, &other_windows, DataFreshness::Fresh);
            assert_eq!(
                fixture.snapshot_count(source_id),
                history_count,
                "failure retains history"
            );
            assert_eq!(
                fixture.snapshot_count(other_source),
                2,
                "other account is unchanged"
            );
        }
        for id in ["quota_window_30d", "quota_window_5h", "quota_window_7d"] {
            assert!(
                fixture
                    .database
                    .latest_snapshot(source_id, id)
                    .unwrap()
                    .is_some(),
                "removed {id} remains available in history"
            );
        }
    }
}

#[test]
fn codex_dynamic_window_keeps_real_zero_and_does_not_return_after_replacement() {
    for source_id in [codex::SOURCE_ID, EXTRA_SOURCE] {
        let fixture = Fixture::new();
        fixture.refresh(source_id, success("custom", &[(259_200, 0)]));
        fixture.assert_windows(source_id, &[(259_200, 0)], DataFreshness::Fresh);
        fixture.fail(source_id);
        fixture.assert_windows(source_id, &[(259_200, 0)], DataFreshness::Stale);

        fixture.refresh(source_id, success("free", &[(2_592_000, 0)]));
        fixture.assert_windows(source_id, &[(2_592_000, 0)], DataFreshness::Fresh);
        fixture.fail(source_id);
        fixture.assert_windows(source_id, &[(2_592_000, 0)], DataFreshness::Stale);
        assert_eq!(fixture.snapshot_count(source_id), 4);
        assert_eq!(
            fixture
                .database
                .latest_snapshot(source_id, "quota_window_259200s")
                .unwrap()
                .unwrap()
                .primary_value
                .as_deref(),
            Some("0%")
        );
    }
}
