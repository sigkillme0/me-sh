use crate::prelude::*;

/// Write `value`, then exit non-zero with `failure` when `ok` is false.
///
/// Verification and bulk-write commands must fail the process when a check
/// fails or a write row errors, so automation can gate on them — matching how
/// `pack`/`unpack` treat an internal verify and how `plan:audit --strict`
/// exits. The full report is printed first so the operator sees what failed.
/// `ok` is passed separately so flat/table output (which drops the `ok` field)
/// can still fail on the underlying report's status.
fn write_checked_value(matches: &ArgMatches, value: Value, ok: bool, failure: &str) -> Result<()> {
    write_value(matches, value)?;
    if ok { Ok(()) } else { err(failure) }
}

/// [`write_checked_value`] reading `ok` from the report's own `ok` field.
fn write_checked(matches: &ArgMatches, report: Value, failure: &str) -> Result<()> {
    let ok = report.get("ok").and_then(Value::as_bool).unwrap_or(false);
    write_checked_value(matches, report, ok, failure)
}

pub(crate) fn install_diagnostics() {
    use std::io::IsTerminal;

    // Agents drive this CLI through pipes and parse stderr. When stderr is
    // not a terminal, drop ANSI colors, OSC-8 links, and unicode art, and
    // never wrap lines: miette's default 80-column wrap breaks URLs and other
    // long tokens mid-word, corrupting machine-parsed messages. Terminal
    // rendering is unchanged.
    let stderr_is_terminal = io::stderr().is_terminal();
    let _ = miette::set_hook(Box::new(move |_| {
        let opts = miette::MietteHandlerOpts::new().context_lines(3);
        let opts = if stderr_is_terminal {
            opts.terminal_links(true).unicode(true)
        } else {
            opts.terminal_links(false)
                .color(false)
                .unicode(false)
                .wrap_lines(false)
        };
        Box::new(opts.build())
    }));

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_ansi(stderr_is_terminal)
        .try_init();
}

pub(crate) async fn run(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "login" => login(&runtime, sub.get_flag("open")).await,
        "logout" => logout(&runtime),
        "status" => status(&runtime).await,
        "whoami" => {
            let data = Value::Object(user_to_map(runtime.current_user().await?));
            write_value(&matches, data)
        }
        "doctor" => {
            let data = runtime.doctor().await?;
            write_value(&matches, data)
        }
        "routes" => write_value(&matches, routes_value()),
        "routes:doctor" => {
            let options = RouteDoctorOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, routes_doctor_dry_run_plan(&options)?);
            }
            let flat = options.flat;
            let data = routes_doctor(&runtime, &options).await?;
            if flat {
                write_value(&matches, route_doctor_flat_rows(&data))
            } else {
                write_value(&matches, data)
            }
        }
        "schema" => {
            let requested = sub.get_one::<String>("command").map(String::as_str);
            write_value(&matches, schema_value(requested)?)
        }
        "plan:audit" => {
            let options = PlanAuditOptions::from_matches(sub)?;
            let strict = options.strict;
            let report = plan_audit(options)?;
            let strict_failed = report
                .get("strict_failed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson => write_value(&matches, report)?,
                OutputFormat::Jsonl
                | OutputFormat::Csv
                | OutputFormat::Tsv
                | OutputFormat::Table => write_value(&matches, plan_audit_flat_rows(&report))?,
            }
            if strict && strict_failed {
                return err("plan:audit strict mode found warnings");
            }
            Ok(())
        }
        "raw" => {
            let route = sub.get_one::<String>("route").expect("required by clap");
            let body = sub.get_one::<String>("body").expect("defaulted by clap");
            let payload = parse_json_object(body, "--body")?;
            if sub.get_flag("dry-run") {
                let route = tool_route_url_path(route)?;
                return write_value(
                    &matches,
                    json!({
                        "route": route,
                        "payload": payload,
                    }),
                );
            }
            let data = runtime.call_tool(route, Value::Object(payload)).await?;
            write_value(&matches, data)
        }
        "completions" => {
            let shell = sub
                .get_one::<Shell>("shell")
                .copied()
                .expect("required by clap");
            let mut command = build_cli();
            generate(shell, &mut command, "mesh", &mut io::stdout());
            Ok(())
        }
        "config:path" => {
            println!("{}", runtime.config_path.display());
            Ok(())
        }
        "config:show" => {
            let config = runtime.read_config()?.unwrap_or_default();
            write_value(&matches, redact_config_value(&config))
        }
        n if n.starts_with("snapshot:") => run_snapshot(matches.clone(), runtime.clone()).await,
        n if n.starts_with("contacts:") => run_contacts(matches.clone(), runtime.clone()).await,
        n if n.starts_with("moments:") => run_moments(matches.clone(), runtime.clone()).await,
        n if n.starts_with("notes:") => run_notes(matches.clone(), runtime.clone()).await,
        n if n.starts_with("groups:") => run_groups(matches.clone(), runtime.clone()).await,
        "fish:init" => {
            println!("fish_add_path ~/.cargo/bin");
            println!("mesh completions fish | source");
            Ok(())
        }
        command_name => run_spec_command(command_name, sub, &matches, &runtime).await,
    }
}

/// Dispatch a declarative command via its `CommandSpec`. This is the fallback for
/// any command without an explicit arm — including domain-prefixed spec commands
/// (e.g. `contacts:search`) that the per-domain routers do not handle directly.
async fn run_spec_command(
    name: &str,
    sub: &ArgMatches,
    matches: &ArgMatches,
    runtime: &Runtime,
) -> Result<()> {
    let spec = command_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| miette!("unknown command {name}"))?;
    run_tool_command(runtime, matches, sub, &spec).await
}

