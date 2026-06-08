use crate::prelude::*;

pub(crate) fn snapshot_index_source_file(
    dir: &Path,
    section: SnapshotQuerySection,
) -> Result<SnapshotIndexFile> {
    let entry = snapshot_manifest_file_entry(dir, section.file_label)?
        .ok_or_else(|| miette!("snapshot does not contain section {}", section.label))?;
    snapshot_index_file_from_manifest(&entry)
}

pub(crate) fn snapshot_index_file_from_manifest(entry: &Value) -> Result<SnapshotIndexFile> {
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry is missing path"))?;
    let bytes = entry
        .get("bytes")
        .and_then(Value::as_u64)
        .ok_or_else(|| miette!("snapshot file entry {path} is missing bytes"))?;
    let sha256 = entry
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry {path} is missing sha256"))?;
    Ok(SnapshotIndexFile {
        path: path.to_string(),
        bytes,
        sha256: sha256.to_string(),
    })
}

pub(crate) fn build_snapshot_index(
    dir: &Path,
    section: SnapshotQuerySection,
    source: SnapshotIndexFile,
) -> Result<SnapshotIndex> {
    let path = safe_snapshot_file_path(dir, &source.path)?;
    let file = fs::File::open(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", path.display()))?;
    let mut reader = StdBufReader::new(file);
    let mut hasher = Sha256::new();
    let mut entries = BTreeMap::new();
    let mut offset = 0_u64;
    let mut record_count = 0_u64;
    let mut indexed_count = 0_u64;
    let mut skipped_without_id = 0_u64;
    let mut line_number = 0_u64;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        hasher.update(line.as_bytes());
        let line_offset = offset;
        offset = offset.saturating_add(bytes_read as u64);
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line)
            .into_diagnostic()
            .wrap_err_with(|| format!("parsing {} line {}", path.display(), line_number))?;
        record_count += 1;
        if let Some(id) = record_id(&value) {
            entries.insert(
                id,
                SnapshotIndexEntry {
                    offset: line_offset,
                    bytes: bytes_read as u64,
                    line: line_number,
                },
            );
            indexed_count += 1;
        } else {
            skipped_without_id += 1;
        }
    }

    let actual_sha256 = hex::encode(hasher.finalize());
    if offset != source.bytes || actual_sha256 != source.sha256 {
        return err(format!(
            "{} does not match manifest while indexing: expected {} bytes/{}, got {} bytes/{}",
            path.display(),
            source.bytes,
            source.sha256,
            offset,
            actual_sha256
        ));
    }

    Ok(SnapshotIndex {
        schema: "meshx.snapshot-index.v1".to_string(),
        meshx_version: VERSION.to_string(),
        created_at_unix_ms: now_millis(),
        section: section.label.to_string(),
        file: source,
        record_count,
        indexed_count,
        skipped_without_id,
        entries,
    })
}

pub(crate) fn read_snapshot_index_if_present(path: &Path) -> Result<Option<SnapshotIndex>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", path.display()));
        }
    };
    let index = serde_json::from_str::<SnapshotIndex>(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    if index.schema != "meshx.snapshot-index.v1" {
        return err(format!("{} is not a meshx snapshot index", path.display()));
    }
    Ok(Some(index))
}

pub(crate) fn snapshot_index_path(dir: &Path, section: SnapshotQuerySection) -> PathBuf {
    dir.join(".meshx-index")
        .join(format!("{}.json", section.file_label))
}

pub(crate) fn snapshot_index_matches_source(
    index: &SnapshotIndex,
    section: SnapshotQuerySection,
    source: &SnapshotIndexFile,
) -> bool {
    index.section == section.label && &index.file == source
}

pub(crate) fn snapshot_index_summary(index: &SnapshotIndex, path: &Path, reused: bool) -> Value {
    json!({
        "index_path": path.display().to_string(),
        "reused": reused,
        "schema": index.schema,
        "section": index.section,
        "file": index.file,
        "record_count": index.record_count,
        "indexed_count": index.indexed_count,
        "skipped_without_id": index.skipped_without_id,
    })
}

