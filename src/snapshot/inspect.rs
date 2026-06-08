use crate::prelude::*;

pub(crate) async fn snapshot_report(
    runtime: &Runtime,
    options: SnapshotReportOptions,
) -> Result<Value> {
    if !options.dir.exists() {
        return err(format!("{} does not exist", options.dir.display()));
    }

    let (verify, verify_ok) = if options.stats.verify {
        snapshot_report_component(verify_snapshot(&options.dir))
    } else {
        (
            json!({
                "ok": true,
                "skipped": true,
                "reason": "--skip-verify was set",
            }),
            true,
        )
    };
    let (stats, stats_ok) =
        snapshot_report_component(snapshot_stats(&options.dir, options.stats.clone()));
    let (doctor, doctor_ok) =
        snapshot_report_component(snapshot_doctor(&options.dir, options.doctor.clone()));
    let (neighbors, neighbors_ok) = snapshot_report_component(snapshot_report_neighbors(&options));
    let (drift, drift_ok) = if options.include_drift {
        snapshot_report_component(snapshot_drift(runtime, options.drift.clone()).await)
    } else {
        (
            json!({
                "ok": true,
                "included": false,
                "reason": "run with --drift or --full-contact-ids to compare live me.sh data",
            }),
            true,
        )
    };

    let summary = snapshot_report_summary(
        SnapshotReportStatus {
            verify_ok,
            stats_ok,
            doctor_ok,
            neighbors_ok,
            drift_included: options.include_drift,
            drift_ok,
        },
        &neighbors,
        &drift,
    );
    let ok = summary.get("ok").and_then(Value::as_bool).unwrap_or(false);

    Ok(json!({
        "ok": ok,
        "snapshot": options.dir.display().to_string(),
        "generated_at_unix_ms": now_millis(),
        "summary": summary,
        "verify": verify,
        "stats": stats,
        "doctor": doctor,
        "neighbors": neighbors,
        "drift": drift,
    }))
}

pub(crate) fn snapshot_report_component(result: Result<Value>) -> (Value, bool) {
    match result {
        Ok(value) => {
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
            (value, ok)
        }
        Err(error) => (
            json!({
                "ok": false,
                "error": error.to_string(),
            }),
            false,
        ),
    }
}