async fn run_snapshot(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "snapshot:create" => {
            let dir = snapshot_create_dir(sub);
            let options = SnapshotCreateOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                let full_contact_selection = if options.full_contact_ids.is_empty() {
                    json!("IDs from contacts.jsonl")
                } else {
                    json!(options.full_contact_ids)
                };
                let mut plan = vec![
                    json!({"route": "/tools/v2/search", "payload": {"limit": 0}, "purpose": "count contacts"}),
                    json!({"route": "/tools/v2/search", "payload": "limit set to 1000 and exclude_contact_ids accumulated from prior pages", "page_size": SEARCH_LIMIT_MAX, "purpose": "write contacts.jsonl"}),
                    json!({"route": "/tools/v2/get-groups", "payload": {}, "purpose": "write groups.json"}),
                ];
                if options.full_contacts {
                    plan.push(json!({
                        "route": "/tools/v2/get-contact",
                        "selection": full_contact_selection,
                        "limit": options.full_limit,
                        "concurrency": options.full_concurrency,
                        "purpose": "write full-contacts.jsonl",
                    }));
                }
                if options.moments {
                    plan.extend(snapshot_moment_plan(&options));
                }
                plan.push(
                    json!({"local_file": "routes.json", "purpose": "write observed route map"}),
                );
                plan.push(json!({"local_file": "manifest.json", "purpose": "write counts and SHA-256 file hashes"}));
                return write_value(
                    &matches,
                    json!({
                        "dir": dir.display().to_string(),
                        "plan": plan
                    }),
                );
            }
            let data = create_snapshot(&runtime, &dir, sub.get_flag("force"), options).await?;
            write_value(&matches, data)
        }
        "snapshot:verify" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "read file list and expected hashes"},
                            {"local_files": "manifest files[*].path", "purpose": "read bytes and compare SHA-256 plus size"}
                        ]
                    }),
                );
            }
            write_checked(
                &matches,
                verify_snapshot(Path::new(dir))?,
                "snapshot:verify failed: manifest hashes did not match",
            )
        }
        "snapshot:verify-archive" => {
            let archive = sub.get_one::<String>("archive").expect("required by clap");
            let options = SnapshotVerifyArchiveOptions::from_matches(sub);
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "archive": archive,
                        "require_index": options.require_index,
                        "plan": [
                            {"archive": archive, "purpose": "stream zstd decompression and tar entries"},
                            {"security": "paths", "purpose": "reject absolute, parent-relative, duplicate, link, device, fifo, and unsupported entries"},
                            {"local_file": "manifest.json inside archive", "purpose": "compare manifest-listed bytes and SHA-256 against streamed archive entries"},
                            {"local_dir": ".meshx-index inside archive", "purpose": "validate included index sidecars and optionally require them for JSONL sections"}
                        ]
                    }),
                );
            }
            write_checked(
                &matches,
                snapshot_verify_archive(Path::new(archive), options)?,
                "snapshot:verify-archive failed: archive did not pass verification",
            )
        }
        "snapshot:catalog" => {
            let options = SnapshotCatalogOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "root": options.root.display().to_string(),
                        "recursive": options.recursive,
                        "max_depth": options.max_depth,
                        "limit": options.limit,
                        "include_snapshots": options.include_snapshots,
                        "include_archives": options.include_archives,
                        "verify": options.verify,
                        "doctor": options.doctor,
                        "require_index": options.require_index,
                        "plan": [
                            {"local": "filesystem", "purpose": "find snapshot directories with manifest.json and .tar.zst archives"},
                            {"local_file": "manifest.json", "purpose": "read cheap snapshot summaries"},
                            {"local": "verification", "purpose": "when --verify is set, verify snapshot manifests and archive contents"},
                            {"local": "doctor", "purpose": "when --doctor is set, run snapshot:doctor for directories and archive verification for archives"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_catalog(options)?)
        }
        "snapshot:prune" => {
            let options = SnapshotPruneOptions::from_matches(sub)?;
            write_value(&matches, snapshot_prune(options)?)
        }
        "snapshot:history" => {
            let options = SnapshotHistoryOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                let ids = options
                    .ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                return write_value(
                    &matches,
                    json!({
                        "root": options.root.display().to_string(),
                        "recursive": options.recursive,
                        "max_depth": options.max_depth,
                        "limit": options.limit,
                        "section": options.section.label,
                        "ids": ids,
                        "verify": options.verify,
                        "index": options.index.as_str(),
                        "details": options.details,
                        "detail_limit": options.detail_limit,
                        "records": options.records,
                        "plan": [
                            {"local": "filesystem", "purpose": "find snapshot directories with manifest.json"},
                            {"local_file": "manifest.json", "purpose": "sort snapshots by created_at_unix_ms and verify hashes unless --skip-verify is set"},
                            {"local_file": options.section.file_name, "purpose": "query requested IDs from each snapshot, using a valid index when available"},
                            {"local": "history", "purpose": "emit present, missing, added, removed, unchanged, changed, and error observations per ID"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_history(options)?)
        }
        "snapshot:find" => {
            let options = SnapshotFindOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                let ids = options
                    .ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                let sections = options
                    .sections
                    .iter()
                    .map(|section| section.label)
                    .collect::<Vec<_>>();
                return write_value(
                    &matches,
                    json!({
                        "root": options.root.display().to_string(),
                        "recursive": options.recursive,
                        "max_depth": options.max_depth,
                        "snapshot_limit": options.snapshot_limit,
                        "sections": sections,
                        "ids": ids,
                        "contains": options.contains,
                        "limit": options.limit,
                        "verify": options.verify,
                        "index": options.index.as_str(),
                        "records": options.records,
                        "plan": [
                            {"local": "filesystem", "purpose": "find snapshot directories with manifest.json"},
                            {"local_file": "manifest.json", "purpose": "verify hashes once per snapshot unless --skip-verify is set and decide which sections exist"},
                            {"local_files": "selected snapshot sections", "purpose": "scan records using JSONL indexes for ID filters when available"},
                            {"local": "filters", "purpose": "match top-level IDs and/or case-insensitive JSON text, then emit snapshot/section match rows"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_find(options)?)
        }
        "snapshot:timeline" => {
            let options = SnapshotTimelineOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "root": options.root.display().to_string(),
                        "recursive": options.recursive,
                        "max_depth": options.max_depth,
                        "limit": options.limit,
                        "changes_only": options.changes_only,
                        "diffs": options.diffs,
                        "details": options.diff.details,
                        "detail_limit": options.diff.detail_limit,
                        "plan": [
                            {"local": "filesystem", "purpose": "find snapshot directories with manifest.json"},
                            {"local_file": "manifest.json", "purpose": "sort snapshots by created_at_unix_ms, falling back to filesystem mtime"},
                            {"local": "snapshot:diff", "purpose": "compare each adjacent snapshot pair in sorted order"},
                            {"local": "timeline", "purpose": "emit pair-level added, removed, changed, moment-change, and error summaries"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_timeline(options)?)
        }
        "snapshot:drift" => {
            let options = SnapshotDriftOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": options.dir.display().to_string(),
                        "verify": options.verify,
                        "page_size": options.page_size,
                        "compare_groups": options.compare_groups,
                        "full_contact_ids": options.full_contact_ids,
                        "full_concurrency": options.full_concurrency,
                        "details": options.diff.details,
                        "detail_limit": options.diff.detail_limit,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify snapshot hashes unless --skip-verify is set"},
                            {"local_file": "contacts.jsonl", "purpose": "compare snapshot search rows with live /tools/v2/search rows"},
                            {"route": "/tools/v2/search", "payload": {"limit": "0, then page_size with exclude_contact_ids"}, "purpose": "page through live contacts without writes"},
                            {"route": "/tools/v2/get-groups", "enabled": options.compare_groups, "purpose": "compare snapshot groups.json with live groups"},
                            {"route": "/tools/v2/get-contact", "enabled": !options.full_contact_ids.is_empty(), "concurrency": options.full_concurrency, "purpose": "compare selected full-contact records when snapshot has full-contacts.jsonl"},
                            {"details": options.diff.details, "purpose": "when enabled, report bounded JSON-pointer field changes"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_drift(&runtime, options).await?)
        }
        "snapshot:report" => {
            let options = SnapshotReportOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": options.dir.display().to_string(),
                        "root": options.root.display().to_string(),
                        "recursive": options.recursive,
                        "max_depth": options.max_depth,
                        "limit": options.limit,
                        "neighbors": options.neighbors,
                        "diffs": options.diffs,
                        "details": options.diff.details,
                        "detail_limit": options.diff.detail_limit,
                        "verify": options.stats.verify,
                        "require_index": options.doctor.require_index,
                        "top": options.stats.top,
                        "drift": options.include_drift,
                        "drift_page_size": options.drift.page_size,
                        "drift_compare_groups": options.drift.compare_groups,
                        "drift_full_contact_ids": options.drift.full_contact_ids,
                        "drift_full_concurrency": options.drift.full_concurrency,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify snapshot hashes unless --skip-verify is set"},
                            {"local": "snapshot:stats", "purpose": "summarize row counts, IDs, field coverage, top keys, and email domains"},
                            {"local": "snapshot:doctor", "purpose": "check manifest counts, required IDs, and optional JSONL index health"},
                            {"local": "neighbor discovery", "enabled": options.neighbors > 0, "root": options.root.display().to_string(), "purpose": "find sibling snapshots and diff nearest previous/next snapshots"},
                            {"local": "snapshot:diff", "enabled": options.neighbors > 0, "diffs": options.diffs, "details": options.diff.details, "purpose": "summarize changes around the selected snapshot"},
                            {"live": "snapshot:drift", "enabled": options.include_drift, "purpose": "compare this snapshot with current live me.sh data without writes"}
                        ]
                    }),
                );
            }
            let report = snapshot_report(&runtime, options).await?;
            if output_format_from_matches(&matches)? == OutputFormat::Table {
                write_value(&matches, snapshot_report_table_rows(&report))
            } else {
                write_value(&matches, report)
            }
        }
        "snapshot:diff" => {
            let old = sub.get_one::<String>("old").expect("required by clap");
            let new = sub.get_one::<String>("new").expect("required by clap");
            let options = SnapshotDiffOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                    "old": old,
                    "new": new,
                    "details": options.details,
                    "detail_limit": options.detail_limit,
                    "plan": [
                            {"snapshot": "old", "purpose": "verify manifest and file hashes"},
                            {"snapshot": "new", "purpose": "verify manifest and file hashes"},
                            {"files": ["contacts.jsonl", "groups.json"], "purpose": "compare record IDs and content hashes"},
                            {"file": "full-contacts.jsonl", "purpose": "compare full contact record IDs and content hashes when both snapshots contain it"},
                            {"files": "optional moment JSONL files", "purpose": "compare manifest file fingerprints when both snapshots contain them"},
                            {"details": options.details, "purpose": "when enabled, load changed records and report bounded JSON-pointer field changes"}
                        ]
                    }),
                );
            }
            write_value(
                &matches,
                diff_snapshots(Path::new(old), Path::new(new), options)?,
            )
        }
        "snapshot:stats" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let options = SnapshotStatsOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "verify": options.verify,
                        "top": options.top,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify file hashes unless --skip-verify is set and read snapshot file inventory"},
                            {"local_files": "record sections", "purpose": "stream JSONL sections and parse JSON array sections"},
                            {"local": "stats", "purpose": "count rows, IDs, duplicate IDs, field coverage, top keys, and email domains"}
                        ]
                    }),
                );
            }
            write_value(&matches, snapshot_stats(Path::new(dir), options)?)
        }
        "snapshot:doctor" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let options = SnapshotDoctorOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "verify": options.verify,
                        "require_index": options.require_index,
                        "top": options.top,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify file hashes unless --skip-verify is set"},
                            {"local": "stats", "purpose": "count rows, top-level IDs, duplicate IDs, field coverage, top keys, and email domains"},
                            {"local": "manifest counts", "purpose": "compare manifest counts with parsed section rows"},
                            {"local": "record IDs", "purpose": "fail missing or duplicate top-level IDs for contacts, full_contacts, and groups; warn for moment rows"},
                            {"local_file": ".meshx-index/<section>.json", "purpose": "check JSONL index presence and freshness; --require-index makes missing or stale indexes fail"}
                        ]
                    }),
                );
            }
            write_checked(
                &matches,
                snapshot_doctor(Path::new(dir), options)?,
                "snapshot:doctor failed: one or more health checks failed",
            )
        }
        "snapshot:pack" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let archive = sub.get_one::<String>("archive").expect("required by clap");
            let options = SnapshotPackOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "archive": archive,
                        "verify": options.verify,
                        "include_indexes": options.include_indexes,
                        "compression": {
                            "format": "zstd",
                            "level": options.compression_level,
                        },
                        "force": options.force,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify snapshot hashes unless --skip-verify is set"},
                            {"local_files": "manifest-listed snapshot files", "purpose": "stream into a tar archive"},
                            {"local_dir": ".meshx-index", "purpose": "include valid JSONL indexes unless --no-index is set"},
                            {"archive": archive, "purpose": "write a zstd-compressed tar through a temporary file and move it into place after success"}
                        ]
                    }),
                );
            }
            write_value(
                &matches,
                snapshot_pack(Path::new(dir), Path::new(archive), options)?,
            )
        }
        "snapshot:unpack" => {
            let archive = sub.get_one::<String>("archive").expect("required by clap");
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let options = SnapshotUnpackOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "archive": archive,
                        "dir": dir,
                        "verify": options.verify,
                        "force": options.force,
                        "plan": [
                            {"archive": archive, "purpose": "stream zstd decompression and tar entries"},
                            {"security": "paths", "purpose": "reject absolute, parent-relative, link, device, fifo, and unsupported archive entries"},
                            {"dir": dir, "purpose": "write into a new destination directory, or an existing empty one with --force"},
                            {"local_file": "manifest.json", "purpose": "verify unpacked snapshot hashes unless --skip-verify is set"}
                        ]
                    }),
                );
            }
            write_value(
                &matches,
                snapshot_unpack(Path::new(archive), Path::new(dir), options)?,
            )
        }
        "snapshot:index" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let options = SnapshotIndexOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "section": options.section.label,
                        "file": options.section.file_name,
                        "force": options.force,
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "read source file fingerprint"},
                            {"local_file": options.section.file_name, "purpose": "scan JSONL records and record byte offsets for top-level IDs"},
                            {"local_dir": ".meshx-index", "purpose": "write a sidecar index bound to source bytes and SHA-256"}
                        ]
                    }),
                );
            }
            write_value(&matches, create_snapshot_index(Path::new(dir), options)?)
        }
        "snapshot:query" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let options = SnapshotQueryOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "section": options.section.label,
                        "file": options.section.file_name,
                        "verify": options.verify,
                        "ids": options.ids,
                        "contains": options.contains,
                        "limit": options.limit,
                        "index": options.index.as_str(),
                        "plan": [
                            {"local_file": "manifest.json", "purpose": "verify manifest hashes unless --skip-verify is set"},
                            {"local_file": options.section.file_name, "purpose": "read snapshot records"},
                            {"local": "filters", "purpose": "apply --ids, --contains, and --limit locally"},
                            {"local_file": ".meshx-index/<section>.json", "purpose": "when --ids is set and --index is auto or require, use valid byte offsets instead of scanning the full JSONL file"},
                            {"output": "streaming", "purpose": "stream JSONL-backed sections for json, compact-json, jsonl, csv, and tsv formats; table materializes rows"}
                        ]
                    }),
                );
            }
            write_snapshot_query(&matches, Path::new(dir), options)
        }
        "snapshot:restore" => {
            let dir = sub.get_one::<String>("dir").expect("required by clap");
            let mode = RestoreMode::from_matches(sub)?;
            let ids = optional_ids_from_matches(sub, "contact-ids")?;
            let include_notes = sub.get_flag("include-notes");
            let contacts = snapshot_restore_contacts(Path::new(dir), &ids)?;
            let actions = snapshot_restore_plan(&contacts, mode, include_notes)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "dir": dir,
                        "mode": mode.as_str(),
                        "include_notes": include_notes,
                        "selected_count": contacts.len(),
                        "actions": actions.iter().map(restore_action_value).collect::<Vec<_>>(),
                    }),
                );
            }
            if !sub.get_flag("yes") {
                return err(
                    "snapshot:restore writes contacts. Re-run with --yes, or use --dry-run.",
                );
            }
            let result = apply_snapshot_restore(&runtime, mode, include_notes, actions).await;
            write_checked(
                &matches,
                result,
                "snapshot:restore: one or more contacts failed to restore",
            )
        }
        _ => run_spec_command(name, sub, &matches, &runtime).await,
    }
}

