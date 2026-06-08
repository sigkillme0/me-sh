use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct SnapshotUnpackOptions {
    pub(crate) verify: bool,
    pub(crate) force: bool,
}

impl SnapshotUnpackOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        Ok(Self {
            verify: !matches.get_flag("skip-verify"),
            force: matches.get_flag("force"),
        })
    }
}

pub(crate) fn snapshot_pack_compression_level(matches: &ArgMatches) -> Result<i32> {
    let Some(raw) = matches.get_one::<String>("compression-level") else {
        return Ok(0);
    };
    let level = raw
        .parse::<i32>()
        .into_diagnostic()
        .wrap_err("--compression-level must be an integer from 0 to 22")?;
    if !(0..=22).contains(&level) {
        return err("--compression-level must be between 0 and 22");
    }
    Ok(level)
}

pub(crate) fn snapshot_catalog_is_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".tar.zst"))
}

pub(crate) fn snapshot_catalog_archive_item(
    archive: &Path,
    options: &SnapshotCatalogOptions,
) -> Value {
    let mut item = snapshot_catalog_base_item(SnapshotCatalogCandidateKind::Archive, archive);
    match fs::metadata(archive) {
        Ok(metadata) => {
            item.set("bytes", json!(metadata.len()));
        }
        Err(error) => {
            return snapshot_catalog_error_item(
                SnapshotCatalogCandidateKind::Archive,
                archive,
                "archive metadata could not be read",
                error,
            );
        }
    }

    if options.verify || options.doctor {
        match snapshot_verify_archive(
            archive,
            SnapshotVerifyArchiveOptions {
                require_index: options.require_index,
            },
        ) {
            Ok(report) => {
                if let Some(manifest) = report.get("manifest") {
                    snapshot_catalog_add_archive_manifest_summary(&mut item, manifest);
                }
                snapshot_catalog_add_report(&mut item, "verify_archive", report);
            }
            Err(error) => {
                return snapshot_catalog_error_item(
                    SnapshotCatalogCandidateKind::Archive,
                    archive,
                    "archive verification could not complete",
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

pub(crate) fn snapshot_catalog_add_archive_manifest_summary(
    item: &mut Map<String, Value>,
    manifest: &Value,
) {
    for key in ["schema", "meshx_version", "created_at_unix_ms", "counts"] {
        item.insert(
            key.to_string(),
            manifest.get(key).cloned().unwrap_or(Value::Null),
        );
    }
}

pub(crate) fn snapshot_verify_archive(
    archive_path: &Path,
    options: SnapshotVerifyArchiveOptions,
) -> Result<Value> {
    let archive_file = fs::File::open(archive_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", archive_path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(archive_file)
        .into_diagnostic()
        .wrap_err_with(|| format!("decompressing {}", archive_path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut checks = Vec::new();
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut files = BTreeMap::new();
    let mut package = Value::Null;
    let mut manifest = Value::Null;
    let mut indexes = BTreeMap::new();

    for entry in archive
        .entries()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading tar entries from {}", archive_path.display()))?
    {
        let mut entry = entry
            .into_diagnostic()
            .wrap_err_with(|| format!("reading tar entry from {}", archive_path.display()))?;
        let raw_path = entry
            .path()
            .into_diagnostic()
            .wrap_err("reading archive entry path")?
            .into_owned();
        let entry_type = entry.header().entry_type();
        let relative = match snapshot_archive_relative_path(&raw_path) {
            Ok(relative) => relative,
            Err(_) if entry_type.is_dir() && snapshot_archive_is_root_dir_path(&raw_path) => {
                continue;
            }
            Err(error) => {
                checks.push(snapshot_doctor_check(
                    "fail",
                    "archive:paths",
                    "archive entry path is unsafe",
                    json!({
                        "path": raw_path.display().to_string(),
                        "error": error.to_string(),
                    }),
                ));
                return Ok(snapshot_verify_archive_result(
                    archive_path,
                    options,
                    checks,
                    entries,
                    package,
                    manifest,
                    indexes,
                ));
            }
        };
        let path = snapshot_archive_path_string(&relative);
        if !seen.insert(path.clone()) {
            checks.push(snapshot_doctor_check(
                "fail",
                format!("entry:{path}"),
                "archive contains duplicate entry path",
                json!({
                    "path": path,
                }),
            ));
            snapshot_archive_discard_entry(&mut entry)?;
            continue;
        }

        if entry_type.is_dir() {
            entries.push(json!({
                "path": path,
                "kind": "directory",
            }));
            continue;
        }
        if !entry_type.is_file() {
            checks.push(snapshot_doctor_check(
                "fail",
                format!("entry:{path}"),
                "archive entry type is unsupported",
                json!({
                    "path": path,
                    "entry_type": format!("{entry_type:?}"),
                }),
            ));
            return Ok(snapshot_verify_archive_result(
                archive_path,
                options,
                checks,
                entries,
                package,
                manifest,
                indexes,
            ));
        }

        let file = if path == "manifest.json" || path == ".meshx-package.json" {
            let (value, file) =
                snapshot_archive_read_json_entry(&mut entry, &path, 16 * 1024 * 1024)?;
            if path == "manifest.json" {
                manifest = value;
            } else {
                package = value;
            }
            file
        } else if path.starts_with(".meshx-index/") && path.ends_with(".json") {
            let (value, file) =
                snapshot_archive_read_json_entry(&mut entry, &path, 256 * 1024 * 1024)?;
            match serde_json::from_value::<SnapshotIndex>(value) {
                Ok(index) => {
                    indexes.insert(path.clone(), index);
                }
                Err(error) => checks.push(snapshot_doctor_check(
                    "fail",
                    format!("index:{path}"),
                    "archive index sidecar is not a valid meshx snapshot index",
                    json!({
                        "path": path,
                        "error": error.to_string(),
                    }),
                )),
            }
            file
        } else {
            snapshot_archive_hash_entry(&mut entry)?
        };
        files.insert(path.clone(), file.clone());
        entries.push(json!({
            "path": path,
            "kind": "file",
            "bytes": file.bytes,
            "sha256": file.sha256,
        }));
    }

    checks.push(snapshot_doctor_check(
        "pass",
        "archive:stream",
        "archive entries were streamed successfully",
        json!({
            "entries": entries.len(),
        }),
    ));
    snapshot_archive_add_package_checks(&package, &mut checks);
    snapshot_archive_add_manifest_checks(&manifest, &files, &mut checks);
    snapshot_archive_add_index_checks(&manifest, &indexes, &mut checks, options.require_index);
    snapshot_archive_add_extra_file_checks(&manifest, &files, &indexes, &mut checks);

    Ok(snapshot_verify_archive_result(
        archive_path,
        options,
        checks,
        entries,
        package,
        manifest,
        indexes,
    ))
}

pub(crate) fn snapshot_verify_archive_result(
    archive_path: &Path,
    options: SnapshotVerifyArchiveOptions,
    checks: Vec<Value>,
    entries: Vec<Value>,
    package: Value,
    manifest: Value,
    indexes: BTreeMap<String, SnapshotIndex>,
) -> Value {
    let summary = snapshot_doctor_summary(&checks);
    let failures = summary
        .get("failures")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    json!({
        "archive": archive_path.display().to_string(),
        "ok": failures == 0,
        "require_index": options.require_index,
        "summary": summary,
        "checks": checks,
        "entries": entries,
        "package": package,
        "manifest": {
            "schema": manifest.get("schema").cloned().unwrap_or(Value::Null),
            "meshx_version": manifest.get("meshx_version").cloned().unwrap_or(Value::Null),
            "created_at_unix_ms": manifest.get("created_at_unix_ms").cloned().unwrap_or(Value::Null),
            "counts": manifest.get("counts").cloned().unwrap_or(Value::Null),
        },
        "indexes": indexes.into_iter().map(|(path, index)| {
            json!({
                "path": path,
                "section": index.section,
                "file": index.file,
                "record_count": index.record_count,
                "indexed_count": index.indexed_count,
                "skipped_without_id": index.skipped_without_id,
            })
        }).collect::<Vec<_>>(),
    })
}

pub(crate) fn snapshot_archive_read_json_entry<R: Read>(
    reader: &mut tar::Entry<'_, R>,
    path: &str,
    max_bytes: u64,
) -> Result<(Value, SnapshotArchiveFile)> {
    let size = reader
        .header()
        .size()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading archive entry size for {path}"))?;
    if size > max_bytes {
        return err(format!(
            "{path} is too large to parse as control JSON: {size} bytes"
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    reader
        .read_to_end(&mut bytes)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {path} from archive"))?;
    let file = SnapshotArchiveFile {
        bytes: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    };
    let value = serde_json::from_slice::<Value>(&bytes)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {path} from archive"))?;
    Ok((value, file))
}

pub(crate) fn snapshot_archive_hash_entry<R: Read>(
    reader: &mut tar::Entry<'_, R>,
) -> Result<SnapshotArchiveFile> {
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .into_diagnostic()
            .wrap_err("reading archive entry")?;
        if read == 0 {
            break;
        }
        bytes = bytes.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    Ok(SnapshotArchiveFile {
        bytes,
        sha256: hex::encode(hasher.finalize()),
    })
}

pub(crate) fn snapshot_archive_discard_entry<R: Read>(
    reader: &mut tar::Entry<'_, R>,
) -> Result<()> {
    io::copy(reader, &mut io::sink())
        .into_diagnostic()
        .wrap_err("discarding duplicate archive entry")?;
    Ok(())
}

pub(crate) fn snapshot_archive_add_package_checks(package: &Value, checks: &mut Vec<Value>) {
    if package.is_null() {
        checks.push(snapshot_doctor_check(
            "warn",
            "package:metadata",
            "archive does not contain .meshx-package.json metadata",
            json!({}),
        ));
        return;
    }
    let schema = package.get("schema").and_then(Value::as_str);
    checks.push(snapshot_doctor_check(
        if schema == Some("meshx.snapshot-package.v1") {
            "pass"
        } else {
            "fail"
        },
        "package:metadata",
        "archive package metadata schema was checked",
        json!({
            "schema": schema,
        }),
    ));
}

pub(crate) fn snapshot_archive_add_manifest_checks(
    manifest: &Value,
    files: &BTreeMap<String, SnapshotArchiveFile>,
    checks: &mut Vec<Value>,
) {
    let Some(files_object) = manifest.get("files").and_then(Value::as_object) else {
        checks.push(snapshot_doctor_check(
            "fail",
            "manifest:files",
            "archive does not contain a snapshot manifest files object",
            json!({}),
        ));
        return;
    };
    checks.push(snapshot_doctor_check(
        "pass",
        "manifest:files",
        "snapshot manifest was parsed from archive",
        json!({
            "files": files_object.len(),
        }),
    ));
    for label in snapshot_stats_section_order(files_object) {
        let Some(entry) = files_object.get(&label) else {
            continue;
        };
        snapshot_archive_add_manifest_file_check(checks, &label, entry, files);
    }
}

pub(crate) fn snapshot_archive_add_manifest_file_check(
    checks: &mut Vec<Value>,
    label: &str,
    entry: &Value,
    files: &BTreeMap<String, SnapshotArchiveFile>,
) {
    let path = match entry.get("path").and_then(Value::as_str) {
        Some(path) => path,
        None => {
            checks.push(snapshot_doctor_check(
                "fail",
                format!("file:{label}"),
                "manifest file entry is missing path",
                json!({
                    "label": label,
                }),
            ));
            return;
        }
    };
    let expected_bytes = entry.get("bytes").and_then(Value::as_u64);
    let expected_sha256 = entry.get("sha256").and_then(Value::as_str);
    let relative = match snapshot_archive_relative_path(Path::new(path)) {
        Ok(relative) => relative,
        Err(error) => {
            checks.push(snapshot_doctor_check(
                "fail",
                format!("file:{label}"),
                "manifest file path is unsafe",
                json!({
                    "label": label,
                    "path": path,
                    "error": error.to_string(),
                }),
            ));
            return;
        }
    };
    let path = snapshot_archive_path_string(&relative);
    let Some(actual) = files.get(&path) else {
        checks.push(snapshot_doctor_check(
            "fail",
            format!("file:{label}"),
            "manifest-listed file is missing from archive",
            json!({
                "label": label,
                "path": path,
                "expected_bytes": expected_bytes,
                "expected_sha256": expected_sha256,
            }),
        ));
        return;
    };
    let ok =
        expected_bytes == Some(actual.bytes) && expected_sha256 == Some(actual.sha256.as_str());
    checks.push(snapshot_doctor_check(
        if ok { "pass" } else { "fail" },
        format!("file:{label}"),
        if ok {
            "archive file matches manifest fingerprint"
        } else {
            "archive file does not match manifest fingerprint"
        },
        json!({
            "label": label,
            "path": path,
            "expected_bytes": expected_bytes,
            "actual_bytes": actual.bytes,
            "expected_sha256": expected_sha256,
            "actual_sha256": actual.sha256,
        }),
    ));
}

pub(crate) fn snapshot_archive_add_index_checks(
    manifest: &Value,
    indexes: &BTreeMap<String, SnapshotIndex>,
    checks: &mut Vec<Value>,
    require_index: bool,
) {
    let Some(files) = manifest.get("files").and_then(Value::as_object) else {
        return;
    };
    let mut seen_sections = BTreeSet::new();
    for (path, index) in indexes {
        let Ok(section) = snapshot_query_section(&index.section) else {
            checks.push(snapshot_doctor_check(
                "fail",
                format!("index:{path}"),
                "index sidecar names an unknown snapshot section",
                json!({
                    "path": path,
                    "section": index.section,
                }),
            ));
            continue;
        };
        seen_sections.insert(section.file_label.to_string());
        let source = files
            .get(section.file_label)
            .map(snapshot_index_file_from_manifest)
            .transpose();
        let source = match source {
            Ok(Some(source)) => source,
            Ok(None) => {
                checks.push(snapshot_doctor_check(
                    "fail",
                    format!("index:{}", section.label),
                    "index sidecar section is not present in manifest",
                    json!({
                        "path": path,
                        "section": section.label,
                    }),
                ));
                continue;
            }
            Err(error) => {
                checks.push(snapshot_doctor_check(
                    "fail",
                    format!("index:{}", section.label),
                    "index source manifest entry could not be read",
                    json!({
                        "path": path,
                        "section": section.label,
                        "error": error.to_string(),
                    }),
                ));
                continue;
            }
        };
        checks.push(snapshot_doctor_check(
            if snapshot_index_matches_source(index, section, &source) {
                "pass"
            } else {
                "fail"
            },
            format!("index:{}", section.label),
            "index sidecar freshness was checked against manifest",
            json!({
                "path": path,
                "section": section.label,
                "record_count": index.record_count,
                "indexed_count": index.indexed_count,
                "skipped_without_id": index.skipped_without_id,
            }),
        ));
    }

    for label in snapshot_stats_section_order(files) {
        let Ok(section) = snapshot_query_section(&label) else {
            continue;
        };
        if section.kind != SnapshotQuerySectionKind::Jsonl
            || seen_sections.contains(section.file_label)
            || !snapshot_archive_section_needs_index(files.get(&label))
        {
            continue;
        }
        checks.push(snapshot_doctor_check(
            if require_index { "fail" } else { "warn" },
            format!("index:{}", section.label),
            "archive does not contain a JSONL index sidecar",
            json!({
                "section": section.label,
                "expected_path": format!(".meshx-index/{}.json", section.file_label),
            }),
        ));
    }
}

pub(crate) fn snapshot_archive_section_needs_index(entry: Option<&Value>) -> bool {
    entry
        .and_then(|entry| entry.get("bytes"))
        .and_then(Value::as_u64)
        .is_some_and(|bytes| bytes > 0)
}

pub(crate) fn snapshot_archive_add_extra_file_checks(
    manifest: &Value,
    files: &BTreeMap<String, SnapshotArchiveFile>,
    indexes: &BTreeMap<String, SnapshotIndex>,
    checks: &mut Vec<Value>,
) {
    let mut expected = BTreeSet::from([
        "manifest.json".to_string(),
        ".meshx-package.json".to_string(),
    ]);
    if let Some(manifest_files) = manifest.get("files").and_then(Value::as_object) {
        for entry in manifest_files.values() {
            if let Some(path) = entry.get("path").and_then(Value::as_str)
                && let Ok(relative) = snapshot_archive_relative_path(Path::new(path))
            {
                expected.insert(snapshot_archive_path_string(&relative));
            }
        }
    }
    expected.extend(indexes.keys().cloned());

    for path in files.keys() {
        if expected.contains(path) {
            continue;
        }
        checks.push(snapshot_doctor_check(
            "fail",
            format!("entry:{path}"),
            "archive contains a file not referenced by the snapshot manifest or valid indexes",
            json!({
                "path": path,
            }),
        ));
    }
}

pub(crate) fn snapshot_pack(
    dir: &Path,
    archive_path: &Path,
    options: SnapshotPackOptions,
) -> Result<Value> {
    if archive_path.exists() && !options.force {
        return err(format!(
            "{} already exists. Re-run with --force to replace it.",
            archive_path.display()
        ));
    }

    let verify = if options.verify {
        let value = verify_snapshot(dir)?;
        if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("snapshot failed manifest verification");
        }
        value
    } else {
        Value::Null
    };

    let manifest = read_snapshot_manifest(dir)?;
    let (entries, skipped_indexes) =
        snapshot_package_entries(dir, &manifest, options.include_indexes)?;
    let package_bytes =
        snapshot_package_metadata(dir, &manifest, &entries, &skipped_indexes, &options)?;
    let (temp_path, file) = create_export_spool(Some(archive_path))?;
    let write_result =
        snapshot_write_package_archive(file, &entries, &package_bytes, options.compression_level);
    if let Err(error) = write_result {
        cleanup_export_spool_best_effort(&temp_path);
        return Err(error);
    }
    move_temp_output(
        &temp_path,
        archive_path,
        &archive_path.display().to_string(),
    )?;
    let archive_bytes = fs::metadata(archive_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", archive_path.display()))?
        .len();

    Ok(json!({
        "dir": dir.display().to_string(),
        "archive": archive_path.display().to_string(),
        "archive_bytes": archive_bytes,
        "verified": options.verify,
        "include_indexes": options.include_indexes,
        "compression": {
            "format": "zstd",
            "level": options.compression_level,
        },
        "entries": entries.iter().map(snapshot_package_entry_value).collect::<Vec<_>>(),
        "skipped_indexes": skipped_indexes,
        "verify": verify,
    }))
}

pub(crate) fn snapshot_unpack(
    archive_path: &Path,
    dir: &Path,
    options: SnapshotUnpackOptions,
) -> Result<Value> {
    let archive_verify = if options.verify {
        let value = snapshot_verify_archive(
            archive_path,
            SnapshotVerifyArchiveOptions {
                require_index: false,
            },
        )?;
        if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("snapshot archive failed verification");
        }
        value
    } else {
        Value::Null
    };

    prepare_snapshot_dir(dir, options.force)?;
    let archive_file = fs::File::open(archive_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("opening {}", archive_path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(archive_file)
        .into_diagnostic()
        .wrap_err_with(|| format!("decompressing {}", archive_path.display()))?;
    let mut archive = tar::Archive::new(decoder);
    let mut unpacked = Vec::new();
    let mut progress = Progress::counter("unpack archive");
    for entry in archive
        .entries()
        .into_diagnostic()
        .wrap_err_with(|| format!("reading tar entries from {}", archive_path.display()))?
    {
        let mut entry = entry
            .into_diagnostic()
            .wrap_err_with(|| format!("reading tar entry from {}", archive_path.display()))?;
        let raw_path = entry
            .path()
            .into_diagnostic()
            .wrap_err("reading archive entry path")?
            .into_owned();
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() && snapshot_archive_is_root_dir_path(&raw_path) {
            continue;
        }
        let relative = snapshot_archive_relative_path(&raw_path)?;
        let output_path = dir.join(&relative);

        if entry_type.is_dir() {
            fs::create_dir_all(&output_path)
                .into_diagnostic()
                .wrap_err_with(|| format!("creating {}", output_path.display()))?;
            unpacked.push(json!({
                "path": snapshot_archive_path_string(&relative),
                "kind": "directory",
            }));
        } else if entry_type.is_file() {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("creating {}", parent.display()))?;
            }
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&output_path)
                .into_diagnostic()
                .wrap_err_with(|| format!("creating {}", output_path.display()))?;
            let bytes = io::copy(&mut entry, &mut output)
                .into_diagnostic()
                .wrap_err_with(|| format!("extracting {}", output_path.display()))?;
            output
                .flush()
                .into_diagnostic()
                .wrap_err_with(|| format!("flushing {}", output_path.display()))?;
            unpacked.push(json!({
                "path": snapshot_archive_path_string(&relative),
                "kind": "file",
                "bytes": bytes,
            }));
        } else {
            return err(format!(
                "unsupported archive entry {} with type {:?}",
                snapshot_archive_path_string(&relative),
                entry_type
            ));
        }
        progress.inc();
    }
    progress.finish();

    let verify = if options.verify {
        let value = verify_snapshot(dir)?;
        if !value.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("unpacked snapshot failed manifest verification");
        }
        value
    } else {
        Value::Null
    };

    Ok(json!({
        "archive": archive_path.display().to_string(),
        "dir": dir.display().to_string(),
        "verified": options.verify,
        "entries": unpacked,
        "archive_verify": archive_verify,
        "verify": verify,
    }))
}

pub(crate) fn snapshot_package_entries(
    dir: &Path,
    manifest: &Value,
    include_indexes: bool,
) -> Result<(Vec<SnapshotPackageEntry>, Vec<Value>)> {
    let Some(files) = manifest.get("files").and_then(Value::as_object) else {
        return err("snapshot manifest does not contain files object");
    };
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    snapshot_package_push_entry(
        &mut entries,
        &mut seen,
        dir.join("manifest.json"),
        PathBuf::from("manifest.json"),
        "manifest",
    )?;

    for label in snapshot_stats_section_order(files) {
        let Some(entry) = files.get(&label) else {
            continue;
        };
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| miette!("manifest file entry {label} is missing path"))?;
        let source_path = safe_snapshot_file_path(dir, path)?;
        let archive_path = snapshot_archive_relative_path(Path::new(path))?;
        snapshot_package_push_entry(
            &mut entries,
            &mut seen,
            source_path,
            archive_path,
            "snapshot-file",
        )?;
    }

    let mut skipped_indexes = Vec::new();
    if include_indexes {
        snapshot_package_add_indexes(dir, files, &mut entries, &mut seen, &mut skipped_indexes)?;
    }
    Ok((entries, skipped_indexes))
}

pub(crate) fn snapshot_package_add_indexes(
    dir: &Path,
    files: &Map<String, Value>,
    entries: &mut Vec<SnapshotPackageEntry>,
    seen: &mut BTreeSet<String>,
    skipped_indexes: &mut Vec<Value>,
) -> Result<()> {
    for label in snapshot_stats_section_order(files) {
        let Ok(section) = snapshot_query_section(&label) else {
            continue;
        };
        if section.kind != SnapshotQuerySectionKind::Jsonl {
            continue;
        }
        let index_path = snapshot_index_path(dir, section);
        let index_path_text = index_path.display().to_string();
        let Some(index) = (match read_snapshot_index_if_present(&index_path) {
            Ok(index) => index,
            Err(error) => {
                skipped_indexes.push(json!({
                    "section": section.label,
                    "index_path": index_path_text,
                    "reason": "unreadable",
                    "error": error.to_string(),
                }));
                continue;
            }
        }) else {
            continue;
        };
        let source = snapshot_index_source_file(dir, section)?;
        if !snapshot_index_matches_source(&index, section, &source) {
            skipped_indexes.push(json!({
                "section": section.label,
                "index_path": index_path_text,
                "reason": "stale",
            }));
            continue;
        }
        snapshot_package_push_entry(
            entries,
            seen,
            index_path,
            PathBuf::from(".meshx-index").join(format!("{}.json", section.file_label)),
            "index",
        )?;
    }
    Ok(())
}

pub(crate) fn snapshot_package_push_entry(
    entries: &mut Vec<SnapshotPackageEntry>,
    seen: &mut BTreeSet<String>,
    source_path: PathBuf,
    archive_path: PathBuf,
    kind: &'static str,
) -> Result<()> {
    let archive_path = snapshot_archive_relative_path(&archive_path)?;
    let archive_key = snapshot_archive_path_string(&archive_path);
    if !seen.insert(archive_key.clone()) {
        return err(format!(
            "snapshot package would contain duplicate path {archive_key}"
        ));
    }
    let metadata = fs::metadata(&source_path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", source_path.display()))?;
    if !metadata.is_file() {
        return err(format!("{} is not a regular file", source_path.display()));
    }
    entries.push(SnapshotPackageEntry {
        archive_path,
        source_path,
        kind,
    });
    Ok(())
}

pub(crate) fn snapshot_package_metadata(
    dir: &Path,
    manifest: &Value,
    entries: &[SnapshotPackageEntry],
    skipped_indexes: &[Value],
    options: &SnapshotPackOptions,
) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(&json!({
        "schema": "meshx.snapshot-package.v1",
        "meshx_version": VERSION,
        "created_at_unix_ms": now_millis(),
        "source_dir": dir.display().to_string(),
        "compression": {
            "format": "zstd",
            "level": options.compression_level,
        },
        "include_indexes": options.include_indexes,
        "snapshot": {
            "schema": manifest.get("schema").cloned().unwrap_or(Value::Null),
            "meshx_version": manifest.get("meshx_version").cloned().unwrap_or(Value::Null),
            "created_at_unix_ms": manifest.get("created_at_unix_ms").cloned().unwrap_or(Value::Null),
            "counts": manifest.get("counts").cloned().unwrap_or(Value::Null),
        },
        "entries": entries.iter().map(snapshot_package_entry_value).collect::<Vec<_>>(),
        "skipped_indexes": skipped_indexes,
    }))
    .into_diagnostic()
    .wrap_err("serializing snapshot package metadata")
}

pub(crate) fn snapshot_write_package_archive(
    file: fs::File,
    entries: &[SnapshotPackageEntry],
    package_bytes: &[u8],
    compression_level: i32,
) -> Result<()> {
    let encoder = zstd::stream::write::Encoder::new(file, compression_level)
        .into_diagnostic()
        .wrap_err("creating zstd encoder")?;
    let mut builder = tar::Builder::new(encoder);
    snapshot_append_package_metadata(&mut builder, package_bytes)?;
    let mut progress = Progress::sized("pack archive", entries.len() as u64);
    for entry in entries {
        builder
            .append_path_with_name(&entry.source_path, &entry.archive_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "packing {} as {}",
                    entry.source_path.display(),
                    snapshot_archive_path_string(&entry.archive_path)
                )
            })?;
        progress.inc();
    }
    progress.finish();
    let encoder = builder
        .into_inner()
        .into_diagnostic()
        .wrap_err("finishing tar archive")?;
    let mut file = encoder
        .finish()
        .into_diagnostic()
        .wrap_err("finishing zstd archive")?;
    file.flush().into_diagnostic().wrap_err("flushing archive")
}

pub(crate) fn snapshot_append_package_metadata<W: Write>(
    builder: &mut tar::Builder<W>,
    package_bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(package_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(now_millis() / 1000);
    header.set_cksum();
    builder
        .append_data(&mut header, ".meshx-package.json", &mut &package_bytes[..])
        .into_diagnostic()
        .wrap_err("packing .meshx-package.json")
}

pub(crate) fn snapshot_package_entry_value(entry: &SnapshotPackageEntry) -> Value {
    let bytes = fs::metadata(&entry.source_path)
        .ok()
        .map(|metadata| metadata.len());
    json!({
        "path": snapshot_archive_path_string(&entry.archive_path),
        "kind": entry.kind,
        "bytes": bytes,
    })
}

pub(crate) fn snapshot_archive_relative_path(path: &Path) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    let mut saw_component = false;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                relative.push(part);
                saw_component = true;
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return err(format!(
                    "archive entry path must stay relative: {}",
                    path.display()
                ));
            }
        }
    }
    if !saw_component {
        return err("archive entry path must not be empty");
    }
    Ok(relative)
}

pub(crate) fn snapshot_archive_is_root_dir_path(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::CurDir))
}

pub(crate) fn snapshot_archive_path_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_archive_relative_path_normalizes_current_dir_segments() -> Result<()> {
        assert_eq!(
            snapshot_archive_relative_path(Path::new("./contacts.jsonl"))?,
            PathBuf::from("contacts.jsonl")
        );
        assert_eq!(
            snapshot_archive_relative_path(Path::new("nested/./contacts.jsonl"))?,
            PathBuf::from("nested/contacts.jsonl")
        );
        Ok(())
    }

    #[test]
    fn snapshot_archive_relative_path_rejects_unsafe_paths() {
        assert!(snapshot_archive_relative_path(Path::new("../contacts.jsonl")).is_err());
        assert!(snapshot_archive_relative_path(Path::new("/tmp/contacts.jsonl")).is_err());
        assert!(snapshot_archive_relative_path(Path::new(".")).is_err());
    }

    #[test]
    fn snapshot_archive_is_root_dir_path_accepts_only_current_dir_segments() {
        assert!(snapshot_archive_is_root_dir_path(Path::new(".")));
        assert!(snapshot_archive_is_root_dir_path(Path::new("./.")));
        assert!(!snapshot_archive_is_root_dir_path(Path::new(
            "./contacts.jsonl"
        )));
    }
}