pub(crate) fn query_snapshot(dir: &Path, options: SnapshotQueryOptions) -> Result<Value> {
    prepare_snapshot_query(dir, &options)?;
    let mut rows = read_snapshot_section_values(dir, options.section)?;
    rows = filter_snapshot_query_rows(rows, &options)?;
    Ok(Value::Array(rows))
}

pub(crate) fn prepare_snapshot_query(dir: &Path, options: &SnapshotQueryOptions) -> Result<()> {
    if options.verify {
        let verify = verify_snapshot(dir)?;
        if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("snapshot failed manifest verification");
        }
    }
    if !snapshot_manifest_has_file(dir, options.section.file_label)? {
        return err(format!(
            "snapshot does not contain section {}",
            options.section.label
        ));
    }
    Ok(())
}

pub(crate) fn snapshot_query_jsonl_each<F>(
    dir: &Path,
    options: &SnapshotQueryOptions,
    mut each: F,
) -> Result<usize>
where
    F: FnMut(Value) -> Result<()>,
{
    if !options.ids.is_empty() && options.index != SnapshotIndexMode::Off {
        match snapshot_query_indexed_each(dir, options, &mut each) {
            Ok(Some(count)) => return Ok(count),
            Ok(None) if options.index == SnapshotIndexMode::Require => {
                return err(format!(
                    "snapshot index is required for section {}; run snapshot:index --dir {} --section {}",
                    options.section.label,
                    dir.display(),
                    options.section.label
                ));
            }
            Ok(None) => {}
            Err(error) if options.index == SnapshotIndexMode::Auto => {
                warn!(?error, "snapshot index unavailable; scanning JSONL section");
            }
            Err(error) => return Err(error),
        }
    }

    let path = snapshot_manifest_file_path(dir, options.section.file_label)?;
    let file = fs::File::open(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", path.display()))?;
    let reader = StdBufReader::new(file);
    let id_filter = snapshot_query_id_filter(options);
    let contains = options.contains.as_ref().map(|value| value.to_lowercase());
    let mut selected = 0_usize;

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
        if !snapshot_query_row_matches(&row, &id_filter, contains.as_deref())? {
            continue;
        }
        each(row)?;
        selected += 1;
        if options.limit.is_some_and(|limit| selected >= limit) {
            break;
        }
    }

    Ok(selected)
}

pub(crate) fn snapshot_query_indexed_each<F>(
    dir: &Path,
    options: &SnapshotQueryOptions,
    each: &mut F,
) -> Result<Option<usize>>
where
    F: FnMut(Value) -> Result<()>,
{
    let source = snapshot_index_source_file(dir, options.section)?;
    let index_path = snapshot_index_path(dir, options.section);
    let Some(index) = read_snapshot_index_if_present(&index_path)? else {
        return Ok(None);
    };
    if !snapshot_index_matches_source(&index, options.section, &source) {
        return Ok(None);
    }
    if snapshot_index_has_duplicate_ids(&index) {
        return err(format!(
            "{} cannot satisfy exact ID queries because section {} contains duplicate IDs",
            index_path.display(),
            options.section.label
        ));
    }

    let requested = snapshot_query_id_filter(options);
    let contains = options.contains.as_ref().map(|value| value.to_lowercase());
    let mut selected_entries = Vec::new();
    for id in &requested {
        if let Some(entry) = index.entries.get(id) {
            selected_entries.push((entry.line, entry.offset, id.clone(), entry.clone()));
        }
    }
    selected_entries.sort_by_key(|(line, offset, _, _)| (*line, *offset));

    let path = safe_snapshot_file_path(dir, &index.file.path)?;
    let mut file = fs::File::open(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", path.display()))?;
    let mut selected = 0_usize;
    for (_, _, id, entry) in selected_entries {
        if entry.offset.saturating_add(entry.bytes) > index.file.bytes {
            return err(format!(
                "{} index entry for ID {id} points outside {}",
                index_path.display(),
                index.file.path
            ));
        }
        let size = usize::try_from(entry.bytes)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "{} index entry for ID {id} is too large",
                    index_path.display()
                )
            })?;
        file.seek(SeekFrom::Start(entry.offset))
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "seeking {} to indexed offset {}",
                    path.display(),
                    entry.offset
                )
            })?;
        let mut bytes = vec![0_u8; size];
        file.read_exact(&mut bytes)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "reading {} indexed bytes for ID {id} from {}",
                    entry.bytes,
                    path.display()
                )
            })?;
        let row = serde_json::from_slice::<Value>(&bytes)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "parsing indexed row for ID {id} from {} line {}",
                    path.display(),
                    entry.line
                )
            })?;
        if record_id(&row).as_deref() != Some(id.as_str()) {
            return err(format!(
                "{} index entry for ID {id} points to record {:?}",
                index_path.display(),
                record_id(&row)
            ));
        }
        if !snapshot_query_row_matches(&row, &BTreeSet::new(), contains.as_deref())? {
            continue;
        }
        each(row)?;
        selected += 1;
        if options.limit.is_some_and(|limit| selected >= limit) {
            break;
        }
    }
    Ok(Some(selected))
}