async fn run_groups(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "groups:find" => {
            let query = sub
                .get_one::<String>("query")
                .expect("required by clap")
                .to_lowercase();
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "route": "/tools/v2/get-groups",
                        "payload": {},
                        "local_filter": {
                            "name_or_title_contains": query,
                        }
                    }),
                );
            }
            let data = runtime.call_tool(route::GET_GROUPS, json!({})).await?;
            let matched_groups = snapshot_group_rows_from_response(&data)?
                .into_iter()
                .filter(|row| {
                    ["name", "title"]
                        .into_iter()
                        .filter_map(|key| row.get(key))
                        .any(|value| cell_string(value).to_lowercase().contains(&query))
                })
                .collect::<Vec<_>>();
            write_value(&matches, Value::Array(matched_groups))
        }
        "groups:resolve" => {
            let options = GroupResolveOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_resolve_dry_run_plan(&options));
            }
            let result = groups_resolve(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson => write_value(&matches, result),
                OutputFormat::Jsonl
                | OutputFormat::Csv
                | OutputFormat::Tsv
                | OutputFormat::Table => write_value(&matches, group_resolve_rows(&result)),
            }
        }
        "groups:profile" => {
            let options = GroupProfileOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_profile_dry_run_plan(&options));
            }
            let result = groups_profile(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson => write_value(&matches, result),
                OutputFormat::Jsonl
                | OutputFormat::Csv
                | OutputFormat::Tsv
                | OutputFormat::Table => write_value(&matches, group_profile_rows(&result)),
            }
        }
        "groups:overlap" => {
            let options = GroupOverlapOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_overlap_dry_run_plan(&options));
            }
            let result = groups_overlap(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson => write_value(&matches, result),
                OutputFormat::Jsonl
                | OutputFormat::Csv
                | OutputFormat::Tsv
                | OutputFormat::Table => write_value(
                    &matches,
                    result
                        .get("pairs")
                        .cloned()
                        .unwrap_or(Value::Array(Vec::new())),
                ),
            }
        }
        "groups:compare" => {
            let options = GroupCompareOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_compare_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = groups_compare(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, group_compare_rows(&result)),
            }
        }
        "groups:audit" => {
            let options = GroupAuditOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_audit_dry_run_plan(&options));
            }
            let result = groups_audit(&runtime, options).await?;
            if output_format_from_matches(&matches)? == OutputFormat::Table {
                write_value(&matches, group_audit_table_rows(&result))
            } else {
                write_value(&matches, result)
            }
        }
        "groups:members" => {
            let options = GroupMembersOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_members_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = groups_members(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, group_members_flat_rows(&result)),
            }
        }
        "groups:sync" => {
            let options = GroupSyncOptions::from_matches(sub)?;
            let plan = groups_sync_plan(&runtime, &options).await?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, group_sync_plan_value(&plan));
            }
            if !sub.get_flag("yes") {
                return err(
                    "groups:sync writes group membership. Re-run with --yes, or use --dry-run.",
                );
            }
            let result = apply_group_sync(&runtime, &plan, options.concurrency).await?;
            write_checked(
                &matches,
                result,
                "groups:sync: one or more membership writes failed",
            )
        }
        "groups:bulk-add" | "groups:bulk-remove" => {
            let (kind, command) = if name == "groups:bulk-add" {
                (GroupApplyKind::Add, "groups:bulk-add")
            } else {
                (GroupApplyKind::Remove, "groups:bulk-remove")
            };
            let options = GroupBulkMembershipOptions::from_matches(sub, kind, command)?;
            let plan = group_bulk_membership_plan(&runtime, &options).await?;
            if sub.get_flag("dry-run") {
                let data = group_bulk_membership_plan_value(&plan);
                return if options.flat {
                    write_value(&matches, group_bulk_membership_rows(&data))
                } else {
                    write_value(&matches, data)
                };
            }
            if !sub.get_flag("yes") {
                return err(format!(
                    "{command} writes group membership. Re-run with --yes, or use --dry-run."
                ));
            }
            let flat = options.flat;
            let result = apply_group_bulk_membership(&runtime, &plan).await?;
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let output = match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => result,
                _ => group_bulk_membership_rows(&result),
            };
            write_checked_value(
                &matches,
                output,
                ok,
                "group bulk membership: one or more chunks failed",
            )
        }
        "groups:apply" => {
            let input = sub.get_one::<String>("input").expect("required by clap");
            let requested_format = InputFormat::parse(
                sub.get_one::<String>("input-format")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?;
            let default_action = GroupApplyKind::parse(
                sub.get_one::<String>("default-action")
                    .map(String::as_str)
                    .unwrap_or("update"),
            )?;
            let concurrency = contact_fetch_concurrency(sub, "concurrency")?;
            let plan = group_apply_plan_from_file(
                Path::new(input),
                requested_format,
                default_action,
                sub.get_flag("ignore-unknown"),
            )?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "input": input,
                        "input_format": plan.input_format.as_str(),
                        "default_action": default_action.as_str(),
                        "concurrency": concurrency,
                        "action_count": plan.actions.len(),
                        "actions": plan.actions.iter().map(group_apply_action_value).collect::<Vec<_>>(),
                    }),
                );
            }
            if !sub.get_flag("yes") {
                return err(
                    "groups:apply writes groups or memberships. Re-run with --yes, or use --dry-run.",
                );
            }
            let result = apply_group_actions(&runtime, plan.actions, concurrency).await?;
            write_checked(&matches, result, "groups:apply: one or more actions failed")
        }
        _ => run_spec_command(name, sub, &matches, &runtime).await,
    }
}

