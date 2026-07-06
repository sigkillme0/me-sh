use crate::prelude::*;

pub(crate) fn snapshot_create_dir(matches: &ArgMatches) -> PathBuf {
    matches
        .get_one::<String>("dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("mesh-snapshot-{}", now_millis())))
}

pub(crate) async fn create_snapshot(
    runtime: &Runtime,
    dir: &Path,
    force: bool,
    options: SnapshotCreateOptions,
) -> Result<Value> {
    check_snapshot_create_target(dir, force)?;
    let temp_dir = create_snapshot_temp_dir(dir)?;
    let result = match build_snapshot(runtime, &temp_dir, options).await {
        Ok(manifest) => commit_snapshot_dir(&temp_dir, dir).map(|()| manifest),
        Err(error) => Err(error),
    };
    if result.is_err() {
        fs::remove_dir_all(&temp_dir).ok();
    }
    result
}

/// Same acceptance rules as [`prepare_snapshot_dir`], but without creating the
/// target directory: the snapshot is built in a temp sibling and renamed into
/// place, so a crash mid-create never leaves a manifest-less final directory.
pub(crate) fn check_snapshot_create_target(dir: &Path, force: bool) -> Result<()> {
    if dir.exists() {
        if !force {
            return err(format!(
                "{} already exists. Use --force only for an existing empty directory.",
                dir.display()
            ));
        }
        let mut entries = fs::read_dir(dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {}", dir.display()))?;
        if entries.next().transpose().into_diagnostic()?.is_some() {
            return err(format!("{} exists and is not empty", dir.display()));
        }
    } else if let Some(parent) = dir.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err_with(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

pub(crate) fn create_snapshot_temp_dir(dir: &Path) -> Result<PathBuf> {
    for attempt in 0..100 {
        let path = snapshot_temp_dir_path(dir, attempt);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("creating {}", path.display()));
            }
        }
    }
    err("could not create a unique snapshot temp directory")
}

pub(crate) fn snapshot_temp_dir_path(dir: &Path, attempt: u32) -> PathBuf {
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "snapshot".to_string());
    let temp_name = format!(
        ".{name}.meshx-snapshot-tmp-{}-{}-{attempt}",
        std::process::id(),
        unix_millis()
    );
    match dir.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

pub(crate) fn commit_snapshot_dir(temp_dir: &Path, dir: &Path) -> Result<()> {
    if dir.exists() {
        // check_snapshot_create_target only accepts an existing dir when it is
        // empty (--force); remove_dir refuses non-empty dirs, so this cannot
        // discard data racing in after the check.
        fs::remove_dir(dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("replacing {}", dir.display()))?;
    }
    fs::rename(temp_dir, dir)
        .into_diagnostic()
        .wrap_err_with(|| format!("moving {} to {}", temp_dir.display(), dir.display()))
}

pub(crate) async fn build_snapshot(
    runtime: &Runtime,
    dir: &Path,
    options: SnapshotCreateOptions,
) -> Result<Value> {
    let (contacts_file, contact_ids, total) = write_snapshot_contacts(runtime, dir)
        .await
        .wrap_err("fetching contacts for snapshot")?;

    let groups_data = runtime
        .call_tool(route::GET_GROUPS, json!({}))
        .await
        .wrap_err("fetching groups for snapshot")?;
    let groups = snapshot_group_rows_from_response(&groups_data)?;

    let mut full_contact_ids = Vec::new();
    let mut full_contacts_file = None;
    let mut full_contacts_count = 0_usize;
    if options.full_contacts {
        full_contact_ids = snapshot_full_contact_ids(&options, &contact_ids)?;
        let (file, count) =
            write_snapshot_full_contacts(runtime, dir, &full_contact_ids, options.full_concurrency)
                .await?;
        full_contacts_file = Some(file);
        full_contacts_count = count;
    }

    let moments_result = if options.moments {
        Some(
            write_snapshot_moments(runtime, dir, &options)
                .await
                .wrap_err("fetching moments for snapshot")?,
        )
    } else {
        None
    };

    let groups_json = serde_json::to_string_pretty(&Value::Array(groups.clone()))
        .into_diagnostic()
        .wrap_err("serializing snapshot groups")?;
    let routes_json = serde_json::to_string_pretty(&routes_value())
        .into_diagnostic()
        .wrap_err("serializing snapshot routes")?;

    let groups_file = write_snapshot_file(dir, "groups.json", groups_json.as_bytes())?;
    let routes_file = write_snapshot_file(dir, "routes.json", routes_json.as_bytes())?;

    let mut counts = Map::new();
    counts.insert(
        "contacts".to_string(),
        Value::Number(Number::from(total as u64)),
    );
    counts.insert(
        "groups".to_string(),
        Value::Number(Number::from(groups.len() as u64)),
    );
    if options.full_contacts {
        counts.insert(
            "full_contacts".to_string(),
            Value::Number(Number::from(full_contacts_count as u64)),
        );
    }
    if let Some(result) = &moments_result {
        counts.set("moments", Value::Object(result.counts.clone()));
    }

    let mut files = Map::new();
    files.insert("contacts".to_string(), contacts_file);
    files.insert("groups".to_string(), groups_file);
    files.insert("routes".to_string(), routes_file);
    if let Some(file) = full_contacts_file {
        files.insert("full_contacts".to_string(), file);
    }
    if let Some(result) = &moments_result {
        files.extend(result.files.clone());
    }

    let mut manifest = Map::new();
    manifest.insert(
        "schema".to_string(),
        Value::String("meshx.snapshot.v1".to_string()),
    );
    manifest.insert(
        "meshx_version".to_string(),
        Value::String(VERSION.to_string()),
    );
    manifest.insert(
        "created_at_unix_ms".to_string(),
        Value::Number(Number::from(now_millis())),
    );
    manifest.insert(
        "source".to_string(),
        json!({
            "api_base": runtime.api_base,
            "mcp_base": runtime.mcp_base,
        }),
    );
    manifest.insert(
        "search".to_string(),
        json!({
            "total": total,
            "observed_limit_cap": SEARCH_LIMIT_MAX,
            "pagination": "exclude_contact_ids",
        }),
    );
    if options.full_contacts {
        manifest.insert(
            "full_contacts".to_string(),
            json!({
                "selected_count": full_contact_ids.len(),
                "selection": if options.full_contact_ids.is_empty() { "contacts_jsonl_ids" } else { "explicit_ids" },
                "limit": options.full_limit,
                "concurrency": options.full_concurrency,
                "ids": full_contact_ids,
            }),
        );
    }
    if let Some(result) = &moments_result {
        manifest.insert(
            "moments".to_string(),
            json!({
                "start": options.moments_start.clone(),
                "end": options.moments_end.clone(),
                "contact_ids": options.moments_contact_ids.clone(),
                "page_size": options.moments_limit,
                "routes": result.routes.clone(),
            }),
        );
    }
    manifest.set("counts", Value::Object(counts));
    manifest.set("files", Value::Object(files));
    let manifest = Value::Object(manifest);

    let manifest_text = serde_json::to_string_pretty(&manifest)
        .into_diagnostic()
        .wrap_err("serializing snapshot manifest")?;
    fs::write(dir.join("manifest.json"), manifest_text)
        .into_diagnostic()
        .wrap_err_with(|| format!("writing {}", dir.join("manifest.json").display()))?;
    Ok(manifest)
}

pub(crate) fn snapshot_group_rows_from_response(data: &Value) -> Result<Vec<Value>> {
    let rows = snapshot_array_rows_for_keys(data, &["groups", "results", "items", "data"])
        .ok_or_else(|| miette!("groups response missing array rows"))?;
    if let Some(index) = rows.iter().position(|row| !row.is_object()) {
        return err(format!(
            "groups response row {} is not an object",
            index + 1
        ));
    }
    Ok(rows)
}

pub(crate) fn snapshot_array_rows_for_keys(data: &Value, keys: &[&str]) -> Option<Vec<Value>> {
    match data {
        Value::Array(items) => Some(items.clone()),
        Value::Object(object) => keys
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_array).cloned()),
        _ => None,
    }
}