pub(crate) fn filter_snapshot_query_rows(
    rows: Vec<Value>,
    options: &SnapshotQueryOptions,
) -> Result<Vec<Value>> {
    let id_filter = snapshot_query_id_filter(options);
    let contains = options.contains.as_ref().map(|value| value.to_lowercase());
    let mut selected = Vec::new();
    for row in rows {
        if !snapshot_query_row_matches(&row, &id_filter, contains.as_deref())? {
            continue;
        }
        selected.push(row);
        if options.limit.is_some_and(|limit| selected.len() >= limit) {
            break;
        }
    }
    Ok(selected)
}

pub(crate) fn snapshot_query_id_filter(options: &SnapshotQueryOptions) -> BTreeSet<String> {
    options
        .ids
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
}

pub(crate) fn snapshot_index_has_duplicate_ids(index: &SnapshotIndex) -> bool {
    index.indexed_count > index.entries.len() as u64
}

pub(crate) fn snapshot_query_row_matches(
    row: &Value,
    id_filter: &BTreeSet<String>,
    contains: Option<&str>,
) -> Result<bool> {
    if !id_filter.is_empty() {
        let Some(id) = record_id(row) else {
            return Ok(false);
        };
        if !id_filter.contains(&id) {
            return Ok(false);
        }
    }
    if let Some(contains) = contains {
        let text = serde_json::to_string(row)
            .into_diagnostic()
            .wrap_err("serializing snapshot record for text filter")?
            .to_lowercase();
        if !text.contains(contains) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(crate) fn snapshot_query_section(raw: &str) -> Result<SnapshotQuerySection> {
    let normalized = raw.replace('-', "_");
    let section = match normalized.as_str() {
        "contacts" => SnapshotQuerySection {
            label: "contacts",
            file_label: "contacts",
            file_name: "contacts.jsonl",
            kind: SnapshotQuerySectionKind::Jsonl,
        },
        "full_contacts" => SnapshotQuerySection {
            label: "full_contacts",
            file_label: "full_contacts",
            file_name: "full-contacts.jsonl",
            kind: SnapshotQuerySectionKind::Jsonl,
        },
        "groups" => SnapshotQuerySection {
            label: "groups",
            file_label: "groups",
            file_name: "groups.json",
            kind: SnapshotQuerySectionKind::JsonArray,
        },
        "notes" => snapshot_moment_query_section("notes", "notes.jsonl"),
        "events" => snapshot_moment_query_section("events", "events.jsonl"),
        "emails" => snapshot_moment_query_section("emails", "emails.jsonl"),
        "events_upcoming" => {
            snapshot_moment_query_section("events_upcoming", "events-upcoming.jsonl")
        }
        "emails_recent" => snapshot_moment_query_section("emails_recent", "emails-recent.jsonl"),
        "reminders_recent" => {
            snapshot_moment_query_section("reminders_recent", "reminders-recent.jsonl")
        }
        "reminders_upcoming" => {
            snapshot_moment_query_section("reminders_upcoming", "reminders-upcoming.jsonl")
        }
        _ => return err(format!("unknown snapshot section {raw}")),
    };
    Ok(section)
}

pub(crate) fn snapshot_all_query_sections() -> Vec<SnapshotQuerySection> {
    [
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
    ]
    .into_iter()
    .map(|section| snapshot_query_section(section).expect("built-in snapshot section"))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-{label}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn write_manifest_file(dir: &Path, label: &str, path: &str, content: &str) -> Result<()> {
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    label: {
                        "path": path,
                        "bytes": content.len() as u64,
                        "sha256": sha256_hex(content.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()
    }

    #[test]
    fn query_snapshot_reads_jsonl_section_from_manifest_path() -> Result<()> {
        let dir = temp_snapshot_dir("query-manifest-path");
        let section_dir = dir.join("data");
        fs::create_dir_all(&section_dir).into_diagnostic()?;
        let content = "{\"id\":7,\"name\":\"nested\"}\n";
        fs::write(section_dir.join("contacts.jsonl"), content).into_diagnostic()?;
        write_manifest_file(&dir, "contacts", "data/contacts.jsonl", content)?;

        let rows = query_snapshot(
            &dir,
            SnapshotQueryOptions {
                section: snapshot_query_section("contacts")?,
                ids: vec![7],
                contains: None,
                limit: None,
                verify: false,
                index: SnapshotIndexMode::Off,
            },
        )?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(rows, json!([{"id":7,"name":"nested"}]));
        Ok(())
    }

    #[test]
    fn snapshot_query_jsonl_each_scans_section_from_manifest_path() -> Result<()> {
        let dir = temp_snapshot_dir("query-each-manifest-path");
        let section_dir = dir.join("data");
        fs::create_dir_all(&section_dir).into_diagnostic()?;
        let content = "{\"id\":7,\"name\":\"nested\"}\n{\"id\":8,\"name\":\"other\"}\n";
        fs::write(section_dir.join("contacts.jsonl"), content).into_diagnostic()?;
        write_manifest_file(&dir, "contacts", "data/contacts.jsonl", content)?;

        let options = SnapshotQueryOptions {
            section: snapshot_query_section("contacts")?,
            ids: vec![7],
            contains: None,
            limit: None,
            verify: false,
            index: SnapshotIndexMode::Off,
        };
        let mut rows = Vec::new();
        let count = snapshot_query_jsonl_each(&dir, &options, |row| {
            rows.push(row);
            Ok(())
        })?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(count, 1);
        assert_eq!(rows, vec![json!({"id":7,"name":"nested"})]);
        Ok(())
    }

    #[test]
    fn snapshot_query_jsonl_each_scans_when_index_cannot_represent_duplicate_ids() -> Result<()> {
        let dir = temp_snapshot_dir("duplicate-index");
        fs::create_dir_all(dir.join(".meshx-index")).into_diagnostic()?;
        let first = "{\"id\":1,\"name\":\"first\"}\n";
        let second = "{\"id\":1,\"name\":\"second\"}\n";
        let content = format!("{first}{second}");
        fs::write(dir.join("contacts.jsonl"), &content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "contacts": {
                        "path": "contacts.jsonl",
                        "bytes": content.len() as u64,
                        "sha256": sha256_hex(content.as_bytes()),
                    }
                }
            }))
            .into_diagnostic()?,
        )
        .into_diagnostic()?;

        let section = snapshot_query_section("contacts")?;
        let index = SnapshotIndex {
            schema: "meshx.snapshot-index.v1".to_string(),
            meshx_version: "test".to_string(),
            created_at_unix_ms: 0,
            section: section.label.to_string(),
            file: SnapshotIndexFile {
                path: "contacts.jsonl".to_string(),
                bytes: content.len() as u64,
                sha256: sha256_hex(content.as_bytes()),
            },
            record_count: 2,
            indexed_count: 2,
            skipped_without_id: 0,
            entries: BTreeMap::from([(
                "1".to_string(),
                SnapshotIndexEntry {
                    offset: 0,
                    bytes: first.len() as u64,
                    line: 1,
                },
            )]),
        };
        fs::write(
            snapshot_index_path(&dir, section),
            serde_json::to_string(&index).into_diagnostic()?,
        )
        .into_diagnostic()?;

        let options = SnapshotQueryOptions {
            section,
            ids: vec![1],
            contains: None,
            limit: None,
            verify: false,
            index: SnapshotIndexMode::Auto,
        };
        let mut rows = Vec::new();
        let count = snapshot_query_jsonl_each(&dir, &options, |row| {
            rows.push(row);
            Ok(())
        })?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(count, 2);
        assert_eq!(
            rows.iter()
                .filter_map(|row| row.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        Ok(())
    }
}