pub(crate) fn snapshot_report_summary(
    status: SnapshotReportStatus,
    neighbors: &Value,
    drift: &Value,
) -> Value {
    let neighbor_count = neighbors
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let neighbor_failures = neighbors
        .get("failed")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let drift_changed = drift
        .get("summary")
        .and_then(|summary| summary.get("changed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let drift_full_contacts_requested = drift
        .get("live")
        .and_then(|live| live.get("full_contacts_requested"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let drift_full_contacts_compared = drift_full_contacts_requested == 0
        || drift
            .get("full_contacts")
            .and_then(|full_contacts| full_contacts.get("compared"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let effective_drift_ok = status.drift_ok && drift_full_contacts_compared;
    let ok = status.verify_ok
        && status.stats_ok
        && status.doctor_ok
        && status.neighbors_ok
        && effective_drift_ok;
    json!({
        "ok": ok,
        "verify_ok": status.verify_ok,
        "stats_ok": status.stats_ok,
        "doctor_ok": status.doctor_ok,
        "neighbors_ok": status.neighbors_ok,
        "neighbor_count": neighbor_count,
        "neighbor_failures": neighbor_failures,
        "drift_included": status.drift_included,
        "drift_ok": effective_drift_ok,
        "drift_command_ok": status.drift_ok,
        "drift_changed": drift_changed,
        "drift_full_contacts_requested": drift_full_contacts_requested,
        "drift_full_contacts_compared": drift_full_contacts_compared,
    })
}

pub(crate) fn snapshot_report_neighbors(options: &SnapshotReportOptions) -> Result<Value> {
    if options.neighbors == 0 {
        return Ok(json!({
            "ok": true,
            "included": false,
            "reason": "--neighbors 0 was set",
            "items": [],
        }));
    }
    if !options.root.exists() {
        return Ok(json!({
            "ok": false,
            "included": true,
            "root": options.root.display().to_string(),
            "error": "neighbor root does not exist",
            "items": [],
        }));
    }

    let timeline_options = SnapshotTimelineOptions {
        root: options.root.clone(),
        recursive: options.recursive,
        max_depth: options.max_depth,
        limit: options.limit,
        changes_only: false,
        diffs: false,
        diff: options.diff,
    };
    let (snapshots, discovery_errors) = snapshot_timeline_snapshots(&timeline_options)?;
    let Some(position) = snapshot_report_find_position(&snapshots, &options.dir)? else {
        return Ok(json!({
            "ok": false,
            "included": true,
            "root": options.root.display().to_string(),
            "recursive": options.recursive,
            "max_depth": options.max_depth,
            "limit": options.limit,
            "error": "snapshot was not found under neighbor root",
            "snapshots": snapshots.len(),
            "discovery_errors": discovery_errors,
            "items": [],
        }));
    };

    let mut items = Vec::new();
    let previous_start = position.saturating_sub(options.neighbors);
    for neighbor_index in previous_start..position {
        let distance = position - neighbor_index;
        items.push(snapshot_report_neighbor_item(
            "previous",
            distance,
            &snapshots[neighbor_index],
            &snapshots[position],
            &snapshots[neighbor_index],
            options,
        )?);
    }
    let next_end = snapshots.len().min(position + options.neighbors + 1);
    for neighbor_index in (position + 1)..next_end {
        let distance = neighbor_index - position;
        items.push(snapshot_report_neighbor_item(
            "next",
            distance,
            &snapshots[position],
            &snapshots[neighbor_index],
            &snapshots[neighbor_index],
            options,
        )?);
    }

    let failed = items
        .iter()
        .filter(|item| !item.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count()
        + discovery_errors.len();
    Ok(json!({
        "ok": failed == 0,
        "included": true,
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "limit": options.limit,
        "neighbors": options.neighbors,
        "position": position,
        "snapshots": snapshots.len(),
        "discovery_errors": discovery_errors,
        "failed": failed,
        "items": items,
    }))
}

pub(crate) fn snapshot_report_find_position(
    snapshots: &[SnapshotHistorySnapshot],
    dir: &Path,
) -> Result<Option<usize>> {
    let target = fs::canonicalize(dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("canonicalizing {}", dir.display()))?;
    for (index, snapshot) in snapshots.iter().enumerate() {
        let Ok(candidate) = fs::canonicalize(&snapshot.dir) else {
            continue;
        };
        if candidate == target {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

pub(crate) fn snapshot_report_neighbor_item(
    relation: &str,
    distance: usize,
    old: &SnapshotHistorySnapshot,
    new: &SnapshotHistorySnapshot,
    neighbor: &SnapshotHistorySnapshot,
    options: &SnapshotReportOptions,
) -> Result<Value> {
    let diff = diff_snapshots(&old.dir, &new.dir, options.diff)?;
    let ok = diff.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let summary = if ok {
        snapshot_timeline_diff_summary(&diff)
    } else {
        Value::Null
    };
    let changed = summary
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut item = json!({
        "relation": relation,
        "distance": distance,
        "ok": ok,
        "changed": changed,
        "neighbor": snapshot_report_snapshot_value(neighbor),
        "old": snapshot_report_snapshot_value(old),
        "new": snapshot_report_snapshot_value(new),
        "summary": summary,
    });
    if let Value::Object(object) = &mut item {
        if !ok && let Some(error) = diff.get("error") {
            object.insert("error".to_string(), error.clone());
        }
        if options.diffs {
            object.insert("diff".to_string(), diff);
        }
    }
    Ok(item)
}

pub(crate) fn snapshot_report_snapshot_value(snapshot: &SnapshotHistorySnapshot) -> Value {
    json!({
        "dir": snapshot.dir.display().to_string(),
        "created_at_unix_ms": snapshot.created_at_unix_ms,
        "rank_time_unix_ms": snapshot.rank_time_unix_ms,
        "rank_time_source": snapshot.rank_time_source,
    })
}

pub(crate) fn snapshot_report_table_rows(report: &Value) -> Value {
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let snapshot = report
        .get("snapshot")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let doctor_summary = report
        .get("doctor")
        .and_then(|doctor| doctor.get("summary"))
        .unwrap_or(&Value::Null);
    let stats_sections = report
        .get("stats")
        .and_then(|stats| stats.get("sections"))
        .and_then(Value::as_object)
        .map(Map::len)
        .unwrap_or_default();
    let drift_live = report
        .get("drift")
        .and_then(|drift| drift.get("live"))
        .unwrap_or(&Value::Null);

    Value::Array(vec![
        json!({
            "component": "overall",
            "status": snapshot_report_status(summary_bool(summary, "ok")),
            "detail": snapshot,
        }),
        json!({
            "component": "verify",
            "status": snapshot_report_status(summary_bool(summary, "verify_ok")),
            "detail": "manifest hashes",
        }),
        json!({
            "component": "stats",
            "status": snapshot_report_status(summary_bool(summary, "stats_ok")),
            "detail": format!("{stats_sections} sections"),
        }),
        json!({
            "component": "doctor",
            "status": snapshot_report_status(summary_bool(summary, "doctor_ok")),
            "detail": format!(
                "pass={} warn={} fail={}",
                value_u64(doctor_summary, "pass"),
                value_u64(doctor_summary, "warn"),
                value_u64(doctor_summary, "fail"),
            ),
        }),
        json!({
            "component": "neighbors",
            "status": snapshot_report_status(summary_bool(summary, "neighbors_ok")),
            "detail": format!(
                "items={} failures={}",
                value_u64(summary, "neighbor_count"),
                value_u64(summary, "neighbor_failures"),
            ),
        }),
        json!({
            "component": "drift",
            "status": if summary_bool(summary, "drift_included") {
                snapshot_report_status(summary_bool(summary, "drift_ok"))
            } else {
                "skipped"
            },
            "detail": if summary_bool(summary, "drift_included") {
                format!(
                    "changed={} contacts={} groups={} full_requested={} full_compared={}",
                    summary_bool(summary, "drift_changed"),
                    value_u64(drift_live, "contacts"),
                    value_u64(drift_live, "groups"),
                    value_u64(summary, "drift_full_contacts_requested"),
                    summary_bool(summary, "drift_full_contacts_compared"),
                )
            } else {
                "not requested".to_string()
            },
        }),
    ])
}

pub(crate) fn snapshot_report_status(ok: bool) -> &'static str {
    if ok { "ok" } else { "fail" }
}

pub(crate) async fn snapshot_drift(
    runtime: &Runtime,
    options: SnapshotDriftOptions,
) -> Result<Value> {
    if !options.dir.exists() {
        return err(format!("{} does not exist", options.dir.display()));
    }

    let snapshot_verify = if options.verify {
        let verify = verify_snapshot(&options.dir)?;
        if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return Ok(json!({
                "ok": false,
                "error": "snapshot failed manifest verification",
                "snapshot": options.dir.display().to_string(),
                "snapshot_verify": verify,
            }));
        }
        verify
    } else {
        json!({
            "skipped": true,
            "reason": "--skip-verify was set",
        })
    };

    let manifest = read_snapshot_manifest(&options.dir)?;
    let old_contacts_path = snapshot_manifest_file_path(&options.dir, "contacts")?;
    let old_contacts = read_snapshot_jsonl_records_at_path(&old_contacts_path)?;
    let (live_contacts, live_contact_values, live_contact_count) =
        live_contact_records(runtime, options.page_size, options.diff.details).await?;
    let mut contacts = record_diff(&old_contacts, &live_contacts);
    if options.diff.details {
        let old_contact_values = read_snapshot_jsonl_record_values_at_path(&old_contacts_path)?;
        contacts = add_record_diff_details(
            contacts,
            &old_contact_values,
            &live_contact_values,
            options.diff.detail_limit,
        )?;
    }

    let (groups, live_group_count) = if options.compare_groups {
        let old_groups_path = snapshot_manifest_file_path(&options.dir, "groups")?;
        let old_groups = read_snapshot_array_records_at_path(&old_groups_path)?;
        let (live_groups, live_group_values, live_group_count) =
            live_group_records(runtime, options.diff.details).await?;
        let mut groups = record_diff(&old_groups, &live_groups);
        if options.diff.details {
            let old_group_values = read_snapshot_array_record_values_at_path(&old_groups_path)?;
            groups = add_record_diff_details(
                groups,
                &old_group_values,
                &live_group_values,
                options.diff.detail_limit,
            )?;
        }
        (groups, Some(live_group_count))
    } else {
        (
            json!({
                "compared": false,
                "reason": "--skip-groups was set",
            }),
            None,
        )
    };

    let full_contacts = snapshot_drift_full_contacts(runtime, &options).await?;
    let summary_source = json!({
        "contacts": contacts,
        "groups": groups,
        "full_contacts": full_contacts,
    });
    let summary = snapshot_timeline_diff_summary(&summary_source);

    Ok(json!({
        "ok": true,
        "snapshot": options.dir.display().to_string(),
        "snapshot_verify": snapshot_verify,
        "snapshot_counts": manifest.get("counts").cloned().unwrap_or(Value::Null),
        "live": {
            "contacts": live_contact_count,
            "groups": live_group_count,
            "full_contacts_requested": options.full_contact_ids.len(),
        },
        "page_size": options.page_size,
        "details": options.diff.details,
        "detail_limit": options.diff.detail_limit,
        "summary": summary,
        "contacts": summary_source.get("contacts").cloned().unwrap_or(Value::Null),
        "groups": summary_source.get("groups").cloned().unwrap_or(Value::Null),
        "full_contacts": summary_source.get("full_contacts").cloned().unwrap_or(Value::Null),
    }))
}

pub(crate) async fn snapshot_drift_full_contacts(
    runtime: &Runtime,
    options: &SnapshotDriftOptions,
) -> Result<Value> {
    if options.full_contact_ids.is_empty() {
        return Ok(json!({
            "compared": false,
            "reason": "no --full-contact-ids were requested",
        }));
    }

    if !snapshot_manifest_has_file(&options.dir, "full_contacts")? {
        return Ok(json!({
            "compared": false,
            "snapshot_available": false,
            "reason": "snapshot does not contain full_contacts. Recreate it with --full-contact-ids or --full-contacts.",
            "requested_ids": options.full_contact_ids,
        }));
    }

    let old_path = snapshot_manifest_file_path(&options.dir, "full_contacts")?;
    let old_records = read_snapshot_jsonl_records_at_path(&old_path)?;
    let old_records = filter_records_by_ids(&old_records, &options.full_contact_ids);
    let (live_records, live_values) = live_full_contact_records(
        runtime,
        &options.full_contact_ids,
        options.full_concurrency,
        options.diff.details,
    )
    .await?;
    let mut diff = record_diff(&old_records, &live_records);
    if options.diff.details {
        let old_values = read_snapshot_jsonl_record_values_at_path(&old_path)?;
        let old_values = filter_records_by_ids(&old_values, &options.full_contact_ids);
        diff = add_record_diff_details(diff, &old_values, &live_values, options.diff.detail_limit)?;
    }
    if let Value::Object(object) = &mut diff {
        object.set("compared", true);
        object.set("snapshot_available", true);
        object.insert(
            "requested_ids".to_string(),
            Value::Array(
                options
                    .full_contact_ids
                    .iter()
                    .copied()
                    .map(|id| Value::Number(Number::from(id)))
                    .collect(),
            ),
        );
    }
    Ok(diff)
}

pub(crate) fn snapshot_catalog(options: SnapshotCatalogOptions) -> Result<Value> {
    if !options.root.exists() {
        return err(format!("{} does not exist", options.root.display()));
    }

    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovery_errors = Vec::new();
    snapshot_catalog_discover(
        &options.root,
        0,
        &options,
        &mut seen,
        &mut candidates,
        &mut discovery_errors,
    )?;
    candidates.sort_by(|left, right| {
        snapshot_catalog_candidate_key(left).cmp(&snapshot_catalog_candidate_key(right))
    });

    let items = candidates
        .iter()
        .map(|candidate| match candidate.kind {
            SnapshotCatalogCandidateKind::Snapshot => {
                snapshot_catalog_snapshot_item(&candidate.path, &options)
            }
            SnapshotCatalogCandidateKind::Archive => {
                snapshot_catalog_archive_item(&candidate.path, &options)
            }
        })
        .collect::<Vec<_>>();
    let summary = snapshot_catalog_summary(&items, &discovery_errors);
    let ok = summary
        .get("failures")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
        && summary
            .get("errors")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            == 0;

    Ok(json!({
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "limit": options.limit,
        "include_snapshots": options.include_snapshots,
        "include_archives": options.include_archives,
        "verified": options.verify || options.doctor,
        "doctor": options.doctor,
        "require_index": options.require_index,
        "ok": ok,
        "summary": summary,
        "errors": discovery_errors,
        "items": items,
    }))
}

pub(crate) fn snapshot_catalog_discover(
    path: &Path,
    depth: usize,
    options: &SnapshotCatalogOptions,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<SnapshotCatalogCandidate>,
    errors: &mut Vec<Value>,
) -> Result<()> {
    if snapshot_catalog_limit_reached(candidates.len(), options.limit) {
        return Ok(());
    }

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            errors.push(snapshot_catalog_discovery_error(path, error));
            return Ok(());
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(());
    }
    if file_type.is_file() {
        if options.include_archives && snapshot_catalog_is_archive(path) {
            snapshot_catalog_push_candidate(
                SnapshotCatalogCandidateKind::Archive,
                path,
                seen,
                candidates,
                options,
            );
        }
        return Ok(());
    }
    if !file_type.is_dir() {
        return Ok(());
    }

    if path.join("manifest.json").is_file() {
        if options.include_snapshots {
            snapshot_catalog_push_candidate(
                SnapshotCatalogCandidateKind::Snapshot,
                path,
                seen,
                candidates,
                options,
            );
        }
        return Ok(());
    }
    if depth > 0 && !options.recursive {
        return Ok(());
    }
    if options
        .max_depth
        .is_some_and(|max_depth| depth >= max_depth)
    {
        return Ok(());
    }

    let mut children = match fs::read_dir(path) {
        Ok(children) => children
            .collect::<std::result::Result<Vec<_>, _>>()
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display()))?,
        Err(error) => {
            errors.push(snapshot_catalog_discovery_error(path, error));
            return Ok(());
        }
    };
    children.sort_by_key(|entry| entry.path());
    for child in children {
        if snapshot_catalog_limit_reached(candidates.len(), options.limit) {
            break;
        }
        snapshot_catalog_discover(&child.path(), depth + 1, options, seen, candidates, errors)?;
    }
    Ok(())
}

pub(crate) fn snapshot_catalog_limit_reached(count: usize, limit: Option<usize>) -> bool {
    limit.is_some_and(|limit| count >= limit)
}

pub(crate) fn snapshot_catalog_push_candidate(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
    seen: &mut BTreeSet<String>,
    candidates: &mut Vec<SnapshotCatalogCandidate>,
    options: &SnapshotCatalogOptions,
) {
    if snapshot_catalog_limit_reached(candidates.len(), options.limit) {
        return;
    }
    let key = format!("{}:{}", kind.as_str(), path.display());
    if seen.insert(key) {
        candidates.push(SnapshotCatalogCandidate {
            kind,
            path: path.to_path_buf(),
        });
    }
}

pub(crate) fn snapshot_catalog_candidate_key(candidate: &SnapshotCatalogCandidate) -> String {
    format!("{}:{}", candidate.kind.as_str(), candidate.path.display())
}

pub(crate) fn snapshot_catalog_discovery_error(path: &Path, error: impl ToString) -> Value {
    json!({
        "path": path.display().to_string(),
        "error": error.to_string(),
    })
}

pub(crate) fn snapshot_catalog_snapshot_item(
    dir: &Path,
    options: &SnapshotCatalogOptions,
) -> Value {
    let manifest = match read_snapshot_manifest(dir) {
        Ok(manifest) => manifest,
        Err(error) => {
            return snapshot_catalog_error_item(
                SnapshotCatalogCandidateKind::Snapshot,
                dir,
                "snapshot manifest could not be read",
                error,
            );
        }
    };
    let mut item = snapshot_catalog_base_item(SnapshotCatalogCandidateKind::Snapshot, dir);
    snapshot_catalog_add_manifest_summary(&mut item, &manifest);

    if options.doctor {
        match snapshot_doctor(
            dir,
            SnapshotDoctorOptions {
                top: SNAPSHOT_STATS_TOP_DEFAULT,
                verify: true,
                require_index: options.require_index,
            },
        ) {
            Ok(report) => snapshot_catalog_add_report(&mut item, "doctor", report),
            Err(error) => {
                return snapshot_catalog_error_item(
                    SnapshotCatalogCandidateKind::Snapshot,
                    dir,
                    "snapshot doctor could not complete",
                    error,
                );
            }
        }
    } else if options.verify {
        match verify_snapshot(dir) {
            Ok(report) => snapshot_catalog_add_report(&mut item, "verify", report),
            Err(error) => {
                return snapshot_catalog_error_item(
                    SnapshotCatalogCandidateKind::Snapshot,
                    dir,
                    "snapshot verification could not complete",
                    error,
                );
            }
        }
    } else {
        item.set("status", json!("found"));
        item.set("ok", Value::Null);
    }

    Value::Object(item)
}

pub(crate) fn snapshot_catalog_base_item(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
) -> Map<String, Value> {
    let mut item = Map::new();
    item.set("kind", json!(kind.as_str()));
    item.set("path", json!(path.display().to_string()));
    item
}

pub(crate) fn snapshot_catalog_error_item(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
    message: &str,
    error: impl ToString,
) -> Value {
    let mut item = snapshot_catalog_base_item(kind, path);
    item.set("status", json!("error"));
    item.set("ok", json!(false));
    item.set("message", json!(message));
    item.set("error", json!(error.to_string()));
    Value::Object(item)
}

pub(crate) fn snapshot_catalog_add_manifest_summary(
    item: &mut Map<String, Value>,
    manifest: &Value,
) {
    item.insert(
        "schema".to_string(),
        manifest.get("schema").cloned().unwrap_or(Value::Null),
    );
    item.insert(
        "meshx_version".to_string(),
        manifest
            .get("meshx_version")
            .cloned()
            .unwrap_or(Value::Null),
    );
    item.insert(
        "created_at_unix_ms".to_string(),
        manifest
            .get("created_at_unix_ms")
            .cloned()
            .unwrap_or(Value::Null),
    );
    item.insert(
        "counts".to_string(),
        manifest.get("counts").cloned().unwrap_or(Value::Null),
    );
    let files = manifest.get("files").and_then(Value::as_object);
    item.insert(
        "file_count".to_string(),
        json!(files.map(Map::len).unwrap_or_default()),
    );
    item.insert(
        "snapshot_bytes".to_string(),
        json!(snapshot_catalog_manifest_bytes(files)),
    );
}

pub(crate) fn snapshot_catalog_manifest_bytes(files: Option<&Map<String, Value>>) -> u64 {
    files
        .into_iter()
        .flat_map(Map::values)
        .filter_map(|entry| entry.get("bytes").and_then(Value::as_u64))
        .sum()
}

pub(crate) fn snapshot_catalog_add_report(item: &mut Map<String, Value>, key: &str, report: Value) {
    let (status, ok) = snapshot_catalog_report_status(&report);
    item.set("status", json!(status));
    item.set("ok", json!(ok));
    item.insert(key.to_string(), report);
}

pub(crate) fn snapshot_catalog_report_status(report: &Value) -> (&'static str, bool) {
    let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return ("fail", false);
    }
    let warnings = report
        .get("summary")
        .and_then(|summary| summary.get("warnings").or_else(|| summary.get("warn")))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    if warnings > 0 {
        ("warn", true)
    } else {
        ("pass", true)
    }
}

pub(crate) fn snapshot_catalog_summary(items: &[Value], discovery_errors: &[Value]) -> Value {
    let mut snapshots = 0_u64;
    let mut archives = 0_u64;
    let mut pass = 0_u64;
    let mut warn = 0_u64;
    let mut fail = 0_u64;
    let mut found = 0_u64;
    let mut errors = discovery_errors.len() as u64;

    for item in items {
        match item.get("kind").and_then(Value::as_str) {
            Some("snapshot") => snapshots += 1,
            Some("archive") => archives += 1,
            _ => {}
        }
        match item.get("status").and_then(Value::as_str) {
            Some("pass") => pass += 1,
            Some("warn") => warn += 1,
            Some("fail") => fail += 1,
            Some("error") => {
                fail += 1;
                errors += 1;
            }
            Some("found") | None => found += 1,
            Some(_) => warn += 1,
        }
    }

    json!({
        "total": items.len(),
        "snapshots": snapshots,
        "archives": archives,
        "found": found,
        "pass": pass,
        "warn": warn,
        "fail": fail,
        "errors": errors,
        "failures": fail,
        "warnings": warn,
    })
}

pub(crate) struct SnapshotPrunePlan {
    pub(crate) actions: Vec<SnapshotPruneAction>,
    pub(crate) discovery_errors: Vec<Value>,
}

pub(crate) fn snapshot_prune(options: SnapshotPruneOptions) -> Result<Value> {
    if !options.root.exists() {
        return err(format!("{} does not exist", options.root.display()));
    }

    let plan = snapshot_prune_plan_actions(&options)?;
    let mut actions = plan.actions;
    let discovery_errors = plan.discovery_errors;
    if !discovery_errors.is_empty() && !options.dry_run {
        return err("snapshot:prune discovery failed. Re-run with --dry-run to inspect errors.");
    }
    let planned_deletes = actions.iter().filter(|action| action.delete).count();
    if planned_deletes > 0 && !options.dry_run && !options.yes {
        return err("snapshot:prune is destructive. Re-run with --yes, or use --dry-run.");
    }

    let mut delete_errors = Vec::new();
    let mut deleted = BTreeSet::new();
    if !options.dry_run {
        for action in actions.iter().filter(|action| action.delete) {
            match snapshot_prune_delete_action(action) {
                Ok(()) => {
                    deleted.insert(action.path.display().to_string());
                }
                Err(error) => delete_errors.push(json!({
                    "kind": action.kind.as_str(),
                    "path": action.path.display().to_string(),
                    "error": error.to_string(),
                })),
            }
        }
    }

    let action_values = actions
        .drain(..)
        .map(|action| snapshot_prune_action_value(action, &deleted, options.dry_run))
        .collect::<Vec<_>>();
    let summary = snapshot_prune_summary(&action_values, &delete_errors, &discovery_errors);
    let action_errors = summary
        .get("action_errors")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let ok = delete_errors.is_empty() && discovery_errors.is_empty() && action_errors == 0;

    Ok(json!({
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "limit": options.limit,
        "include_snapshots": options.include_snapshots,
        "include_archives": options.include_archives,
        "criteria": snapshot_prune_criteria_value(&options),
        "dry_run": options.dry_run,
        "confirmed": options.yes,
        "ok": ok,
        "summary": summary,
        "delete_errors": delete_errors,
        "discovery_errors": discovery_errors,
        "actions": action_values,
    }))
}

pub(crate) fn snapshot_prune_plan_actions(
    options: &SnapshotPruneOptions,
) -> Result<SnapshotPrunePlan> {
    let catalog_options = SnapshotCatalogOptions {
        root: options.root.clone(),
        recursive: options.recursive,
        max_depth: options.max_depth,
        limit: options.limit,
        include_snapshots: options.include_snapshots,
        include_archives: options.include_archives,
        verify: false,
        doctor: false,
        require_index: false,
    };
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovery_errors = Vec::new();
    snapshot_catalog_discover(
        &catalog_options.root,
        0,
        &catalog_options,
        &mut seen,
        &mut candidates,
        &mut discovery_errors,
    )?;
    candidates.sort_by(|left, right| {
        snapshot_catalog_candidate_key(left).cmp(&snapshot_catalog_candidate_key(right))
    });

    let cutoff = snapshot_prune_cutoff(options.older_than_days)?;
    let mut actions = candidates
        .into_iter()
        .map(|candidate| snapshot_prune_action(candidate, options, cutoff))
        .collect::<Result<Vec<_>>>()?;
    snapshot_prune_apply_keep_latest(&mut actions, options.keep_latest);
    Ok(SnapshotPrunePlan {
        actions,
        discovery_errors,
    })
}

pub(crate) fn snapshot_prune_action(
    candidate: SnapshotCatalogCandidate,
    options: &SnapshotPruneOptions,
    cutoff: Option<u64>,
) -> Result<SnapshotPruneAction> {
    let (rank_time_unix_ms, rank_time_source) =
        snapshot_prune_rank_time(candidate.kind, &candidate.path)?;
    let mut error = None;
    let bytes = match snapshot_prune_candidate_bytes(candidate.kind, &candidate.path) {
        Ok(bytes) => bytes,
        Err(byte_error) => {
            error = Some(byte_error.to_string());
            None
        }
    };
    let mut reasons = Vec::new();
    if let Some(cutoff) = cutoff
        && rank_time_unix_ms.is_some_and(|rank_time| rank_time < cutoff)
    {
        reasons.push("older-than-days".to_string());
    }

    let mut health = None;
    if options.failed {
        match snapshot_prune_health(candidate.kind, &candidate.path, options.require_index) {
            Ok(report) => {
                if !report.get("ok").and_then(Value::as_bool).unwrap_or(false) {
                    reasons.push("failed".to_string());
                }
                health = Some(report);
            }
            Err(health_error) => {
                reasons.push("failed".to_string());
                error = Some(health_error.to_string());
            }
        }
    }

    Ok(SnapshotPruneAction {
        kind: candidate.kind,
        path: candidate.path,
        delete: !reasons.is_empty(),
        reasons,
        rank_time_unix_ms,
        rank_time_source,
        bytes,
        health,
        error,
    })
}

pub(crate) fn snapshot_prune_apply_keep_latest(
    actions: &mut [SnapshotPruneAction],
    keep_latest: Option<usize>,
) {
    let Some(keep_latest) = keep_latest else {
        return;
    };
    for kind in [
        SnapshotCatalogCandidateKind::Snapshot,
        SnapshotCatalogCandidateKind::Archive,
    ] {
        let mut indexes = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| action.kind == kind)
            .map(|(index, action)| {
                (
                    action.rank_time_unix_ms.unwrap_or_default(),
                    action.path.display().to_string(),
                    index,
                )
            })
            .collect::<Vec<_>>();
        indexes.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        for (_, _, index) in indexes.into_iter().skip(keep_latest) {
            actions[index].reasons.push("keep-latest".to_string());
            actions[index].delete = true;
        }
    }
}

pub(crate) fn snapshot_prune_cutoff(older_than_days: Option<usize>) -> Result<Option<u64>> {
    let Some(days) = older_than_days else {
        return Ok(None);
    };
    let millis = (days as u64)
        .checked_mul(24)
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_mul(60))
        .and_then(|value| value.checked_mul(1000))
        .ok_or_else(|| miette!("--older-than-days is too large"))?;
    Ok(Some(now_millis().saturating_sub(millis)))
}

pub(crate) fn snapshot_prune_rank_time(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
) -> Result<(Option<u64>, &'static str)> {
    if kind == SnapshotCatalogCandidateKind::Snapshot
        && let Ok(manifest) = read_snapshot_manifest(path)
        && let Some(created_at) = manifest.get("created_at_unix_ms").and_then(Value::as_u64)
    {
        return Ok((Some(created_at), "manifest.created_at_unix_ms"));
    }
    let metadata = fs::metadata(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let modified = metadata
        .modified()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading mtime for {}", path.display()))?;
    Ok((Some(system_time_unix_ms(modified)?), "filesystem.modified"))
}

pub(crate) fn snapshot_prune_candidate_bytes(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
) -> Result<Option<u64>> {
    match kind {
        SnapshotCatalogCandidateKind::Archive => Ok(Some(
            fs::metadata(path)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", path.display()))?
                .len(),
        )),
        SnapshotCatalogCandidateKind::Snapshot => {
            let manifest = read_snapshot_manifest(path)?;
            let files = manifest.get("files").and_then(Value::as_object);
            Ok(Some(snapshot_catalog_manifest_bytes(files)))
        }
    }
}

pub(crate) fn snapshot_prune_health(
    kind: SnapshotCatalogCandidateKind,
    path: &Path,
    require_index: bool,
) -> Result<Value> {
    match kind {
        SnapshotCatalogCandidateKind::Snapshot if require_index => snapshot_doctor(
            path,
            SnapshotDoctorOptions {
                top: SNAPSHOT_STATS_TOP_DEFAULT,
                verify: true,
                require_index: true,
            },
        ),
        SnapshotCatalogCandidateKind::Snapshot => verify_snapshot(path),
        SnapshotCatalogCandidateKind::Archive => {
            snapshot_verify_archive(path, SnapshotVerifyArchiveOptions { require_index })
        }
    }
}

pub(crate) fn snapshot_prune_delete_action(action: &SnapshotPruneAction) -> Result<()> {
    match action.kind {
        SnapshotCatalogCandidateKind::Snapshot => {
            let metadata = fs::symlink_metadata(&action.path)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", action.path.display()))?;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || !action.path.join("manifest.json").is_file()
            {
                return err(format!(
                    "{} is no longer a snapshot directory",
                    action.path.display()
                ));
            }
            fs::remove_dir_all(&action.path)
                .into_diagnostic()
                .wrap_err_with(|| format!("deleting {}", action.path.display()))
        }
        SnapshotCatalogCandidateKind::Archive => {
            let metadata = fs::symlink_metadata(&action.path)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", action.path.display()))?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || !snapshot_catalog_is_archive(&action.path)
            {
                return err(format!(
                    "{} is no longer a snapshot archive",
                    action.path.display()
                ));
            }
            fs::remove_file(&action.path)
                .into_diagnostic()
                .wrap_err_with(|| format!("deleting {}", action.path.display()))
        }
    }
}

pub(crate) fn snapshot_prune_action_value(
    action: SnapshotPruneAction,
    deleted: &BTreeSet<String>,
    dry_run: bool,
) -> Value {
    let path = action.path.display().to_string();
    let status = if action.delete {
        if dry_run {
            "would-delete"
        } else if deleted.contains(&path) {
            "deleted"
        } else {
            "delete-failed"
        }
    } else {
        "keep"
    };
    let mut value = json!({
        "kind": action.kind.as_str(),
        "path": path,
        "action": if action.delete { "delete" } else { "keep" },
        "status": status,
        "reasons": action.reasons,
        "rank_time_unix_ms": action.rank_time_unix_ms,
        "rank_time_source": action.rank_time_source,
        "bytes": action.bytes,
    });
    if let Value::Object(object) = &mut value {
        if let Some(health) = action.health {
            object.insert("health".to_string(), snapshot_prune_health_value(&health));
        }
        if let Some(error) = action.error {
            object.set("error", json!(error));
        }
    }
    value
}

pub(crate) fn snapshot_prune_health_value(report: &Value) -> Value {
    let mut value = Map::new();
    value.insert(
        "ok".to_string(),
        report.get("ok").cloned().unwrap_or(Value::Null),
    );
    if let Some(summary) = report.get("summary") {
        value.insert("summary".to_string(), summary.clone());
    }
    if let Some(failures) = report.get("failures") {
        value.insert("failures".to_string(), failures.clone());
    }
    let failing_checks = report
        .get("checks")
        .and_then(Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .filter(|check| check.get("status").and_then(Value::as_str) == Some("fail"))
                .filter_map(|check| check.get("name").and_then(Value::as_str))
                .map(|name| Value::String(name.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !failing_checks.is_empty() {
        value.set("failing_checks", Value::Array(failing_checks));
    }
    Value::Object(value)
}

pub(crate) fn snapshot_prune_criteria_value(options: &SnapshotPruneOptions) -> Value {
    json!({
        "keep_latest": options.keep_latest,
        "older_than_days": options.older_than_days,
        "failed": options.failed,
        "require_index": options.require_index,
    })
}

pub(crate) fn snapshot_prune_summary(
    actions: &[Value],
    delete_errors: &[Value],
    discovery_errors: &[Value],
) -> Value {
    let mut snapshots = 0_u64;
    let mut archives = 0_u64;
    let mut keep = 0_u64;
    let mut planned_delete = 0_u64;
    let mut deleted = 0_u64;
    let mut action_errors = 0_u64;
    let mut bytes = 0_u64;

    for action in actions {
        match action.get("kind").and_then(Value::as_str) {
            Some("snapshot") => snapshots += 1,
            Some("archive") => archives += 1,
            _ => {}
        }
        if action.get("action").and_then(Value::as_str) == Some("delete") {
            planned_delete += 1;
            bytes = bytes.saturating_add(
                action
                    .get("bytes")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            );
        } else {
            keep += 1;
        }
        if action.get("status").and_then(Value::as_str) == Some("deleted") {
            deleted += 1;
        }
        if action.get("error").is_some() {
            action_errors += 1;
        }
    }

    json!({
        "total": actions.len(),
        "snapshots": snapshots,
        "archives": archives,
        "keep": keep,
        "planned_delete": planned_delete,
        "deleted": deleted,
        "action_errors": action_errors,
        "delete_errors": delete_errors.len(),
        "discovery_errors": discovery_errors.len(),
        "planned_bytes": bytes,
    })
}

pub(crate) fn snapshot_history(options: SnapshotHistoryOptions) -> Result<Value> {
    if !options.root.exists() {
        return err(format!("{} does not exist", options.root.display()));
    }

    let (snapshots, discovery_errors) = snapshot_history_snapshots(&options)?;
    let ids = options
        .ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut states = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                SnapshotHistoryState {
                    seen: false,
                    present: false,
                    hash: None,
                    record: None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut items = Vec::new();

    for snapshot in &snapshots {
        match snapshot_history_records(&snapshot.dir, &options) {
            Ok(records) => {
                for id in &ids {
                    let record = records.get(id);
                    let state = states.get_mut(id).expect("initialized above");
                    items.push(snapshot_history_observation(
                        id, snapshot, record, state, &options,
                    )?);
                }
            }
            Err(error) => {
                for id in &ids {
                    items.push(json!({
                        "id": id,
                        "section": options.section.label,
                        "snapshot": snapshot.dir.display().to_string(),
                        "created_at_unix_ms": snapshot.created_at_unix_ms,
                        "rank_time_unix_ms": snapshot.rank_time_unix_ms,
                        "rank_time_source": snapshot.rank_time_source,
                        "status": "error",
                        "error": error.to_string(),
                    }));
                }
            }
        }
    }

    let summary = snapshot_history_summary(&items, snapshots.len(), ids.len());
    let ok = summary
        .get("error")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
        && discovery_errors.is_empty();
    Ok(json!({
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "limit": options.limit,
        "section": options.section.label,
        "ids": ids,
        "verified": options.verify,
        "index": options.index.as_str(),
        "details": options.details,
        "detail_limit": options.detail_limit,
        "records": options.records,
        "ok": ok,
        "summary": summary,
        "discovery_errors": discovery_errors,
        "items": items,
    }))
}

pub(crate) fn snapshot_history_snapshots(
    options: &SnapshotHistoryOptions,
) -> Result<(Vec<SnapshotHistorySnapshot>, Vec<Value>)> {
    let catalog_options = SnapshotCatalogOptions {
        root: options.root.clone(),
        recursive: options.recursive,
        max_depth: options.max_depth,
        limit: options.limit,
        include_snapshots: true,
        include_archives: false,
        verify: false,
        doctor: false,
        require_index: false,
    };
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovery_errors = Vec::new();
    snapshot_catalog_discover(
        &catalog_options.root,
        0,
        &catalog_options,
        &mut seen,
        &mut candidates,
        &mut discovery_errors,
    )?;
    let mut snapshots = candidates
        .into_iter()
        .filter(|candidate| candidate.kind == SnapshotCatalogCandidateKind::Snapshot)
        .map(|candidate| snapshot_history_snapshot(candidate.path))
        .collect::<Result<Vec<_>>>()?;
    snapshots.sort_by(|left, right| {
        left.rank_time_unix_ms
            .unwrap_or_default()
            .cmp(&right.rank_time_unix_ms.unwrap_or_default())
            .then_with(|| left.dir.cmp(&right.dir))
    });
    Ok((snapshots, discovery_errors))
}

pub(crate) fn snapshot_history_snapshot(dir: PathBuf) -> Result<SnapshotHistorySnapshot> {
    let manifest = read_snapshot_manifest(&dir)?;
    let created_at_unix_ms = manifest.get("created_at_unix_ms").and_then(Value::as_u64);
    if let Some(created_at_unix_ms) = created_at_unix_ms {
        return Ok(SnapshotHistorySnapshot {
            dir,
            created_at_unix_ms: Some(created_at_unix_ms),
            rank_time_unix_ms: Some(created_at_unix_ms),
            rank_time_source: "manifest.created_at_unix_ms",
        });
    }
    let metadata = fs::metadata(&dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", dir.display()))?;
    let modified = metadata
        .modified()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading mtime for {}", dir.display()))?;
    Ok(SnapshotHistorySnapshot {
        dir,
        created_at_unix_ms: None,
        rank_time_unix_ms: Some(system_time_unix_ms(modified)?),
        rank_time_source: "filesystem.modified",
    })
}

pub(crate) fn snapshot_history_records(
    dir: &Path,
    options: &SnapshotHistoryOptions,
) -> Result<BTreeMap<String, Value>> {
    let query_options = SnapshotQueryOptions {
        section: options.section,
        ids: options.ids.clone(),
        contains: None,
        limit: None,
        verify: options.verify,
        index: options.index,
    };
    match options.section.kind {
        SnapshotQuerySectionKind::Jsonl => {
            prepare_snapshot_query(dir, &query_options)?;
            let mut records = BTreeMap::new();
            let path = snapshot_manifest_file_path(dir, options.section.file_label)?;
            snapshot_query_jsonl_each(dir, &query_options, |row| {
                if let Some(id) = record_id(&row) {
                    insert_snapshot_record(
                        &mut records,
                        id,
                        row,
                        &path,
                        "selected row".to_string(),
                    )?;
                }
                Ok(())
            })?;
            Ok(records)
        }
        SnapshotQuerySectionKind::JsonArray => {
            let rows = query_snapshot(dir, query_options)?;
            let path = snapshot_manifest_file_path(dir, options.section.file_label)?;
            let mut records = BTreeMap::new();
            for (index, row) in rows.as_array().into_iter().flatten().enumerate() {
                if let Some(id) = record_id(row) {
                    insert_snapshot_record(
                        &mut records,
                        id,
                        row.clone(),
                        &path,
                        format!("selected row {}", index + 1),
                    )?;
                }
            }
            Ok(records)
        }
    }
}

pub(crate) fn snapshot_history_observation(
    id: &str,
    snapshot: &SnapshotHistorySnapshot,
    record: Option<&Value>,
    state: &mut SnapshotHistoryState,
    options: &SnapshotHistoryOptions,
) -> Result<Value> {
    let current_hash = record.map(record_hash).transpose()?;
    let previous_hash = state.hash.clone();
    let status = match (
        state.seen,
        state.present,
        record.is_some(),
        &previous_hash,
        &current_hash,
    ) {
        (false, _, true, _, _) => "present",
        (false, _, false, _, _) => "missing",
        (true, true, false, _, _) => "removed",
        (true, false, true, _, _) => "added",
        (true, true, true, Some(previous), Some(current)) if previous == current => "unchanged",
        (true, true, true, _, _) => "changed",
        (true, false, false, _, _) => "missing",
    };
    let previous_record = state.record.clone();
    let mut value = json!({
        "id": id,
        "section": options.section.label,
        "snapshot": snapshot.dir.display().to_string(),
        "created_at_unix_ms": snapshot.created_at_unix_ms,
        "rank_time_unix_ms": snapshot.rank_time_unix_ms,
        "rank_time_source": snapshot.rank_time_source,
        "status": status,
        "hash": current_hash,
        "previous_hash": previous_hash,
    });
    if let Value::Object(object) = &mut value {
        if let Some(record) = record {
            object.insert(
                "summary".to_string(),
                snapshot_history_record_summary(options.section, record),
            );
            if options.records {
                object.insert("record".to_string(), record.clone());
            }
        }
        if options.details
            && status == "changed"
            && let (Some(previous), Some(current)) = (previous_record.as_ref(), record)
        {
            let mut changes = Vec::new();
            collect_value_changes(
                previous,
                current,
                "",
                &mut changes,
                options.detail_limit.saturating_add(1),
            );
            let changes_truncated = changes.len() > options.detail_limit;
            changes.truncate(options.detail_limit);
            object.set("changes", Value::Array(changes));
            object.set("changes_truncated", json!(changes_truncated));
        }
    }

    state.seen = true;
    state.present = record.is_some();
    state.hash = current_hash;
    state.record = record.cloned();
    Ok(value)
}

pub(crate) fn snapshot_history_record_summary(
    section: SnapshotQuerySection,
    record: &Value,
) -> Value {
    if matches!(section.label, "contacts" | "full_contacts") {
        return dedupe_contact_summary(record);
    }
    let mut summary = Map::new();
    if let Some(id) = record_id(record) {
        summary.set("id", id);
    }
    if let Some(name) =
        first_contact_string(record, &["name", "displayName", "display_name", "title"])
    {
        summary.set("name", name);
    }
    Value::Object(summary)
}

pub(crate) fn snapshot_history_summary(items: &[Value], snapshots: usize, ids: usize) -> Value {
    let mut counts = BTreeMap::<String, u64>::new();
    for item in items {
        let status = item
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *counts.entry(status).or_default() += 1;
    }
    let mut summary = Map::new();
    summary.set("snapshots", json!(snapshots));
    summary.set("ids", json!(ids));
    summary.set("observations", json!(items.len()));
    for status in [
        "present",
        "added",
        "removed",
        "changed",
        "unchanged",
        "missing",
        "error",
    ] {
        summary.insert(
            status.to_string(),
            json!(counts.remove(status).unwrap_or_default()),
        );
    }
    for (status, count) in counts {
        summary.insert(status, json!(count));
    }
    Value::Object(summary)
}

pub(crate) fn snapshot_find(options: SnapshotFindOptions) -> Result<Value> {
    if !options.root.exists() {
        return err(format!("{} does not exist", options.root.display()));
    }

    let (snapshots, discovery_errors) = snapshot_find_snapshots(&options)?;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    let mut searched_sections = 0_u64;
    let mut skipped_sections = 0_u64;

    'snapshots: for snapshot in &snapshots {
        if options.verify {
            match verify_snapshot(&snapshot.dir) {
                Ok(verify) if verify.get("ok").and_then(Value::as_bool).unwrap_or(false) => {}
                Ok(_) => {
                    errors.push(snapshot_find_error(
                        snapshot,
                        None,
                        "snapshot failed manifest verification",
                    ));
                    continue;
                }
                Err(error) => {
                    errors.push(snapshot_find_error(snapshot, None, error.to_string()));
                    continue;
                }
            }
        }

        for section in &options.sections {
            if options.limit.is_some_and(|limit| items.len() >= limit) {
                break 'snapshots;
            }
            let has_section = match snapshot_manifest_has_file(&snapshot.dir, section.file_label) {
                Ok(value) => value,
                Err(error) => {
                    errors.push(snapshot_find_error(
                        snapshot,
                        Some(*section),
                        error.to_string(),
                    ));
                    continue;
                }
            };
            if !has_section {
                skipped_sections += 1;
                continue;
            }
            searched_sections += 1;
            let remaining = options
                .limit
                .map(|limit| limit.saturating_sub(items.len()))
                .filter(|remaining| *remaining > 0);
            let query_options = SnapshotQueryOptions {
                section: *section,
                ids: options.ids.clone(),
                contains: options.contains.clone(),
                limit: remaining,
                verify: false,
                index: options.index,
            };
            match snapshot_find_section(snapshot, &query_options, &options) {
                Ok(mut matches) => items.append(&mut matches),
                Err(error) => errors.push(snapshot_find_error(
                    snapshot,
                    Some(*section),
                    error.to_string(),
                )),
            }
        }
    }

    let summary = snapshot_find_summary(
        &items,
        snapshots.len(),
        searched_sections,
        skipped_sections,
        errors.len(),
        discovery_errors.len(),
    );
    let section_labels = options
        .sections
        .iter()
        .map(|section| section.label)
        .collect::<Vec<_>>();
    let ids = options
        .ids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let ok = errors.is_empty() && discovery_errors.is_empty();
    Ok(json!({
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "snapshot_limit": options.snapshot_limit,
        "sections": section_labels,
        "ids": ids,
        "contains": options.contains,
        "limit": options.limit,
        "verified": options.verify,
        "index": options.index.as_str(),
        "records": options.records,
        "ok": ok,
        "summary": summary,
        "discovery_errors": discovery_errors,
        "errors": errors,
        "items": items,
    }))
}

pub(crate) fn snapshot_find_snapshots(
    options: &SnapshotFindOptions,
) -> Result<(Vec<SnapshotHistorySnapshot>, Vec<Value>)> {
    let catalog_options = SnapshotCatalogOptions {
        root: options.root.clone(),
        recursive: options.recursive,
        max_depth: options.max_depth,
        limit: options.snapshot_limit,
        include_snapshots: true,
        include_archives: false,
        verify: false,
        doctor: false,
        require_index: false,
    };
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovery_errors = Vec::new();
    snapshot_catalog_discover(
        &catalog_options.root,
        0,
        &catalog_options,
        &mut seen,
        &mut candidates,
        &mut discovery_errors,
    )?;
    let mut snapshots = candidates
        .into_iter()
        .filter(|candidate| candidate.kind == SnapshotCatalogCandidateKind::Snapshot)
        .map(|candidate| snapshot_history_snapshot(candidate.path))
        .collect::<Result<Vec<_>>>()?;
    snapshots.sort_by(|left, right| {
        left.rank_time_unix_ms
            .unwrap_or_default()
            .cmp(&right.rank_time_unix_ms.unwrap_or_default())
            .then_with(|| left.dir.cmp(&right.dir))
    });
    Ok((snapshots, discovery_errors))
}

pub(crate) fn snapshot_find_section(
    snapshot: &SnapshotHistorySnapshot,
    query_options: &SnapshotQueryOptions,
    options: &SnapshotFindOptions,
) -> Result<Vec<Value>> {
    let mut matches = Vec::new();
    match query_options.section.kind {
        SnapshotQuerySectionKind::Jsonl => {
            snapshot_query_jsonl_each(&snapshot.dir, query_options, |row| {
                matches.push(snapshot_find_item(
                    snapshot,
                    query_options.section,
                    &row,
                    options,
                )?);
                Ok(())
            })?;
        }
        SnapshotQuerySectionKind::JsonArray => {
            let rows = read_snapshot_section_values(&snapshot.dir, query_options.section)?;
            for row in filter_snapshot_query_rows(rows, query_options)? {
                matches.push(snapshot_find_item(
                    snapshot,
                    query_options.section,
                    &row,
                    options,
                )?);
            }
        }
    }
    Ok(matches)
}

pub(crate) fn snapshot_find_item(
    snapshot: &SnapshotHistorySnapshot,
    section: SnapshotQuerySection,
    record: &Value,
    options: &SnapshotFindOptions,
) -> Result<Value> {
    let mut item = Map::new();
    item.insert(
        "snapshot".to_string(),
        Value::String(snapshot.dir.display().to_string()),
    );
    item.insert(
        "section".to_string(),
        Value::String(section.label.to_string()),
    );
    item.insert(
        "created_at_unix_ms".to_string(),
        snapshot
            .created_at_unix_ms
            .map(|value| Value::Number(Number::from(value)))
            .unwrap_or(Value::Null),
    );
    item.insert(
        "rank_time_unix_ms".to_string(),
        snapshot
            .rank_time_unix_ms
            .map(|value| Value::Number(Number::from(value)))
            .unwrap_or(Value::Null),
    );
    item.insert(
        "rank_time_source".to_string(),
        Value::String(snapshot.rank_time_source.to_string()),
    );
    item.insert(
        "id".to_string(),
        record_id(record).map(Value::String).unwrap_or(Value::Null),
    );
    item.set("hash", record_hash(record)?);
    item.insert(
        "summary".to_string(),
        snapshot_history_record_summary(section, record),
    );
    if let Some(contains) = &options.contains {
        item.insert(
            "field_matches".to_string(),
            Value::Array(snapshot_find_field_matches(record, contains, 5)),
        );
    }
    if options.records {
        item.insert("record".to_string(), record.clone());
    }
    Ok(Value::Object(item))
}

pub(crate) fn snapshot_find_field_matches(
    record: &Value,
    contains: &str,
    limit: usize,
) -> Vec<Value> {
    let needle = contains.to_lowercase();
    let mut matches = Vec::new();
    collect_snapshot_find_field_matches(record, "", &needle, &mut matches, limit);
    matches
}

pub(crate) fn collect_snapshot_find_field_matches(
    value: &Value,
    path: &str,
    needle: &str,
    matches: &mut Vec<Value>,
    limit: usize,
) {
    if matches.len() >= limit {
        return;
    }
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if matches.len() >= limit {
                    break;
                }
                collect_snapshot_find_field_matches(
                    child,
                    &json_pointer_child(path, key),
                    needle,
                    matches,
                    limit,
                );
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                if matches.len() >= limit {
                    break;
                }
                collect_snapshot_find_field_matches(
                    child,
                    &json_pointer_child(path, &index.to_string()),
                    needle,
                    matches,
                    limit,
                );
            }
        }
        Value::Null => {}
        Value::String(text) => {
            if text.to_lowercase().contains(needle) {
                matches.push(json!({
                    "path": if path.is_empty() { "/" } else { path },
                    "value": diff_preview_value(value),
                }));
            }
        }
        Value::Bool(_) | Value::Number(_) => {
            let text = cell_string(value);
            if text.to_lowercase().contains(needle) {
                matches.push(json!({
                    "path": if path.is_empty() { "/" } else { path },
                    "value": diff_preview_value(value),
                }));
            }
        }
    }
}

pub(crate) fn snapshot_find_error(
    snapshot: &SnapshotHistorySnapshot,
    section: Option<SnapshotQuerySection>,
    error: impl ToString,
) -> Value {
    json!({
        "snapshot": snapshot.dir.display().to_string(),
        "created_at_unix_ms": snapshot.created_at_unix_ms,
        "rank_time_unix_ms": snapshot.rank_time_unix_ms,
        "rank_time_source": snapshot.rank_time_source,
        "section": section.map(|section| section.label),
        "error": error.to_string(),
    })
}

pub(crate) fn snapshot_find_summary(
    items: &[Value],
    snapshots: usize,
    searched_sections: u64,
    skipped_sections: u64,
    errors: usize,
    discovery_errors: usize,
) -> Value {
    let mut snapshots_with_matches = BTreeSet::new();
    let mut section_counts = BTreeMap::<String, u64>::new();
    for item in items {
        if let Some(snapshot) = item.get("snapshot").and_then(Value::as_str) {
            snapshots_with_matches.insert(snapshot.to_string());
        }
        if let Some(section) = item.get("section").and_then(Value::as_str) {
            *section_counts.entry(section.to_string()).or_default() += 1;
        }
    }
    json!({
        "snapshots": snapshots,
        "snapshots_with_matches": snapshots_with_matches.len(),
        "searched_sections": searched_sections,
        "skipped_sections": skipped_sections,
        "matches": items.len(),
        "errors": errors,
        "discovery_errors": discovery_errors,
        "sections": section_counts,
    })
}

pub(crate) fn snapshot_timeline(options: SnapshotTimelineOptions) -> Result<Value> {
    if !options.root.exists() {
        return err(format!("{} does not exist", options.root.display()));
    }

    let (snapshots, discovery_errors) = snapshot_timeline_snapshots(&options)?;
    let total_pairs = snapshots.len().saturating_sub(1);
    let mut items = Vec::new();

    for (index, pair) in snapshots.windows(2).enumerate() {
        let old = &pair[0];
        let new = &pair[1];
        let item = snapshot_timeline_pair(index, old, new, &options)?;
        let changed = item
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let ok = item.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if options.changes_only && ok && !changed {
            continue;
        }
        items.push(item);
    }

    let summary = snapshot_timeline_summary(&items, snapshots.len(), total_pairs);
    let ok = summary
        .get("failed_pairs")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        == 0
        && discovery_errors.is_empty();

    Ok(json!({
        "root": options.root.display().to_string(),
        "recursive": options.recursive,
        "max_depth": options.max_depth,
        "limit": options.limit,
        "changes_only": options.changes_only,
        "diffs": options.diffs,
        "details": options.diff.details,
        "detail_limit": options.diff.detail_limit,
        "ok": ok,
        "summary": summary,
        "discovery_errors": discovery_errors,
        "items": items,
    }))
}

pub(crate) fn snapshot_timeline_snapshots(
    options: &SnapshotTimelineOptions,
) -> Result<(Vec<SnapshotHistorySnapshot>, Vec<Value>)> {
    let catalog_options = SnapshotCatalogOptions {
        root: options.root.clone(),
        recursive: options.recursive,
        max_depth: options.max_depth,
        limit: options.limit,
        include_snapshots: true,
        include_archives: false,
        verify: false,
        doctor: false,
        require_index: false,
    };
    let mut candidates = Vec::new();
    let mut seen = BTreeSet::new();
    let mut discovery_errors = Vec::new();
    snapshot_catalog_discover(
        &catalog_options.root,
        0,
        &catalog_options,
        &mut seen,
        &mut candidates,
        &mut discovery_errors,
    )?;
    let mut snapshots = candidates
        .into_iter()
        .filter(|candidate| candidate.kind == SnapshotCatalogCandidateKind::Snapshot)
        .map(|candidate| snapshot_history_snapshot(candidate.path))
        .collect::<Result<Vec<_>>>()?;
    snapshots.sort_by(|left, right| {
        left.rank_time_unix_ms
            .unwrap_or_default()
            .cmp(&right.rank_time_unix_ms.unwrap_or_default())
            .then_with(|| left.dir.cmp(&right.dir))
    });
    Ok((snapshots, discovery_errors))
}

pub(crate) fn snapshot_timeline_pair(
    index: usize,
    old: &SnapshotHistorySnapshot,
    new: &SnapshotHistorySnapshot,
    options: &SnapshotTimelineOptions,
) -> Result<Value> {
    let diff = diff_snapshots(&old.dir, &new.dir, options.diff)?;
    let ok = diff.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let summary = if ok {
        snapshot_timeline_diff_summary(&diff)
    } else {
        Value::Null
    };
    let changed = summary
        .get("changed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut item = json!({
        "pair": index,
        "ok": ok,
        "changed": changed,
        "old": old.dir.display().to_string(),
        "new": new.dir.display().to_string(),
        "old_created_at_unix_ms": old.created_at_unix_ms,
        "new_created_at_unix_ms": new.created_at_unix_ms,
        "old_rank_time_unix_ms": old.rank_time_unix_ms,
        "new_rank_time_unix_ms": new.rank_time_unix_ms,
        "old_rank_time_source": old.rank_time_source,
        "new_rank_time_source": new.rank_time_source,
        "summary": summary,
    });
    if let Value::Object(object) = &mut item {
        if !ok && let Some(error) = diff.get("error") {
            object.insert("error".to_string(), error.clone());
        }
        if options.diffs {
            object.insert("diff".to_string(), diff);
        }
    }
    Ok(item)
}

pub(crate) fn snapshot_timeline_diff_summary(diff: &Value) -> Value {
    let contacts_added = diff_section_count(diff, "contacts", "added_count");
    let contacts_removed = diff_section_count(diff, "contacts", "removed_count");
    let contacts_changed = diff_section_count(diff, "contacts", "changed_count");
    let groups_added = diff_section_count(diff, "groups", "added_count");
    let groups_removed = diff_section_count(diff, "groups", "removed_count");
    let groups_changed = diff_section_count(diff, "groups", "changed_count");
    let full_contacts_added = diff_section_count(diff, "full_contacts", "added_count");
    let full_contacts_removed = diff_section_count(diff, "full_contacts", "removed_count");
    let full_contacts_changed = diff_section_count(diff, "full_contacts", "changed_count");
    let moments_changed = snapshot_timeline_moments_changed(diff);
    let added = contacts_added + groups_added + full_contacts_added;
    let removed = contacts_removed + groups_removed + full_contacts_removed;
    let changed_records = contacts_changed + groups_changed + full_contacts_changed;
    json!({
        "changed": added > 0 || removed > 0 || changed_records > 0 || moments_changed > 0,
        "added": added,
        "removed": removed,
        "changed_records": changed_records,
        "contacts_added": contacts_added,
        "contacts_removed": contacts_removed,
        "contacts_changed": contacts_changed,
        "groups_added": groups_added,
        "groups_removed": groups_removed,
        "groups_changed": groups_changed,
        "full_contacts_added": full_contacts_added,
        "full_contacts_removed": full_contacts_removed,
        "full_contacts_changed": full_contacts_changed,
        "moments_changed": moments_changed,
    })
}

pub(crate) fn snapshot_timeline_summary(
    items: &[Value],
    snapshots: usize,
    total_pairs: usize,
) -> Value {
    let emitted_pairs = items.len();
    let failed_pairs = items
        .iter()
        .filter(|item| !item.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let changed_pairs = items
        .iter()
        .filter(|item| {
            item.get("changed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let emitted_unchanged_pairs = emitted_pairs
        .saturating_sub(failed_pairs)
        .saturating_sub(changed_pairs);
    let mut added = 0_u64;
    let mut removed = 0_u64;
    let mut changed_records = 0_u64;
    let mut moments_changed = 0_u64;
    for item in items {
        let Some(summary) = item.get("summary") else {
            continue;
        };
        added += summary
            .get("added")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        removed += summary
            .get("removed")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        changed_records += summary
            .get("changed_records")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        moments_changed += summary
            .get("moments_changed")
            .and_then(Value::as_u64)
            .unwrap_or_default();
    }
    json!({
        "snapshots": snapshots,
        "total_pairs": total_pairs,
        "emitted_pairs": emitted_pairs,
        "changed_pairs": changed_pairs,
        "unchanged_pairs": total_pairs.saturating_sub(changed_pairs).saturating_sub(failed_pairs),
        "emitted_unchanged_pairs": emitted_unchanged_pairs,
        "failed_pairs": failed_pairs,
        "added": added,
        "removed": removed,
        "changed_records": changed_records,
        "moments_changed": moments_changed,
    })
}

pub(crate) fn snapshot_doctor(dir: &Path, options: SnapshotDoctorOptions) -> Result<Value> {
    let mut checks = Vec::new();
    let mut verified = false;
    let mut verify = Value::Null;

    if options.verify {
        match verify_snapshot(dir) {
            Ok(value) => {
                let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
                verified = ok;
                let checked = value
                    .get("checked")
                    .and_then(Value::as_array)
                    .map(Vec::len)
                    .unwrap_or_default();
                let failures = value.get("failures").cloned().unwrap_or_else(|| json!([]));
                checks.push(snapshot_doctor_check(
                    if ok { "pass" } else { "fail" },
                    "manifest_hashes",
                    if ok {
                        "all manifest file hashes match"
                    } else {
                        "one or more manifest file hashes do not match"
                    },
                    json!({
                        "checked": checked,
                        "failures": failures,
                    }),
                ));
                verify = value;
            }
            Err(error) => checks.push(snapshot_doctor_check(
                "fail",
                "manifest_hashes",
                "manifest verification could not complete",
                json!({
                    "error": error.to_string(),
                }),
            )),
        }
    } else {
        checks.push(snapshot_doctor_check(
            "skipped",
            "manifest_hashes",
            "manifest verification was skipped",
            json!({}),
        ));
    }

    let stats = match snapshot_stats(
        dir,
        SnapshotStatsOptions {
            top: options.top,
            verify: false,
        },
    ) {
        Ok(value) => {
            checks.push(snapshot_doctor_check(
                "pass",
                "stats:read",
                "snapshot records were parsed",
                json!({}),
            ));
            snapshot_doctor_add_stats_checks(&mut checks, &value);
            snapshot_doctor_add_index_checks(dir, &mut checks, &value, options.require_index);
            value
        }
        Err(error) => {
            checks.push(snapshot_doctor_check(
                "fail",
                "stats:read",
                "snapshot records could not be parsed",
                json!({
                    "error": error.to_string(),
                }),
            ));
            Value::Null
        }
    };

    let summary = snapshot_doctor_summary(&checks);
    let failures = summary
        .get("failures")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    Ok(json!({
        "dir": dir.display().to_string(),
        "ok": failures == 0,
        "verified": verified,
        "require_index": options.require_index,
        "top": options.top,
        "summary": summary,
        "checks": checks,
        "verify": verify,
        "stats": stats,
    }))
}

pub(crate) fn snapshot_doctor_check(
    status: &str,
    name: impl Into<String>,
    message: impl Into<String>,
    details: Value,
) -> Value {
    json!({
        "name": name.into(),
        "status": status,
        "message": message.into(),
        "details": details,
    })
}

pub(crate) fn snapshot_doctor_summary(checks: &[Value]) -> Value {
    let mut pass = 0_u64;
    let mut warn = 0_u64;
    let mut fail = 0_u64;
    let mut skipped = 0_u64;
    for check in checks {
        match check.get("status").and_then(Value::as_str) {
            Some("pass") => pass += 1,
            Some("warn") => warn += 1,
            Some("fail") => fail += 1,
            Some("skipped") => skipped += 1,
            _ => warn += 1,
        }
    }
    json!({
        "total": checks.len(),
        "pass": pass,
        "warn": warn,
        "fail": fail,
        "skipped": skipped,
        "failures": fail,
        "warnings": warn,
    })
}

pub(crate) fn snapshot_doctor_add_stats_checks(checks: &mut Vec<Value>, stats: &Value) {
    let Some(sections) = stats.get("sections").and_then(Value::as_object) else {
        checks.push(snapshot_doctor_check(
            "fail",
            "stats:sections",
            "snapshot stats did not contain sections",
            json!({}),
        ));
        return;
    };

    for (label, section_stats) in sections {
        snapshot_doctor_add_count_check(checks, stats, label, section_stats);
        snapshot_doctor_add_id_check(checks, label, section_stats);
    }
}

pub(crate) fn snapshot_doctor_add_count_check(
    checks: &mut Vec<Value>,
    stats: &Value,
    label: &str,
    section_stats: &Value,
) {
    let actual = section_stats.get("rows").and_then(Value::as_u64);
    let expected = snapshot_doctor_manifest_count(stats, label);
    let (status, message) = match (expected, actual) {
        (Some(expected), Some(actual)) if expected == actual => {
            ("pass", "section row count matches manifest count")
        }
        (Some(_), Some(_)) => ("fail", "section row count does not match manifest count"),
        (None, Some(_)) => ("warn", "manifest does not contain a count for this section"),
        (_, None) => ("fail", "section stats did not contain a row count"),
    };
    checks.push(snapshot_doctor_check(
        status,
        format!("count:{label}"),
        message,
        json!({
            "section": label,
            "expected": expected,
            "actual": actual,
        }),
    ));
}

pub(crate) fn snapshot_doctor_add_id_check(
    checks: &mut Vec<Value>,
    label: &str,
    section_stats: &Value,
) {
    let Some(ids) = section_stats.get("ids").and_then(Value::as_object) else {
        checks.push(snapshot_doctor_check(
            "fail",
            format!("ids:{label}"),
            "section stats did not contain ID metrics",
            json!({
                "section": label,
            }),
        ));
        return;
    };

    let rows = section_stats
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rows_with_id = ids
        .get("rows_with_id")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let missing_id = ids
        .get("missing_id")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let unique_ids = ids
        .get("unique_ids")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let duplicate_id_count = ids
        .get("duplicate_id_count")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let strict = snapshot_doctor_requires_top_level_ids(label);
    let has_id_problem = missing_id > 0 || duplicate_id_count > 0;
    let status = if has_id_problem && strict {
        "fail"
    } else if has_id_problem {
        "warn"
    } else {
        "pass"
    };
    let message = if !has_id_problem {
        "top-level ID coverage is consistent"
    } else if strict {
        "required top-level IDs are missing or duplicated"
    } else {
        "moment rows do not always expose stable top-level IDs"
    };

    checks.push(snapshot_doctor_check(
        status,
        format!("ids:{label}"),
        message,
        json!({
            "section": label,
            "strict": strict,
            "rows": rows,
            "rows_with_id": rows_with_id,
            "missing_id": missing_id,
            "unique_ids": unique_ids,
            "duplicate_id_count": duplicate_id_count,
            "duplicate_ids": ids.get("duplicate_ids").cloned().unwrap_or_else(|| json!([])),
        }),
    ));
}

pub(crate) fn snapshot_doctor_add_index_checks(
    dir: &Path,
    checks: &mut Vec<Value>,
    stats: &Value,
    require_index: bool,
) {
    let Some(sections) = stats.get("sections").and_then(Value::as_object) else {
        return;
    };

    for (label, section_stats) in sections {
        let Ok(section) = snapshot_query_section(label) else {
            continue;
        };
        if section.kind != SnapshotQuerySectionKind::Jsonl {
            continue;
        }
        snapshot_doctor_add_index_check(dir, checks, section, section_stats, require_index);
    }
}

pub(crate) fn snapshot_doctor_add_index_check(
    dir: &Path,
    checks: &mut Vec<Value>,
    section: SnapshotQuerySection,
    section_stats: &Value,
    require_index: bool,
) {
    let rows = section_stats
        .get("rows")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let rows_with_id = snapshot_doctor_id_metric(section_stats, "rows_with_id").unwrap_or_default();
    let missing_id = snapshot_doctor_id_metric(section_stats, "missing_id").unwrap_or_default();
    let index_path = snapshot_index_path(dir, section);
    let index_path_text = index_path.display().to_string();

    if rows == 0 {
        checks.push(snapshot_doctor_check(
            "pass",
            format!("index:{}", section.label),
            "empty JSONL section does not need an index",
            json!({
                "section": section.label,
                "file": section.file_name,
                "index_path": index_path_text,
                "rows": rows,
            }),
        ));
        return;
    }

    let source = match snapshot_index_source_file(dir, section) {
        Ok(source) => source,
        Err(error) => {
            checks.push(snapshot_doctor_check(
                if require_index { "fail" } else { "warn" },
                format!("index:{}", section.label),
                "index source file could not be read from manifest",
                json!({
                    "section": section.label,
                    "file": section.file_name,
                    "index_path": index_path_text,
                    "error": error.to_string(),
                }),
            ));
            return;
        }
    };

    match read_snapshot_index_if_present(&index_path) {
        Ok(Some(index)) if snapshot_index_matches_source(&index, section, &source) => {
            let counts_match = index.record_count == rows
                && index.indexed_count == rows_with_id
                && index.skipped_without_id == missing_id;
            checks.push(snapshot_doctor_check(
                if counts_match { "pass" } else { "fail" },
                format!("index:{}", section.label),
                if counts_match {
                    "index is fresh for this snapshot file"
                } else {
                    "index is fresh by fingerprint but its counts do not match parsed stats"
                },
                json!({
                    "section": section.label,
                    "file": section.file_name,
                    "index_path": index_path_text,
                    "record_count": index.record_count,
                    "indexed_count": index.indexed_count,
                    "skipped_without_id": index.skipped_without_id,
                    "expected_record_count": rows,
                    "expected_indexed_count": rows_with_id,
                    "expected_skipped_without_id": missing_id,
                }),
            ));
        }
        Ok(Some(index)) => checks.push(snapshot_doctor_check(
            if require_index { "fail" } else { "warn" },
            format!("index:{}", section.label),
            "index exists but is stale or belongs to a different section",
            json!({
                "section": section.label,
                "file": section.file_name,
                "index_path": index_path_text,
                "expected_file": source,
                "indexed_section": index.section,
                "indexed_file": index.file,
            }),
        )),
        Ok(None) => checks.push(snapshot_doctor_check(
            if require_index { "fail" } else { "warn" },
            format!("index:{}", section.label),
            "JSONL section does not have an index",
            json!({
                "section": section.label,
                "file": section.file_name,
                "index_path": index_path_text,
                "rows": rows,
            }),
        )),
        Err(error) => checks.push(snapshot_doctor_check(
            if require_index { "fail" } else { "warn" },
            format!("index:{}", section.label),
            "index could not be read",
            json!({
                "section": section.label,
                "file": section.file_name,
                "index_path": index_path_text,
                "error": error.to_string(),
            }),
        )),
    }
}

pub(crate) fn snapshot_doctor_manifest_count(stats: &Value, label: &str) -> Option<u64> {
    let counts = stats.get("manifest")?.get("counts")?;
    counts
        .get(label)
        .and_then(Value::as_u64)
        .or_else(|| counts.get("moments")?.get(label)?.as_u64())
}

pub(crate) fn snapshot_doctor_id_metric(section_stats: &Value, key: &str) -> Option<u64> {
    section_stats.get("ids")?.get(key).and_then(Value::as_u64)
}

pub(crate) fn snapshot_doctor_requires_top_level_ids(label: &str) -> bool {
    matches!(label, "contacts" | "full_contacts" | "groups")
}

pub(crate) fn snapshot_stats(dir: &Path, options: SnapshotStatsOptions) -> Result<Value> {
    if options.verify {
        let verify = verify_snapshot(dir)?;
        if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("snapshot failed manifest verification");
        }
    }
    let manifest = read_snapshot_manifest(dir)?;
    let Some(files) = manifest.get("files").and_then(Value::as_object) else {
        return err("snapshot manifest does not contain files object");
    };

    let mut sections = Map::new();
    let mut non_record_files = Vec::new();
    for label in snapshot_stats_section_order(files) {
        let Some(entry) = files.get(&label) else {
            continue;
        };
        match snapshot_query_section(&label) {
            Ok(section) => {
                let stats = match section.kind {
                    SnapshotQuerySectionKind::Jsonl => {
                        snapshot_jsonl_section_stats(dir, section, entry, options.top)?
                    }
                    SnapshotQuerySectionKind::JsonArray => {
                        snapshot_array_section_stats(dir, section, entry, options.top)?
                    }
                };
                sections.insert(label, stats);
            }
            Err(_) => {
                non_record_files.push(json!({
                    "label": label,
                    "file": snapshot_file_fingerprint(entry)?,
                }));
            }
        }
    }

    Ok(json!({
        "ok": true,
        "dir": dir.display().to_string(),
        "verified": options.verify,
        "top": options.top,
        "manifest": {
            "schema": manifest.get("schema").cloned().unwrap_or(Value::Null),
            "meshx_version": manifest.get("meshx_version").cloned().unwrap_or(Value::Null),
            "created_at_unix_ms": manifest.get("created_at_unix_ms").cloned().unwrap_or(Value::Null),
            "counts": manifest.get("counts").cloned().unwrap_or(Value::Null),
            "file_count": files.len(),
        },
        "sections": sections,
        "non_record_files": non_record_files,
    }))
}

pub(crate) fn snapshot_stats_section_order(files: &Map<String, Value>) -> Vec<String> {
    let mut labels = Vec::new();
    for label in [
        "contacts",
        "full_contacts",
        "groups",
        "notes",
        "events",
        "emails",
        "events_upcoming",
        "emails_recent",
        "reminders_recent",
        "reminders_upcoming",
    ] {
        if files.contains_key(label) {
            labels.push(label.to_string());
        }
    }
    for label in files.keys() {
        if !labels.iter().any(|value| value == label) {
            labels.push(label.clone());
        }
    }
    labels
}

pub(crate) fn snapshot_jsonl_section_stats(
    dir: &Path,
    section: SnapshotQuerySection,
    entry: &Value,
    top: usize,
) -> Result<Value> {
    let path = snapshot_manifest_file_path(dir, section.file_label)?;
    let file = fs::File::open(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", path.display()))?;
    let reader = StdBufReader::new(file);
    let mut stats = SnapshotStatsAccumulator::default();
    for (index, line) in reader.lines().enumerate() {
        let line = line
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {} line {}", path.display(), index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let row = serde_json::from_str::<Value>(&line)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {} line {}", path.display(), index + 1))?;
        stats.add_row(&row);
    }
    stats.finish(section, "jsonl", entry, top)
}

pub(crate) fn snapshot_array_section_stats(
    dir: &Path,
    section: SnapshotQuerySection,
    entry: &Value,
    top: usize,
) -> Result<Value> {
    let mut stats = SnapshotStatsAccumulator::default();
    let path = snapshot_manifest_file_path(dir, section.file_label)?;
    for row in read_snapshot_array_values_at_path(&path)? {
        stats.add_row(&row);
    }
    stats.finish(section, "json-array", entry, top)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prune_action(
        kind: SnapshotCatalogCandidateKind,
        path: &str,
        rank_time_unix_ms: u64,
        bytes: u64,
    ) -> SnapshotPruneAction {
        SnapshotPruneAction {
            kind,
            path: PathBuf::from(path),
            delete: false,
            reasons: Vec::new(),
            rank_time_unix_ms: Some(rank_time_unix_ms),
            rank_time_source: "test",
            bytes: Some(bytes),
            health: None,
            error: None,
        }
    }

    #[test]
    fn snapshot_prune_apply_keep_latest_prunes_older_items_per_kind() {
        let mut actions = vec![
            prune_action(SnapshotCatalogCandidateKind::Snapshot, "snap-old", 10, 100),
            prune_action(
                SnapshotCatalogCandidateKind::Archive,
                "archive-new.tar.zst",
                40,
                400,
            ),
            prune_action(SnapshotCatalogCandidateKind::Snapshot, "snap-new", 20, 200),
            prune_action(
                SnapshotCatalogCandidateKind::Archive,
                "archive-old.tar.zst",
                30,
                300,
            ),
        ];

        snapshot_prune_apply_keep_latest(&mut actions, Some(1));

        assert!(actions[0].delete);
        assert_eq!(actions[0].reasons, vec!["keep-latest"]);
        assert!(!actions[1].delete);
        assert!(!actions[2].delete);
        assert!(actions[3].delete);
        assert_eq!(actions[3].reasons, vec!["keep-latest"]);
    }

    #[test]
    fn snapshot_prune_action_value_reports_dry_run_delete() {
        let mut action = prune_action(SnapshotCatalogCandidateKind::Snapshot, "snap-old", 10, 100);
        action.delete = true;
        action.reasons.push("keep-latest".to_string());

        let value = snapshot_prune_action_value(action, &BTreeSet::new(), true);

        assert_eq!(value.get("action"), Some(&json!("delete")));
        assert_eq!(value.get("status"), Some(&json!("would-delete")));
        assert_eq!(value.get("reasons"), Some(&json!(["keep-latest"])));
    }

    #[test]
    fn snapshot_prune_summary_counts_planned_deletes_and_bytes() {
        let actions = vec![
            json!({
                "kind": "snapshot",
                "action": "delete",
                "status": "would-delete",
                "bytes": 100,
            }),
            json!({
                "kind": "archive",
                "action": "keep",
                "status": "keep",
                "bytes": 300,
            }),
            json!({
                "kind": "archive",
                "action": "delete",
                "status": "deleted",
                "bytes": 400,
            }),
        ];

        let summary = snapshot_prune_summary(&actions, &[json!({"path": "broken"})], &[]);

        assert_eq!(summary.get("total"), Some(&json!(3)));
        assert_eq!(summary.get("snapshots"), Some(&json!(1)));
        assert_eq!(summary.get("archives"), Some(&json!(2)));
        assert_eq!(summary.get("keep"), Some(&json!(1)));
        assert_eq!(summary.get("planned_delete"), Some(&json!(2)));
        assert_eq!(summary.get("deleted"), Some(&json!(1)));
        assert_eq!(summary.get("action_errors"), Some(&json!(0)));
        assert_eq!(summary.get("delete_errors"), Some(&json!(1)));
        assert_eq!(summary.get("planned_bytes"), Some(&json!(500)));
    }

    #[test]
    fn snapshot_report_component_requires_explicit_ok() {
        let (_, ok) = snapshot_report_component(Ok(json!({"items": []})));

        assert!(!ok);
    }

    #[test]
    fn snapshot_stats_reports_explicit_ok() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-stats-ok-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(dir.join("manifest.json"), r#"{"files":{}}"#).into_diagnostic()?;

        let value = snapshot_stats(
            &dir,
            SnapshotStatsOptions {
                top: 5,
                verify: false,
            },
        )?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(value.get("ok"), Some(&json!(true)));
        Ok(())
    }

    #[test]
    fn snapshot_stats_reads_sections_from_manifest_paths() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-stats-manifest-path-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(dir.join("data")).into_diagnostic()?;
        let contacts = "{\"id\":1,\"name\":\"Ada\"}\n";
        let groups = r#"[{"id":10,"name":"Operators"}]"#;
        fs::write(dir.join("data/contacts.jsonl"), contacts).into_diagnostic()?;
        fs::write(dir.join("data/groups.json"), groups).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "contacts": {
                        "path": "data/contacts.jsonl",
                        "bytes": contacts.len() as u64,
                        "sha256": sha256_hex(contacts.as_bytes()),
                    },
                    "groups": {
                        "path": "data/groups.json",
                        "bytes": groups.len() as u64,
                        "sha256": sha256_hex(groups.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()?;

        let value = snapshot_stats(
            &dir,
            SnapshotStatsOptions {
                top: 5,
                verify: false,
            },
        )?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(
            value.pointer("/sections/contacts/file/path"),
            Some(&json!("data/contacts.jsonl"))
        );
        assert_eq!(value.pointer("/sections/contacts/rows"), Some(&json!(1)));
        assert_eq!(
            value.pointer("/sections/groups/file/path"),
            Some(&json!("data/groups.json"))
        );
        assert_eq!(value.pointer("/sections/groups/rows"), Some(&json!(1)));
        Ok(())
    }

    #[test]
    fn snapshot_history_snapshot_rejects_malformed_manifest() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-history-malformed-manifest-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(dir.join("manifest.json"), "{").into_diagnostic()?;

        let error = snapshot_history_snapshot(dir.clone())
            .expect_err("malformed manifests should not rank as valid snapshots");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("parsing"));
        Ok(())
    }

    #[test]
    fn snapshot_history_snapshot_falls_back_to_mtime_without_created_at() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-history-mtime-fallback-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(dir.join("manifest.json"), r#"{"files":{}}"#).into_diagnostic()?;

        let snapshot = snapshot_history_snapshot(dir.clone())?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(snapshot.created_at_unix_ms, None);
        assert!(snapshot.rank_time_unix_ms.is_some());
        assert_eq!(snapshot.rank_time_source, "filesystem.modified");
        Ok(())
    }

    #[test]
    fn snapshot_history_records_rejects_duplicate_jsonl_ids() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-history-duplicate-jsonl-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(dir.join("data")).into_diagnostic()?;
        let content = "{\"id\":1,\"name\":\"first\"}\n{\"id\":1,\"name\":\"second\"}\n";
        fs::write(dir.join("data/contacts.jsonl"), content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "contacts": {
                        "path": "data/contacts.jsonl",
                        "bytes": content.len() as u64,
                        "sha256": sha256_hex(content.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()?;

        let options = SnapshotHistoryOptions {
            root: dir.clone(),
            recursive: false,
            max_depth: None,
            limit: None,
            section: snapshot_query_section("contacts")?,
            ids: vec![1],
            verify: false,
            index: SnapshotIndexMode::Off,
            details: false,
            detail_limit: 0,
            records: false,
        };
        let error = snapshot_history_records(&dir, &options)
            .expect_err("duplicate IDs should not be collapsed");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        assert!(error.to_string().contains("data/contacts.jsonl"));
        Ok(())
    }

    #[test]
    fn snapshot_history_records_rejects_duplicate_array_ids() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "meshx-history-duplicate-array-{}-{}",
            std::process::id(),
            now_millis()
        ));
        fs::create_dir_all(dir.join("data")).into_diagnostic()?;
        let content = r#"[{"id":1,"name":"first"},{"id":1,"name":"second"}]"#;
        fs::write(dir.join("data/groups.json"), content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "groups": {
                        "path": "data/groups.json",
                        "bytes": content.len() as u64,
                        "sha256": sha256_hex(content.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()?;

        let options = SnapshotHistoryOptions {
            root: dir.clone(),
            recursive: false,
            max_depth: None,
            limit: None,
            section: snapshot_query_section("groups")?,
            ids: vec![1],
            verify: false,
            index: SnapshotIndexMode::Off,
            details: false,
            detail_limit: 0,
            records: false,
        };
        let error = snapshot_history_records(&dir, &options)
            .expect_err("duplicate IDs should not be collapsed");

        fs::remove_dir_all(&dir).ok();
        assert!(error.to_string().contains("duplicate record ID 1"));
        assert!(error.to_string().contains("data/groups.json"));
        Ok(())
    }

    #[test]
    fn snapshot_prune_reports_snapshot_byte_errors() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "meshx-prune-byte-error-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let snapshot = root.join("broken-snapshot");
        fs::create_dir_all(&snapshot).into_diagnostic()?;
        fs::write(snapshot.join("manifest.json"), "{").into_diagnostic()?;

        let result = snapshot_prune(SnapshotPruneOptions {
            root: root.clone(),
            recursive: true,
            max_depth: None,
            limit: None,
            include_snapshots: true,
            include_archives: true,
            keep_latest: Some(0),
            older_than_days: None,
            failed: false,
            require_index: false,
            dry_run: true,
            yes: false,
        });

        fs::remove_dir_all(&root).ok();

        let value = result?;
        assert_eq!(value.get("ok"), Some(&json!(false)));
        assert_eq!(value.pointer("/summary/action_errors"), Some(&json!(1)));
        let action = value
            .get("actions")
            .and_then(Value::as_array)
            .and_then(|actions| actions.first())
            .expect("expected prune action");
        assert_eq!(action.get("status"), Some(&json!("would-delete")));
        assert_eq!(action.get("bytes"), Some(&Value::Null));
        assert!(
            action
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|error| error.contains("parsing"))
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_prune_reports_discovery_errors() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "meshx-prune-discovery-error-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let blocked = root.join("blocked");
        fs::create_dir_all(&blocked).into_diagnostic()?;
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).into_diagnostic()?;

        let result = snapshot_prune(SnapshotPruneOptions {
            root: root.clone(),
            recursive: true,
            max_depth: None,
            limit: None,
            include_snapshots: true,
            include_archives: true,
            keep_latest: Some(0),
            older_than_days: None,
            failed: false,
            require_index: false,
            dry_run: true,
            yes: false,
        });

        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).ok();
        fs::remove_dir_all(&root).ok();

        let value = result?;
        assert_eq!(value.get("ok"), Some(&json!(false)));
        assert_eq!(value.pointer("/summary/discovery_errors"), Some(&json!(1)));
        assert_eq!(
            value
                .get("discovery_errors")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_prune_refuses_live_delete_after_discovery_errors() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "meshx-prune-live-discovery-error-{}-{}",
            std::process::id(),
            now_millis()
        ));
        let blocked = root.join("blocked");
        fs::create_dir_all(&blocked).into_diagnostic()?;
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).into_diagnostic()?;

        let result = snapshot_prune(SnapshotPruneOptions {
            root: root.clone(),
            recursive: true,
            max_depth: None,
            limit: None,
            include_snapshots: true,
            include_archives: true,
            keep_latest: Some(0),
            older_than_days: None,
            failed: false,
            require_index: false,
            dry_run: false,
            yes: true,
        });

        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).ok();
        fs::remove_dir_all(&root).ok();

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("snapshot:prune discovery failed")
        );
        Ok(())
    }
}