async fn run_contacts(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "contacts:count" => {
            let spec = search_command_spec();
            let mut payload = parse_payload(&spec, sub)?;
            payload.set("limit", 0);
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "route": "/tools/v2/search",
                        "payload": payload,
                    }),
                );
            }
            let data = runtime
                .call_tool(route::SEARCH, Value::Object(payload))
                .await?;
            write_value(
                &matches,
                json!({ "total": total_from_search(&data), "raw": data }),
            )
        }
        "contacts:export" => {
            let spec = search_command_spec();
            let payload = parse_payload(&spec, sub)?;
            let page_size =
                optional_usize_from_matches(sub, "page-size")?.unwrap_or(SEARCH_LIMIT_MAX);
            let format = output_format_from_matches(&matches)?;
            let resume = sub.get_flag("resume");
            if resume {
                validate_export_resume_args(sub, format)?;
            }
            if sub.get_flag("dry-run") {
                if sub.get_flag("all") {
                    let mut count_payload = payload.clone();
                    count_payload.set("limit", 0);
                    let plan = json!({
                            "plan": [
                                {
                                    "route": "/tools/v2/search",
                                    "payload": count_payload,
                                    "purpose": "count matching contacts"
                                },
                                {
                                    "route": "/tools/v2/search",
                                    "payload": "same filters with limit set to page_size and exclude_contact_ids accumulated from prior pages",
                                    "page_size": page_size,
                                    "purpose": "export pages until exported unique IDs matches the counted total"
                                },
                                {
                                    "local_file": sub.get_one::<String>("output"),
                                    "state_file": sub.get_one::<String>("output").map(|path| export_state_path(Path::new(path)).display().to_string()),
                                    "enabled": resume,
                                    "purpose": "when --resume is set, scan existing JSONL output for exported contact IDs before appending missing rows"
                                }
                            ]
                    });
                    if resume {
                        return write_value_stdout(&matches, plan);
                    }
                    return write_value(&matches, plan);
                }
                return write_value(
                    &matches,
                    json!({
                        "route": "/tools/v2/search",
                        "payload": payload,
                    }),
                );
            }
            if sub.get_flag("all") {
                match format {
                    OutputFormat::Jsonl => {
                        return export_all_contacts_jsonl(
                            &runtime, &matches, payload, page_size, resume,
                        )
                        .await;
                    }
                    OutputFormat::Csv => {
                        return export_all_contacts_delimited(
                            &runtime, &matches, payload, page_size, b',',
                        )
                        .await;
                    }
                    OutputFormat::Tsv => {
                        return export_all_contacts_delimited(
                            &runtime, &matches, payload, page_size, b'\t',
                        )
                        .await;
                    }
                    OutputFormat::Json => {
                        return export_all_contacts_json_array(
                            &runtime, &matches, payload, page_size, true,
                        )
                        .await;
                    }
                    OutputFormat::CompactJson => {
                        return export_all_contacts_json_array(
                            &runtime, &matches, payload, page_size, false,
                        )
                        .await;
                    }
                    OutputFormat::Table => {}
                }
                let data = export_all_contacts(&runtime, payload, page_size).await?;
                return write_value(&matches, data);
            }
            let data = runtime
                .call_tool(route::SEARCH, Value::Object(payload))
                .await?;
            write_value(&matches, unwrap_rows_for_export(data))
        }
        "contacts:resolve" => {
            let options = ContactResolveOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contacts_resolve_dry_run(&options));
            }
            let result = contacts_resolve(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson => write_value(&matches, result),
                OutputFormat::Jsonl
                | OutputFormat::Csv
                | OutputFormat::Tsv
                | OutputFormat::Table => write_value(&matches, contacts_resolve_rows(&result)),
            }
        }
        "contacts:bulk-get" => {
            let ids = contact_ids_from_matches(sub, "contact-ids")?;
            let concurrency = contact_fetch_concurrency(sub, "concurrency")?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "route": "/tools/v2/get-contact",
                        "concurrency": concurrency,
                        "requests": ids.iter().map(|id| json!({ "contact_id": id })).collect::<Vec<_>>(),
                    }),
                );
            }
            match output_format_from_matches(&matches)? {
                OutputFormat::Json => {
                    bulk_get_contacts_json_array(&runtime, &matches, &ids, concurrency, true).await
                }
                OutputFormat::CompactJson => {
                    bulk_get_contacts_json_array(&runtime, &matches, &ids, concurrency, false).await
                }
                OutputFormat::Jsonl => {
                    bulk_get_contacts_jsonl(&runtime, &matches, &ids, concurrency).await
                }
                OutputFormat::Csv => {
                    bulk_get_contacts_delimited(&runtime, &matches, &ids, concurrency, b',').await
                }
                OutputFormat::Tsv => {
                    bulk_get_contacts_delimited(&runtime, &matches, &ids, concurrency, b'\t').await
                }
                OutputFormat::Table => {
                    let contacts = fetch_contacts(&runtime, &ids, concurrency).await?;
                    write_value(&matches, Value::Array(contacts))
                }
            }
        }
        "contacts:activity" => {
            let options = ContactActivityOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_activity_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = contacts_activity(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_activity_flat_rows(&result)),
            }
        }
        "contacts:profile" => {
            let options = ContactProfileOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_profile_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = contacts_profile(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_profile_flat_rows(&result)),
            }
        }
        "contacts:groups" => {
            let options = ContactGroupsOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_groups_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = contacts_groups(&runtime, &options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_groups_flat_rows(&result)),
            }
        }
        "contacts:bulk-archive" | "contacts:bulk-restore" => {
            let (kind, command) = if name == "contacts:bulk-archive" {
                (ApplyKind::Archive, "contacts:bulk-archive")
            } else {
                (ApplyKind::Restore, "contacts:bulk-restore")
            };
            let options = ContactBulkStateOptions::from_matches(sub, kind, command)?;
            let plan = contact_bulk_state_plan(&runtime, &options).await?;
            if sub.get_flag("dry-run") {
                let data = contact_bulk_state_plan_value(&plan);
                return if options.flat {
                    write_value(&matches, contact_bulk_state_rows(&data))
                } else {
                    write_value(&matches, data)
                };
            }
            if !sub.get_flag("yes") {
                return err(format!(
                    "{command} writes contacts. Re-run with --yes, or use --dry-run."
                ));
            }
            let flat = options.flat;
            let result = apply_contact_bulk_state(&runtime, &plan).await?;
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let output = match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => result,
                _ => contact_bulk_state_rows(&result),
            };
            write_checked_value(
                &matches,
                output,
                ok,
                "bulk write: one or more chunks failed",
            )
        }
        "contacts:bulk-update" => {
            let options = ContactBulkUpdateOptions::from_matches(sub)?;
            let plan = contact_bulk_update_plan(&runtime, &options).await?;
            if sub.get_flag("dry-run") {
                let data = contact_bulk_update_plan_value(&plan);
                return if options.flat {
                    write_value(&matches, contact_bulk_update_rows(&data))
                } else {
                    write_value(&matches, data)
                };
            }
            if !sub.get_flag("yes") {
                return err(
                    "contacts:bulk-update writes contacts. Re-run with --yes, or use --dry-run.",
                );
            }
            let flat = options.flat;
            let result = apply_contact_bulk_update(&runtime, &plan).await?;
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let output = match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => result,
                _ => contact_bulk_update_rows(&result),
            };
            write_checked_value(
                &matches,
                output,
                ok,
                "contacts:bulk-update: one or more updates failed",
            )
        }
        "contacts:apply" => {
            let input = sub.get_one::<String>("input").expect("required by clap");
            let requested_format = InputFormat::parse(
                sub.get_one::<String>("input-format")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?;
            let default_action = ApplyKind::parse(
                sub.get_one::<String>("default-action")
                    .map(String::as_str)
                    .unwrap_or("create"),
            )?;
            let concurrency = contact_fetch_concurrency(sub, "concurrency")?;
            let plan = contact_apply_plan_from_file(
                Path::new(input),
                requested_format,
                default_action,
                sub.get_flag("ignore-unknown"),
            )?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "input": input,
                        "input_format": plan.input_format.as_str(),
                        "default_action": default_action.as_str(),
                        "concurrency": concurrency,
                        "action_count": plan.actions.len(),
                        "actions": plan.actions.iter().map(contact_apply_action_value).collect::<Vec<_>>(),
                    }),
                );
            }
            if !sub.get_flag("yes") {
                return err(
                    "contacts:apply writes contacts or notes. Re-run with --yes, or use --dry-run.",
                );
            }
            let result = apply_contact_actions(&runtime, plan.actions, concurrency).await?;
            write_checked(
                &matches,
                result,
                "contacts:apply: one or more actions failed",
            )
        }
        "contacts:dedupe" => {
            let input = sub.get_one::<String>("input").map(String::as_str);
            let snapshot_dir = sub.get_one::<String>("snapshot-dir").map(String::as_str);
            if input.is_some() && snapshot_dir.is_some() {
                return err("contacts:dedupe accepts either --input or --snapshot-dir, not both");
            }
            let signals = dedupe_signals_from_matches(sub)?;
            let min_confidence = min_confidence_from_matches(sub)?;
            let candidate_limit = optional_usize_from_matches(sub, "candidate-limit")?;
            let live_payload = if input.is_none() && snapshot_dir.is_none() {
                let spec = search_command_spec();
                Some(parse_payload(&spec, sub)?)
            } else {
                None
            };
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "source": dedupe_source_label(input, snapshot_dir),
                        "filters": live_payload.as_ref().map(|payload| Value::Object(payload.clone())).unwrap_or(Value::Null),
                        "signals": signals.iter().map(|signal| signal.as_str()).collect::<Vec<_>>(),
                        "min_confidence": min_confidence,
                        "candidate_limit": candidate_limit,
                        "plan": dedupe_dry_run_plan(input, snapshot_dir, live_payload.as_ref()),
                    }),
                );
            }
            let (source, contacts) = if let Some(input) = input {
                let requested_format = InputFormat::parse(
                    sub.get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?;
                contacts_for_dedupe_input(Path::new(input), requested_format)?
            } else if let Some(snapshot_dir) = snapshot_dir {
                contacts_for_dedupe_snapshot(Path::new(snapshot_dir))?
            } else {
                contacts_for_dedupe_live(&runtime, live_payload.expect("live payload parsed"))
                    .await?
            };
            let result =
                dedupe_contacts(source, contacts, &signals, min_confidence, candidate_limit);
            write_value(&matches, result)
        }
        "contacts:quality" => {
            let input = sub.get_one::<String>("input").map(String::as_str);
            let snapshot_dir = sub.get_one::<String>("snapshot-dir").map(String::as_str);
            if input.is_some() && snapshot_dir.is_some() {
                return err("contacts:quality accepts either --input or --snapshot-dir, not both");
            }
            let options = ContactQualityOptions::from_matches(sub)?;
            let live_payload = if input.is_none() && snapshot_dir.is_none() {
                let spec = search_command_spec();
                Some(parse_payload(&spec, sub)?)
            } else {
                None
            };
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "source": dedupe_source_label(input, snapshot_dir),
                        "filters": live_payload.as_ref().map(|payload| Value::Object(payload.clone())).unwrap_or(Value::Null),
                        "issue_limit": options.issue_limit,
                        "top": options.top,
                        "plan": contact_quality_dry_run_plan(input, snapshot_dir, live_payload.as_ref()),
                    }),
                );
            }
            let (source, contacts) = if let Some(input) = input {
                let requested_format = InputFormat::parse(
                    sub.get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?;
                contacts_for_dedupe_input(Path::new(input), requested_format)?
            } else if let Some(snapshot_dir) = snapshot_dir {
                contacts_for_dedupe_snapshot(Path::new(snapshot_dir))?
            } else {
                contacts_for_dedupe_live(&runtime, live_payload.expect("live payload parsed"))
                    .await?
            };
            let result = contact_quality_report(source, contacts, options)?;
            if output_format_from_matches(&matches)? == OutputFormat::Table {
                write_value(&matches, contact_quality_table_rows(&result))
            } else {
                write_value(&matches, result)
            }
        }
        "contacts:facets" => {
            let input = sub.get_one::<String>("input").map(String::as_str);
            let snapshot_dir = sub.get_one::<String>("snapshot-dir").map(String::as_str);
            if input.is_some() && snapshot_dir.is_some() {
                return err("contacts:facets accepts either --input or --snapshot-dir, not both");
            }
            let options = ContactFacetsOptions::from_matches(sub)?;
            let live_payload = if input.is_none() && snapshot_dir.is_none() {
                let spec = search_command_spec();
                Some(parse_payload(&spec, sub)?)
            } else {
                None
            };
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "source": dedupe_source_label(input, snapshot_dir),
                        "filters": live_payload.as_ref().map(|payload| Value::Object(payload.clone())).unwrap_or(Value::Null),
                        "facets": options.facets.iter().map(|facet| facet.as_str()).collect::<Vec<_>>(),
                        "top": options.top,
                        "min_count": options.min_count,
                        "sample_limit": options.sample_limit,
                        "include_empty": options.include_empty,
                        "plan": contact_facets_dry_run_plan(input, snapshot_dir, live_payload.as_ref()),
                    }),
                );
            }
            let flat = options.flat;
            let (source, contacts) = if let Some(input) = input {
                let requested_format = InputFormat::parse(
                    sub.get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?;
                contacts_for_dedupe_input(Path::new(input), requested_format)?
            } else if let Some(snapshot_dir) = snapshot_dir {
                contacts_for_dedupe_snapshot(Path::new(snapshot_dir))?
            } else {
                contacts_for_dedupe_live(&runtime, live_payload.expect("live payload parsed"))
                    .await?
            };
            let result = contact_facets_report(source, contacts, options);
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_facets_table_rows(&result)),
            }
        }
        "contacts:pivot" => {
            let input = sub.get_one::<String>("input").map(String::as_str);
            let snapshot_dir = sub.get_one::<String>("snapshot-dir").map(String::as_str);
            if input.is_some() && snapshot_dir.is_some() {
                return err("contacts:pivot accepts either --input or --snapshot-dir, not both");
            }
            let options = ContactPivotOptions::from_matches(sub)?;
            let live_payload = if input.is_none() && snapshot_dir.is_none() {
                let spec = search_command_spec();
                Some(parse_payload(&spec, sub)?)
            } else {
                None
            };
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    contact_pivot_dry_run_plan(
                        input,
                        snapshot_dir,
                        live_payload.as_ref(),
                        &options,
                    ),
                );
            }
            let flat = options.flat;
            let (source, contacts) = if let Some(input) = input {
                let requested_format = InputFormat::parse(
                    sub.get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?;
                contacts_for_input(Path::new(input), requested_format, "contacts:pivot")?
            } else if let Some(snapshot_dir) = snapshot_dir {
                contacts_for_dedupe_snapshot(Path::new(snapshot_dir))?
            } else {
                contacts_for_dedupe_live(&runtime, live_payload.expect("live payload parsed"))
                    .await?
            };
            let result = contact_pivot_report(source, contacts, options);
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_pivot_flat_rows(&result)),
            }
        }
        "contacts:overview" => {
            let input = sub.get_one::<String>("input").map(String::as_str);
            let snapshot_dir = sub.get_one::<String>("snapshot-dir").map(String::as_str);
            if input.is_some() && snapshot_dir.is_some() {
                return err("contacts:overview accepts either --input or --snapshot-dir, not both");
            }
            let options = ContactOverviewOptions::from_matches(sub)?;
            let live_payload = if input.is_none() && snapshot_dir.is_none() {
                let spec = search_command_spec();
                Some(parse_payload(&spec, sub)?)
            } else {
                None
            };
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    contact_overview_dry_run_plan(
                        input,
                        snapshot_dir,
                        live_payload.as_ref(),
                        &options,
                    ),
                );
            }
            let flat = options.flat;
            let (source, contacts) = if let Some(input) = input {
                let requested_format = InputFormat::parse(
                    sub.get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?;
                contacts_for_input(Path::new(input), requested_format, "contacts:overview")?
            } else if let Some(snapshot_dir) = snapshot_dir {
                contacts_for_dedupe_snapshot(Path::new(snapshot_dir))?
            } else {
                contacts_for_dedupe_live(&runtime, live_payload.expect("live payload parsed"))
                    .await?
            };
            let result = contact_overview_report(source, contacts, options)?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_overview_flat_rows(&result)),
            }
        }
        "contacts:map" => {
            let options = ContactMapOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_map_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = contacts_map(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_map_flat_rows(&result)),
            }
        }
        "contacts:reconnect" => {
            let options = ContactReconnectOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_reconnect_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = contacts_reconnect(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_reconnect_flat_rows(&result)),
            }
        }
        "contacts:segments" => {
            let options = ContactSegmentsOptions::from_matches(sub)?;
            let definitions = contact_segment_definitions(&options)?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    contact_segments_dry_run_plan(&options, &definitions),
                );
            }
            let flat = options.flat;
            let result = contacts_segments(&runtime, &options, definitions).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_segments_flat_rows(&result)),
            }
        }
        "contacts:sets" => {
            let options = ContactSetsOptions::from_matches(sub)?;
            let definitions = contact_set_definitions(&options)?;
            let definitions = contact_sets_selected_definitions(definitions, &options.segments)?;
            contact_sets_validate_selection(&options, &definitions)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, contact_sets_dry_run_plan(&options, &definitions));
            }
            let flat = options.flat;
            let result = contacts_sets(&runtime, &options, definitions).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, contact_sets_flat_rows(&result)),
            }
        }
        "contacts:merge-plan" => {
            let ids = contact_ids_from_matches(sub, "contact-ids")?;
            validate_merge_ids(&ids)?;
            let concurrency = contact_fetch_concurrency(sub, "concurrency")?;
            if sub.get_flag("dry-run") {
                return write_value(
                    &matches,
                    json!({
                        "route": "/tools/v2/get-contact",
                        "concurrency": concurrency,
                        "requests": ids.iter().map(|id| json!({ "contact_id": id })).collect::<Vec<_>>(),
                    }),
                );
            }
            let contacts = fetch_contacts(&runtime, &ids, concurrency).await?;
            write_value(&matches, json!({ "merge_plan": contacts }))
        }
        _ => run_spec_command(name, sub, &matches, &runtime).await,
    }
}

