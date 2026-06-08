use crate::prelude::*;

pub(crate) fn diff_snapshots(
    old_dir: &Path,
    new_dir: &Path,
    options: SnapshotDiffOptions,
) -> Result<Value> {
    let old_verify = verify_snapshot(old_dir)?;
    let new_verify = verify_snapshot(new_dir)?;
    let old_ok = old_verify
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let new_ok = new_verify
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !old_ok || !new_ok {
        return Ok(json!({
            "ok": false,
            "error": "one or both snapshots failed manifest verification",
            "old_verify": old_verify,
            "new_verify": new_verify,
        }));
    }

    let old_contacts_path = snapshot_manifest_file_path(old_dir, "contacts")?;
    let new_contacts_path = snapshot_manifest_file_path(new_dir, "contacts")?;
    let old_groups_path = snapshot_manifest_file_path(old_dir, "groups")?;
    let new_groups_path = snapshot_manifest_file_path(new_dir, "groups")?;
    let old_contacts = read_snapshot_jsonl_records_at_path(&old_contacts_path)?;
    let new_contacts = read_snapshot_jsonl_records_at_path(&new_contacts_path)?;
    let old_groups = read_snapshot_array_records_at_path(&old_groups_path)?;
    let new_groups = read_snapshot_array_records_at_path(&new_groups_path)?;
    let mut contacts = record_diff(&old_contacts, &new_contacts);
    let mut groups = record_diff(&old_groups, &new_groups);
    if options.details {
        contacts = add_snapshot_jsonl_diff_details_at_paths(
            contacts,
            &old_contacts_path,
            &new_contacts_path,
            options.detail_limit,
        )?;
        groups = add_snapshot_array_diff_details_at_paths(
            groups,
            &old_groups_path,
            &new_groups_path,
            options.detail_limit,
        )?;
    }
    let full_contacts = optional_snapshot_jsonl_diff(old_dir, new_dir, "full_contacts", options)?;
    let moments = snapshot_moment_fingerprint_diffs(old_dir, new_dir)?;

    Ok(json!({
        "ok": true,
        "old": old_dir.display().to_string(),
        "new": new_dir.display().to_string(),
        "details": options.details,
        "detail_limit": options.detail_limit,
        "contacts": contacts,
        "groups": groups,
        "full_contacts": full_contacts,
        "moments": moments,
    }))
}

pub(crate) fn diff_section_count(diff: &Value, section: &str, key: &str) -> u64 {
    diff.get(section)
        .and_then(|section| section.get(key))
        .and_then(Value::as_u64)
        .unwrap_or_default()
}

pub(crate) fn optional_snapshot_jsonl_diff(
    old_dir: &Path,
    new_dir: &Path,
    label: &str,
    options: SnapshotDiffOptions,
) -> Result<Value> {
    let old_has = snapshot_manifest_has_file(old_dir, label)?;
    let new_has = snapshot_manifest_has_file(new_dir, label)?;
    match (old_has, new_has) {
        (true, true) => {
            let old_path = snapshot_manifest_file_path(old_dir, label)?;
            let new_path = snapshot_manifest_file_path(new_dir, label)?;
            let old_records = read_snapshot_jsonl_records_at_path(&old_path)?;
            let new_records = read_snapshot_jsonl_records_at_path(&new_path)?;
            let diff = record_diff(&old_records, &new_records);
            if options.details {
                add_snapshot_jsonl_diff_details_at_paths(
                    diff,
                    &old_path,
                    &new_path,
                    options.detail_limit,
                )
            } else {
                Ok(diff)
            }
        }
        _ => Ok(json!({
            "old_available": old_has,
            "new_available": new_has,
            "compared": false,
        })),
    }
}

pub(crate) fn optional_snapshot_file_fingerprint_diff(
    old_dir: &Path,
    new_dir: &Path,
    label: &str,
) -> Result<Value> {
    let old_entry = snapshot_manifest_file_entry(old_dir, label)?;
    let new_entry = snapshot_manifest_file_entry(new_dir, label)?;
    match (old_entry, new_entry) {
        (Some(old_entry), Some(new_entry)) => {
            let old_fingerprint = snapshot_file_fingerprint(&old_entry)?;
            let new_fingerprint = snapshot_file_fingerprint(&new_entry)?;
            Ok(json!({
                "old_available": true,
                "new_available": true,
                "compared": true,
                "changed": old_fingerprint != new_fingerprint,
                "old": old_fingerprint,
                "new": new_fingerprint,
            }))
        }
        (old_entry, new_entry) => Ok(json!({
            "old_available": old_entry.is_some(),
            "new_available": new_entry.is_some(),
            "compared": false,
        })),
    }
}

pub(crate) fn add_snapshot_jsonl_diff_details_at_paths(
    diff: Value,
    old_path: &Path,
    new_path: &Path,
    detail_limit: usize,
) -> Result<Value> {
    let old_records = read_snapshot_jsonl_record_values_at_path(old_path)?;
    let new_records = read_snapshot_jsonl_record_values_at_path(new_path)?;
    add_record_diff_details(diff, &old_records, &new_records, detail_limit)
}

pub(crate) fn add_snapshot_array_diff_details_at_paths(
    diff: Value,
    old_path: &Path,
    new_path: &Path,
    detail_limit: usize,
) -> Result<Value> {
    let old_records = read_snapshot_array_record_values_at_path(old_path)?;
    let new_records = read_snapshot_array_record_values_at_path(new_path)?;
    add_record_diff_details(diff, &old_records, &new_records, detail_limit)
}