pub(crate) fn snapshot_moment_rows_from_response(
    route: &SnapshotMomentRoute,
    data: &Value,
) -> Result<Vec<Value>> {
    let rows = snapshot_array_rows_for_keys(data, &["results", "items", "data"])
        .ok_or_else(|| miette!("{} response missing array rows", route.route))?;
    if let Some(index) = rows.iter().position(|row| !row.is_object()) {
        return err(format!(
            "{} response row {} is not an object",
            route.route,
            index + 1
        ));
    }
    Ok(rows)
}

pub(crate) async fn write_snapshot_contacts(
    runtime: &Runtime,
    dir: &Path,
) -> Result<(Value, Vec<u64>, usize)> {
    let name = "contacts.jsonl";
    let path = safe_snapshot_file_path(dir, name)?;
    let mut file = fs::File::create(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("creating {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_usize;
    let mut ids = Vec::new();
    let mut progress = Progress::counter("snapshot contacts");

    let count = export_all_contacts_each(runtime, Map::new(), SEARCH_LIMIT_MAX, |row| {
        let id = contact_id_from_value(&row)
            .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
        write_jsonl_row_hashed(&mut file, &row, &mut hasher, &mut bytes)
            .wrap_err_with(|| format!("writing {}", path.display()))?;
        ids.push(id);
        progress.inc();
        Ok(())
    })
    .await?;
    progress.finish();

    file.flush()
        .into_diagnostic()
        .wrap_err_with(|| format!("flushing {}", path.display()))?;

    Ok((
        snapshot_file_entry(name, bytes, hasher.finalize()),
        ids,
        count,
    ))
}

pub(crate) async fn write_snapshot_full_contacts(
    runtime: &Runtime,
    dir: &Path,
    ids: &[u64],
    concurrency: usize,
) -> Result<(Value, usize)> {
    let name = "full-contacts.jsonl";
    let path = safe_snapshot_file_path(dir, name)?;
    let mut file = fs::File::create(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("creating {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_usize;
    let mut written = 0_usize;
    let mut progress = Progress::sized("snapshot full contacts", ids.len() as u64);

    fetch_contacts_each(runtime, ids, concurrency, |id, contact| {
        let row = normalize_full_contact(id, contact);
        write_jsonl_row_hashed(&mut file, &row, &mut hasher, &mut bytes)
            .wrap_err_with(|| format!("writing {}", path.display()))?;
        written += 1;
        progress.inc();
        Ok(())
    })
    .await?;
    progress.finish();

    file.flush()
        .into_diagnostic()
        .wrap_err_with(|| format!("flushing {}", path.display()))?;

    Ok((snapshot_file_entry(name, bytes, hasher.finalize()), written))
}

pub(crate) async fn write_snapshot_moments(
    runtime: &Runtime,
    dir: &Path,
    options: &SnapshotCreateOptions,
) -> Result<SnapshotMomentsResult> {
    let mut files = Map::new();
    let mut counts = Map::new();
    let mut routes = Vec::new();

    for route in SNAPSHOT_MOMENT_ROUTES {
        let written = write_snapshot_moment_route(runtime, dir, options, route).await?;
        files.insert(written.label.to_string(), written.file);
        counts.insert(
            written.label.to_string(),
            Value::Number(Number::from(written.count as u64)),
        );
        routes.push(written.meta);
    }

    Ok(SnapshotMomentsResult {
        files,
        counts,
        routes,
    })
}

pub(crate) async fn write_snapshot_moment_route(
    runtime: &Runtime,
    dir: &Path,
    options: &SnapshotCreateOptions,
    route: &SnapshotMomentRoute,
) -> Result<SnapshotMomentWrite> {
    let path = safe_snapshot_file_path(dir, route.file_name)?;
    let mut file = fs::File::create(&path)
        .into_diagnostic()
        .wrap_err_with(|| format!("creating {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_usize;
    let mut count = 0_usize;
    let mut pages = 0_usize;

    match route.kind {
        SnapshotMomentKind::DateWindow => {
            let payload = snapshot_moment_date_payload(options)?;
            let data = runtime
                .call_tool(route.route, Value::Object(payload))
                .await
                .wrap_err_with(|| format!("fetching {}", route.route))?;
            pages = 1;
            for row in snapshot_moment_rows_from_response(route, &data)? {
                write_jsonl_row_hashed(&mut file, &row, &mut hasher, &mut bytes)
                    .wrap_err_with(|| format!("writing {}", path.display()))?;
                count += 1;
            }
        }
        SnapshotMomentKind::Paged => {
            let mut page = 1_usize;
            loop {
                let payload = snapshot_moment_paged_payload(options, page);
                let data = runtime
                    .call_tool(route.route, Value::Object(payload))
                    .await
                    .wrap_err_with(|| format!("fetching {} page {page}", route.route))?;
                pages += 1;
                let rows = snapshot_moment_rows_from_response(route, &data)?;
                for row in &rows {
                    write_jsonl_row_hashed(&mut file, row, &mut hasher, &mut bytes)
                        .wrap_err_with(|| format!("writing {}", path.display()))?;
                    count += 1;
                }
                if !moment_response_has_next(&data) {
                    break;
                }
                if rows.is_empty() {
                    return err(format!(
                        "{} returned has_next=true with no rows on page {page}",
                        route.route
                    ));
                }
                page += 1;
            }
        }
    }

    file.flush()
        .into_diagnostic()
        .wrap_err_with(|| format!("flushing {}", path.display()))?;

    Ok(SnapshotMomentWrite {
        label: route.label,
        file: snapshot_file_entry(route.file_name, bytes, hasher.finalize()),
        count,
        meta: json!({
            "label": route.label,
            "route": route.route,
            "file": route.file_name,
            "kind": match route.kind {
                SnapshotMomentKind::DateWindow => "date_window",
                SnapshotMomentKind::Paged => "paged",
            },
            "pages": pages,
            "rows": count,
        }),
    })
}

pub(crate) fn snapshot_moment_date_payload(
    options: &SnapshotCreateOptions,
) -> Result<Map<String, Value>> {
    let start = options
        .moments_start
        .as_ref()
        .ok_or_else(|| miette!("missing --moments-start"))?;
    let end = options
        .moments_end
        .as_ref()
        .ok_or_else(|| miette!("missing --moments-end"))?;
    let mut payload = snapshot_moment_base_payload(options);
    payload.set("start", start.clone());
    payload.set("end", end.clone());
    Ok(payload)
}

pub(crate) fn snapshot_moment_paged_payload(
    options: &SnapshotCreateOptions,
    page: usize,
) -> Map<String, Value> {
    let mut payload = snapshot_moment_base_payload(options);
    payload.insert(
        "limit".to_string(),
        Value::Number(Number::from(options.moments_limit as u64)),
    );
    payload.set("page", page as u64);
    payload
}

pub(crate) fn snapshot_moment_base_payload(options: &SnapshotCreateOptions) -> Map<String, Value> {
    let mut payload = Map::new();
    if !options.moments_contact_ids.is_empty() {
        payload.insert(
            "contact_ids".to_string(),
            json!(options.moments_contact_ids),
        );
    }
    payload
}

pub(crate) fn snapshot_moment_plan(options: &SnapshotCreateOptions) -> Vec<Value> {
    SNAPSHOT_MOMENT_ROUTES
        .iter()
        .map(|route| {
            let mut payload = snapshot_moment_base_payload(options);
            match route.kind {
                SnapshotMomentKind::DateWindow => {
                    payload.insert(
                        "start".to_string(),
                        Value::String(options.moments_start.clone().unwrap_or_default()),
                    );
                    payload.insert(
                        "end".to_string(),
                        Value::String(options.moments_end.clone().unwrap_or_default()),
                    );
                }
                SnapshotMomentKind::Paged => {
                    payload.insert(
                        "limit".to_string(),
                        Value::Number(Number::from(options.moments_limit as u64)),
                    );
                    payload.set("page", "1..has_next".to_string());
                }
            };
            json!({
                "route": format!("/tools/v2{}", route.route),
                "payload": Value::Object(payload),
                "local_file": route.file_name,
                "purpose": "write moment rows as hashed JSONL",
            })
        })
        .collect()
}

pub(crate) fn snapshot_moment_route_by_label(label: &str) -> Option<&'static SnapshotMomentRoute> {
    SNAPSHOT_MOMENT_ROUTES
        .iter()
        .find(|route| route.label == label)
}

pub(crate) fn write_snapshot_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<Value> {
    let path = safe_snapshot_file_path(dir, name)?;
    fs::write(&path, bytes)
        .into_diagnostic()
        .wrap_err_with(|| format!("writing {}", path.display()))?;
    Ok(snapshot_file_entry(
        name,
        bytes.len(),
        Sha256::digest(bytes),
    ))
}

pub(crate) fn snapshot_timeline_moments_changed(diff: &Value) -> u64 {
    let Some(moments) = diff.get("moments").and_then(Value::as_object) else {
        return 0;
    };
    moments
        .values()
        .filter(|moment| {
            let old_available = moment
                .get("old_available")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let new_available = moment
                .get("new_available")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let changed = moment
                .get("changed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            old_available != new_available || changed
        })
        .count() as u64
}

pub(crate) fn create_snapshot_index(dir: &Path, options: SnapshotIndexOptions) -> Result<Value> {
    let source = snapshot_index_source_file(dir, options.section)?;
    let index_path = snapshot_index_path(dir, options.section);
    if !options.force
        && let Some(existing) = read_snapshot_index_if_present(&index_path)?
        && snapshot_index_matches_source(&existing, options.section, &source)
    {
        return Ok(snapshot_index_summary(&existing, &index_path, true));
    }

    let index = build_snapshot_index(dir, options.section, source)?;
    write_snapshot_index(&index_path, &index)?;
    Ok(snapshot_index_summary(&index, &index_path, false))
}

pub(crate) fn write_snapshot_index(path: &Path, index: &SnapshotIndex) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .into_diagnostic()
            .wrap_err_with(|| format!("creating {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(index)
        .into_diagnostic()
        .wrap_err("serializing snapshot index")?;
    let temp_path =
        path.with_extension(format!("json.tmp-{}-{}", std::process::id(), unix_millis()));
    fs::write(&temp_path, text)
        .into_diagnostic()
        .wrap_err_with(|| format!("writing {}", temp_path.display()))?;
    move_temp_output(&temp_path, path, &path.display().to_string())
}

pub(crate) fn write_snapshot_query(
    matches: &ArgMatches,
    dir: &Path,
    options: SnapshotQueryOptions,
) -> Result<()> {
    let format = output_format_from_matches(matches)?;
    if options.section.kind == SnapshotQuerySectionKind::Jsonl && format != OutputFormat::Table {
        write_snapshot_query_jsonl_section(matches, dir, &options, format)
    } else {
        write_value(matches, query_snapshot(dir, options)?)
    }
}

pub(crate) fn write_snapshot_query_jsonl_section(
    matches: &ArgMatches,
    dir: &Path,
    options: &SnapshotQueryOptions,
    format: OutputFormat,
) -> Result<()> {
    prepare_snapshot_query(dir, options)?;
    match format {
        OutputFormat::Json => write_snapshot_query_json_array(matches, dir, options, true),
        OutputFormat::CompactJson => write_snapshot_query_json_array(matches, dir, options, false),
        OutputFormat::Jsonl => write_snapshot_query_jsonl(matches, dir, options),
        OutputFormat::Csv => write_snapshot_query_delimited(matches, dir, options, b','),
        OutputFormat::Tsv => write_snapshot_query_delimited(matches, dir, options, b'\t'),
        OutputFormat::Table => write_value(matches, query_snapshot(dir, options.clone())?),
    }
}

pub(crate) fn write_snapshot_query_json_array(
    matches: &ArgMatches,
    dir: &Path,
    options: &SnapshotQueryOptions,
    pretty: bool,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result = write_snapshot_query_json_array_to(dir, options, pretty, &mut file)
            .and_then(|_| {
                file.flush()
                    .into_diagnostic()
                    .wrap_err_with(|| format!("flushing {}", temp_path.display()))
            });
        if let Err(error) = write_result {
            cleanup_export_spool_best_effort(&temp_path);
            return Err(error);
        }
        move_temp_output(&temp_path, output_path, path)
    } else {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        write_snapshot_query_json_array_to(dir, options, pretty, &mut stdout)?;
        stdout.flush().into_diagnostic().wrap_err("flushing stdout")
    }
}

pub(crate) fn write_snapshot_query_json_array_to<W: Write>(
    dir: &Path,
    options: &SnapshotQueryOptions,
    pretty: bool,
    writer: &mut W,
) -> Result<()> {
    writer.write_all(b"[").into_diagnostic()?;
    let mut first = true;
    snapshot_query_jsonl_each(dir, options, |row| {
        write_json_array_row(writer, &row, &mut first, pretty)
    })?;
    if pretty && !first {
        writer.write_all(b"\n").into_diagnostic()?;
    }
    writer.write_all(b"]\n").into_diagnostic()?;
    Ok(())
}

pub(crate) fn write_snapshot_query_jsonl(
    matches: &ArgMatches,
    dir: &Path,
    options: &SnapshotQueryOptions,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result =
            snapshot_query_jsonl_each(dir, options, |row| write_jsonl_row(&mut file, &row))
                .and_then(|_| {
                    file.flush()
                        .into_diagnostic()
                        .wrap_err_with(|| format!("flushing {}", temp_path.display()))
                });
        if let Err(error) = write_result {
            cleanup_export_spool_best_effort(&temp_path);
            return Err(error);
        }
        move_temp_output(&temp_path, output_path, path)
    } else {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        snapshot_query_jsonl_each(dir, options, |row| write_jsonl_row(&mut stdout, &row))?;
        stdout.flush().into_diagnostic().wrap_err("flushing stdout")
    }
}

pub(crate) fn write_snapshot_query_delimited(
    matches: &ArgMatches,
    dir: &Path,
    options: &SnapshotQueryOptions,
    delimiter: u8,
) -> Result<()> {
    let output_path = matches.get_one::<String>("output").map(Path::new);
    let (spool_path, mut spool_file) = create_export_spool(output_path)?;
    let mut headers = BTreeSet::new();
    let export_result = snapshot_query_jsonl_each(dir, options, |row| {
        collect_row_headers(&row, &mut headers)?;
        write_jsonl_row(&mut spool_file, &row)
    })
    .and_then(|_| {
        spool_file
            .flush()
            .into_diagnostic()
            .wrap_err_with(|| format!("flushing {}", spool_path.display()))
    });

    if let Err(error) = export_result {
        cleanup_export_spool_best_effort(&spool_path);
        return Err(error);
    }

    let headers = headers.into_iter().collect::<Vec<_>>();
    match write_delimited_spool(matches, &spool_path, &headers, delimiter) {
        Ok(()) => cleanup_export_spool(&spool_path),
        Err(error) => {
            cleanup_export_spool_best_effort(&spool_path);
            Err(error)
        }
    }
}

pub(crate) fn snapshot_moment_query_section(
    label: &'static str,
    file_name: &'static str,
) -> SnapshotQuerySection {
    SnapshotQuerySection {
        label,
        file_label: label,
        file_name,
        kind: SnapshotQuerySectionKind::Jsonl,
    }
}

pub(crate) fn snapshot_moment_fingerprint_diffs(old_dir: &Path, new_dir: &Path) -> Result<Value> {
    let mut moments = Map::new();
    for route in SNAPSHOT_MOMENT_ROUTES {
        moments.insert(
            route.label.to_string(),
            optional_snapshot_file_fingerprint_diff(old_dir, new_dir, route.label)?,
        );
    }
    Ok(Value::Object(moments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_create_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-create-{label}-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn dir_entry_names(dir: &Path) -> Result<Vec<String>> {
        let mut names = fs::read_dir(dir)
            .into_diagnostic()?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .into_diagnostic()?;
        names.sort();
        Ok(names)
    }

    fn unreachable_runtime(root: &Path) -> Runtime {
        Runtime {
            http: HttpClient::new(),
            config_path: root.join("missing-config.json"),
            legacy_config_paths: Vec::new(),
            api_base: "http://127.0.0.1:1".to_string(),
            mcp_base: "http://127.0.0.1:1".to_string(),
            timeout: Duration::from_millis(250),
            retries: 0,
            refresh_lock: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn no_moments_options() -> SnapshotCreateOptions {
        SnapshotCreateOptions {
            full_contacts: false,
            full_contact_ids: Vec::new(),
            full_limit: None,
            full_concurrency: 4,
            moments: false,
            moments_start: None,
            moments_end: None,
            moments_contact_ids: Vec::new(),
            moments_limit: 75,
        }
    }

    #[test]
    fn commit_snapshot_dir_renames_temp_into_place() -> Result<()> {
        let root = temp_create_root("commit");
        fs::create_dir_all(&root).into_diagnostic()?;
        let dir = root.join("snap");

        check_snapshot_create_target(&dir, false)?;
        let temp_dir = create_snapshot_temp_dir(&dir)?;
        fs::write(temp_dir.join("manifest.json"), b"{}").into_diagnostic()?;
        commit_snapshot_dir(&temp_dir, &dir)?;

        assert!(dir.join("manifest.json").is_file());
        assert!(!temp_dir.exists());
        assert_eq!(dir_entry_names(&root)?, vec!["snap".to_string()]);
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn commit_snapshot_dir_replaces_existing_empty_dir_with_force() -> Result<()> {
        let root = temp_create_root("commit-force");
        let dir = root.join("snap");
        fs::create_dir_all(&dir).into_diagnostic()?;

        check_snapshot_create_target(&dir, true)?;
        let temp_dir = create_snapshot_temp_dir(&dir)?;
        fs::write(temp_dir.join("manifest.json"), b"{}").into_diagnostic()?;
        commit_snapshot_dir(&temp_dir, &dir)?;

        assert!(dir.join("manifest.json").is_file());
        assert!(!temp_dir.exists());
        assert_eq!(dir_entry_names(&root)?, vec!["snap".to_string()]);
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[test]
    fn check_snapshot_create_target_keeps_prepare_snapshot_dir_semantics() -> Result<()> {
        let root = temp_create_root("check-target");
        let dir = root.join("snap");
        fs::create_dir_all(&dir).into_diagnostic()?;
        fs::write(dir.join("existing.txt"), b"data").into_diagnostic()?;

        let error = check_snapshot_create_target(&dir, false)
            .expect_err("existing dir without --force should be refused");
        assert!(error.to_string().contains("already exists"));

        let error = check_snapshot_create_target(&dir, true)
            .expect_err("existing non-empty dir should be refused even with --force");
        assert!(error.to_string().contains("exists and is not empty"));

        let nested = root.join("a").join("b").join("snap");
        check_snapshot_create_target(&nested, false)?;
        assert!(nested.parent().is_some_and(Path::is_dir));
        assert!(!nested.exists());

        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    #[tokio::test]
    async fn create_snapshot_failure_leaves_no_final_dir_and_cleans_temp() -> Result<()> {
        let root = temp_create_root("atomic-failure");
        fs::create_dir_all(&root).into_diagnostic()?;
        let dir = root.join("snap");
        let runtime = unreachable_runtime(&root);

        let result = create_snapshot(&runtime, &dir, false, no_moments_options()).await;

        assert!(result.is_err());
        assert!(!dir.exists());
        assert_eq!(dir_entry_names(&root)?, Vec::<String>::new());
        fs::remove_dir_all(&root).ok();
        Ok(())
    }

    fn options(moments_contact_ids: Vec<u64>) -> SnapshotCreateOptions {
        SnapshotCreateOptions {
            full_contacts: false,
            full_contact_ids: Vec::new(),
            full_limit: None,
            full_concurrency: 4,
            moments: true,
            moments_start: Some("2024-03-01".to_string()),
            moments_end: Some("2024-03-31".to_string()),
            moments_contact_ids,
            moments_limit: 75,
        }
    }

    #[test]
    fn snapshot_moment_base_payload_includes_contact_ids_only_when_present() {
        assert_eq!(
            Value::Object(snapshot_moment_base_payload(&options(vec![42, 7]))),
            json!({"contact_ids": [42, 7]})
        );
        assert_eq!(
            Value::Object(snapshot_moment_base_payload(&options(Vec::new()))),
            json!({})
        );
    }

    #[test]
    fn snapshot_moment_payloads_add_window_or_paging_fields() -> Result<()> {
        let options = options(vec![42]);

        assert_eq!(
            Value::Object(snapshot_moment_date_payload(&options)?),
            json!({
                "contact_ids": [42],
                "start": "2024-03-01",
                "end": "2024-03-31",
            })
        );
        assert_eq!(
            Value::Object(snapshot_moment_paged_payload(&options, 3)),
            json!({
                "contact_ids": [42],
                "limit": 75,
                "page": 3,
            })
        );
        Ok(())
    }

    #[test]
    fn snapshot_group_rows_from_response_rejects_bare_object() {
        let error = snapshot_group_rows_from_response(&json!({"error": "wrong shape"}))
            .expect_err("group snapshots should require an array-shaped response");

        assert!(
            error
                .to_string()
                .contains("groups response missing array rows")
        );
    }

    #[test]
    fn snapshot_group_rows_from_response_accepts_arrays_and_rejects_non_objects() -> Result<()> {
        assert_eq!(
            snapshot_group_rows_from_response(&json!({"groups": [{"id": 1}]}))?,
            vec![json!({"id": 1})]
        );
        assert_eq!(
            snapshot_group_rows_from_response(&json!([{"id": 2}]))?,
            vec![json!({"id": 2})]
        );

        let error = snapshot_group_rows_from_response(&json!({"groups": [1]}))
            .expect_err("group rows should be objects");

        assert!(error.to_string().contains("row 1 is not an object"));
        Ok(())
    }

    #[test]
    fn snapshot_moment_rows_from_response_rejects_bare_object() {
        let route = snapshot_moment_route_by_label("notes").unwrap();
        let error = snapshot_moment_rows_from_response(route, &json!({"error": "wrong shape"}))
            .expect_err("moment snapshots should require an array-shaped response");

        assert!(
            error
                .to_string()
                .contains("/moments/notes response missing array rows")
        );
    }

    #[test]
    fn snapshot_moment_rows_from_response_accepts_arrays_and_rejects_non_objects() -> Result<()> {
        let route = snapshot_moment_route_by_label("emails_recent").unwrap();
        assert_eq!(
            snapshot_moment_rows_from_response(route, &json!({"items": [{"id": 1}]}))?,
            vec![json!({"id": 1})]
        );
        assert_eq!(
            snapshot_moment_rows_from_response(route, &json!([{"id": 2}]))?,
            vec![json!({"id": 2})]
        );

        let error = snapshot_moment_rows_from_response(route, &json!({"items": [1]}))
            .expect_err("moment rows should be objects");

        assert!(error.to_string().contains("row 1 is not an object"));
        Ok(())
    }

    #[test]
    fn snapshot_moment_plan_uses_real_routes_and_payload_shapes() {
        let plan = snapshot_moment_plan(&options(vec![42]));

        assert_eq!(plan.len(), SNAPSHOT_MOMENT_ROUTES.len());
        assert_eq!(
            plan.first()
                .and_then(|row| row.get("payload"))
                .cloned()
                .unwrap_or(Value::Null),
            json!({
                "contact_ids": [42],
                "start": "2024-03-01",
                "end": "2024-03-31",
            })
        );
        assert!(plan.iter().any(|row| {
            row.get("payload").is_some_and(|payload| {
                payload == &json!({"contact_ids": [42], "limit": 75, "page": "1..has_next"})
            })
        }));
    }
}