async fn run_moments(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "moments:feed" => {
            let options = MomentsFeedOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, moments_feed_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = moments_feed(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, moments_feed_output_rows(&result)),
            }
        }
        "moments:stats" => {
            let options = MomentsStatsOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, moments_stats_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = moments_stats(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, moments_stats_flat_rows(&result)),
            }
        }
        "moments:timeline" => {
            let options = MomentsTimelineOptions::from_matches(sub)?;
            if sub.get_flag("dry-run") {
                return write_value(&matches, moments_timeline_dry_run_plan(&options));
            }
            let flat = options.flat;
            let result = moments_timeline(&runtime, options).await?;
            match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => {
                    write_value(&matches, result)
                }
                _ => write_value(&matches, moments_timeline_flat_rows(&result)),
            }
        }
        _ => run_spec_command(name, sub, &matches, &runtime).await,
    }
}

async fn run_notes(matches: ArgMatches, runtime: Runtime) -> Result<()> {
    let (name, sub) = matches.subcommand().expect("subcommand required by clap");
    match name {
        "notes:bulk-create" => {
            let options = NotesBulkCreateOptions::from_matches(sub)?;
            let plan = notes_bulk_create_plan(&runtime, &options).await?;
            if sub.get_flag("dry-run") {
                let data = notes_bulk_create_plan_value(&plan);
                return if options.flat {
                    write_value(&matches, notes_bulk_create_rows(&data))
                } else {
                    write_value(&matches, data)
                };
            }
            if !sub.get_flag("yes") {
                return err("notes:bulk-create writes notes. Re-run with --yes, or use --dry-run.");
            }
            let flat = options.flat;
            let result = apply_notes_bulk_create(&runtime, &plan).await?;
            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let output = match output_format_from_matches(&matches)? {
                OutputFormat::Json | OutputFormat::CompactJson if !flat => result,
                _ => notes_bulk_create_rows(&result),
            };
            write_checked_value(
                &matches,
                output,
                ok,
                "notes:bulk-create: one or more notes failed",
            )
        }
        _ => run_spec_command(name, sub, &matches, &runtime).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every CLI subcommand must reach a handler: an explicit `run()` arm, a
    /// per-domain sub-dispatcher, or the `CommandSpec` fallback. This guards the
    /// command surface — a command clap accepts but `run()` cannot route would
    /// otherwise fail only at runtime (dispatch is not exercised by other tests).
    #[test]
    fn every_cli_subcommand_is_dispatchable() {
        const DOMAIN_PREFIXES: &[&str] =
            &["snapshot:", "contacts:", "groups:", "moments:", "notes:"];
        const EXPLICIT: &[&str] = &[
            "login",
            "logout",
            "status",
            "whoami",
            "doctor",
            "routes",
            "routes:doctor",
            "schema",
            "plan:audit",
            "raw",
            "completions",
            "config:path",
            "config:show",
            "fish:init",
        ];
        let specs: BTreeSet<String> = command_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect();
        for sub in build_cli().get_subcommands() {
            let name = sub.get_name();
            let dispatchable = EXPLICIT.contains(&name)
                || DOMAIN_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                || specs.contains(name);
            assert!(
                dispatchable,
                "CLI subcommand `{name}` has no dispatch handler"
            );
        }
    }

    /// Domain-prefixed spec commands (e.g. `contacts:search`) have no explicit
    /// arm in their sub-dispatcher; they rely on the `run_spec_command` fallback.
    /// If they were not registered as specs, that fallback would error at
    /// runtime — so pin that they stay in `command_specs()`.
    #[test]
    fn domain_prefixed_spec_commands_are_registered() {
        let specs: BTreeSet<String> = command_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect();
        for name in [
            "contacts:search",
            "contacts:create",
            "contacts:update",
            "contacts:archive",
            "contacts:restore",
            "contacts:merge",
            "notes:create",
            "groups:create",
            "groups:update",
        ] {
            assert!(
                specs.contains(name),
                "spec command `{name}` is no longer registered"
            );
        }
    }
}