pub(crate) fn add_record_diff_details(
    mut diff: Value,
    old_records: &BTreeMap<String, Value>,
    new_records: &BTreeMap<String, Value>,
    detail_limit: usize,
) -> Result<Value> {
    let Some(diff_object) = diff.as_object_mut() else {
        return err("record diff result was not an object");
    };
    let changed_ids = diff_object
        .get("changed")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut details = Vec::new();
    for id in changed_ids.iter().take(detail_limit) {
        let Some(id) = id.as_str() else {
            continue;
        };
        let (Some(old_value), Some(new_value)) = (old_records.get(id), new_records.get(id)) else {
            continue;
        };
        let mut changes = Vec::new();
        collect_value_changes(
            old_value,
            new_value,
            "",
            &mut changes,
            SNAPSHOT_DIFF_CHANGES_PER_RECORD_MAX + 1,
        );
        let changes_truncated = changes.len() > SNAPSHOT_DIFF_CHANGES_PER_RECORD_MAX;
        changes.truncate(SNAPSHOT_DIFF_CHANGES_PER_RECORD_MAX);
        details.push(json!({
            "id": id,
            "reported_change_count": changes.len(),
            "changes_truncated": changes_truncated,
            "changes": changes,
        }));
    }
    diff_object.insert(
        "detail_limit".to_string(),
        Value::Number(Number::from(detail_limit as u64)),
    );
    diff_object.insert(
        "details_truncated".to_string(),
        Value::Bool(changed_ids.len() > detail_limit),
    );
    diff_object.set("details", Value::Array(details));
    Ok(diff)
}

pub(crate) fn diff_preview_value(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            const MAX_STRING_PREVIEW: usize = 200;
            if text.chars().count() > MAX_STRING_PREVIEW {
                let preview = text.chars().take(MAX_STRING_PREVIEW).collect::<String>();
                json!({
                    "type": "string",
                    "preview": preview,
                    "truncated": true,
                })
            } else {
                Value::String(text.clone())
            }
        }
        Value::Array(items) => json!({
            "type": "array",
            "len": items.len(),
        }),
        Value::Object(object) => json!({
            "type": "object",
            "keys": object.len(),
        }),
        other => other.clone(),
    }
}

pub(crate) fn record_diff(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> Value {
    let old_ids = old.keys().cloned().collect::<BTreeSet<_>>();
    let new_ids = new.keys().cloned().collect::<BTreeSet<_>>();
    let added = new_ids.difference(&old_ids).cloned().collect::<Vec<_>>();
    let removed = old_ids.difference(&new_ids).cloned().collect::<Vec<_>>();
    let changed = old_ids
        .intersection(&new_ids)
        .filter(|id| old.get(*id) != new.get(*id))
        .cloned()
        .collect::<Vec<_>>();
    let unchanged_count = old_ids
        .intersection(&new_ids)
        .filter(|id| old.get(*id) == new.get(*id))
        .count();
    json!({
        "old_count": old.len(),
        "new_count": new.len(),
        "added_count": added.len(),
        "removed_count": removed.len(),
        "changed_count": changed.len(),
        "unchanged_count": unchanged_count,
        "added": added,
        "removed": removed,
        "changed": changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-diff-{label}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_snapshot(
        dir: &Path,
        contacts_path: &str,
        contacts: &str,
        groups_path: &str,
        groups: &str,
    ) -> Result<()> {
        fs::create_dir_all(dir).into_diagnostic()?;
        let contacts_file = safe_snapshot_file_path(dir, contacts_path)?;
        let groups_file = safe_snapshot_file_path(dir, groups_path)?;
        if let Some(parent) = contacts_file.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        if let Some(parent) = groups_file.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(&contacts_file, contacts).into_diagnostic()?;
        fs::write(&groups_file, groups).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "contacts": {
                        "path": contacts_path,
                        "bytes": contacts.len() as u64,
                        "sha256": sha256_hex(contacts.as_bytes()),
                    },
                    "groups": {
                        "path": groups_path,
                        "bytes": groups.len() as u64,
                        "sha256": sha256_hex(groups.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()
    }

    #[test]
    fn diff_snapshots_reads_required_sections_from_manifest_paths() -> Result<()> {
        let old_dir = temp_snapshot_dir("old-manifest-path");
        let new_dir = temp_snapshot_dir("new-manifest-path");
        write_snapshot(
            &old_dir,
            "data/contacts.jsonl",
            "{\"id\":1,\"name\":\"old\"}\n",
            "data/groups.json",
            r#"[{"id":10,"name":"group"}]"#,
        )?;
        write_snapshot(
            &new_dir,
            "data/contacts.jsonl",
            "{\"id\":1,\"name\":\"new\"}\n{\"id\":2,\"name\":\"added\"}\n",
            "data/groups.json",
            r#"[{"id":10,"name":"group"}]"#,
        )?;

        let diff = diff_snapshots(
            &old_dir,
            &new_dir,
            SnapshotDiffOptions {
                details: true,
                detail_limit: 10,
            },
        )?;

        fs::remove_dir_all(&old_dir).ok();
        fs::remove_dir_all(&new_dir).ok();
        assert_eq!(diff.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(diff_section_count(&diff, "contacts", "added_count"), 1);
        assert_eq!(diff_section_count(&diff, "contacts", "changed_count"), 1);
        assert_eq!(diff_section_count(&diff, "groups", "unchanged_count"), 1);
        Ok(())
    }
}
