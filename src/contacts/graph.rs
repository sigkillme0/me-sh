use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct ContactMapOptions {
    pub(crate) source: ContactMapSource,
    pub(crate) facets: Vec<ContactFacetKind>,
    pub(crate) min_shared: usize,
    pub(crate) top_buckets: usize,
    pub(crate) sample_limit: usize,
    pub(crate) edge_limit: usize,
    pub(crate) include_empty: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ContactMapSource {
    Input {
        path: PathBuf,
        input_format: InputFormat,
    },
    Snapshot {
        dir: PathBuf,
    },
    Live {
        payload: Map<String, Value>,
        page_size: usize,
        analyze_limit: Option<usize>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ContactMapBucket {
    pub(crate) facet: ContactFacetKind,
    pub(crate) value: String,
    pub(crate) contact_ids: BTreeSet<String>,
    pub(crate) sample_contacts: Vec<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactMapContact {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactReconnectOptions {
    pub(crate) source: ContactReconnectSource,
    pub(crate) activity_sections: Vec<&'static SnapshotMomentRoute>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) activity_limit: usize,
    pub(crate) include_activity: bool,
    pub(crate) top: usize,
    pub(crate) low_activity_threshold: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum ContactReconnectSource {
    Input {
        path: PathBuf,
        input_format: InputFormat,
    },
    Snapshot {
        dir: PathBuf,
    },
    Live {
        payload: Map<String, Value>,
        page_size: usize,
        analyze_limit: Option<usize>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct ContactReconnectRow {
    pub(crate) score: i64,
    pub(crate) activity_count: Option<usize>,
    pub(crate) latest_date: Option<String>,
    pub(crate) contact_key: String,
    pub(crate) value: Value,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactSegmentsOptions {
    pub(crate) input: PathBuf,
    pub(crate) input_format: InputFormat,
    pub(crate) include_fields: Vec<String>,
    pub(crate) page_size: usize,
    pub(crate) sample_limit: usize,
    pub(crate) min_overlap: usize,
    pub(crate) min_jaccard: f64,
    pub(crate) top_overlaps: Option<usize>,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactSetsOptions {
    pub(crate) input: PathBuf,
    pub(crate) input_format: InputFormat,
    pub(crate) include_fields: Vec<String>,
    pub(crate) page_size: usize,
    pub(crate) sample_limit: usize,
    pub(crate) segments: Vec<String>,
    pub(crate) mode: ContactSetMode,
    pub(crate) id_limit: Option<usize>,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactSegmentDefinition {
    pub(crate) row: usize,
    pub(crate) name: String,
    pub(crate) payload: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactSegmentRun {
    pub(crate) name: String,
    pub(crate) payload: Map<String, Value>,
    pub(crate) matched_count: usize,
    pub(crate) analyzed_count: usize,
    pub(crate) ids: BTreeSet<u64>,
    pub(crate) contact_summaries: BTreeMap<u64, Value>,
    pub(crate) sample_contacts: Vec<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContactSetMode {
    Union,
    Intersection,
    FirstOnly,
    SymmetricDiff,
}

impl ContactMapOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let input = matches.get_one::<String>("input").map(PathBuf::from);
        let snapshot_dir = matches.get_one::<String>("snapshot-dir").map(PathBuf::from);
        if input.is_some() && snapshot_dir.is_some() {
            return err("contacts:map accepts either --input or --snapshot-dir, not both");
        }
        let facets = contact_map_facets_from_matches(matches)?;
        let min_shared = optional_positive_usize_from_matches(matches, "min-shared")?.unwrap_or(2);
        let top_buckets = optional_positive_usize_from_matches(matches, "top-buckets")?
            .unwrap_or(CONTACT_MAP_TOP_BUCKETS_DEFAULT);
        let sample_limit = optional_nonnegative_usize_from_matches(matches, "sample-limit")?
            .unwrap_or(CONTACT_MAP_SAMPLE_LIMIT_DEFAULT);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        let edge_limit = optional_nonnegative_usize_from_matches(matches, "edge-limit")?
            .unwrap_or(CONTACT_MAP_EDGE_LIMIT_DEFAULT);
        let source = if let Some(path) = input {
            ContactMapSource::Input {
                path,
                input_format: InputFormat::parse(
                    matches
                        .get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?,
            }
        } else if let Some(dir) = snapshot_dir {
            ContactMapSource::Snapshot { dir }
        } else {
            let spec = search_command_spec();
            let mut payload = parse_payload(&spec, matches)?;
            contact_map_merge_live_include_fields(&mut payload, &facets)?;
            ContactMapSource::Live {
                payload,
                page_size: optional_usize_from_matches(matches, "page-size")?
                    .unwrap_or(SEARCH_LIMIT_MAX),
                analyze_limit: optional_positive_usize_from_matches(matches, "analyze-limit")?,
            }
        };
        Ok(Self {
            source,
            facets,
            min_shared,
            top_buckets,
            sample_limit,
            edge_limit,
            include_empty: matches.get_flag("include-empty"),
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactReconnectOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let input = matches.get_one::<String>("input").map(PathBuf::from);
        let snapshot_dir = matches.get_one::<String>("snapshot-dir").map(PathBuf::from);
        if input.is_some() && snapshot_dir.is_some() {
            return err("contacts:reconnect accepts either --input or --snapshot-dir, not both");
        }
        let include_activity = !matches.get_flag("skip-activity");
        let activity_sections = if include_activity {
            moment_sections_from_matches(
                matches,
                "activity-sections",
                PROFILE_ACTIVITY_DEFAULT_SECTIONS,
            )?
        } else {
            Vec::new()
        };
        let start = matches.get_one::<String>("start").cloned();
        let end = matches.get_one::<String>("end").cloned();
        if include_activity
            && activity_sections
                .iter()
                .any(|route| matches!(route.kind, SnapshotMomentKind::DateWindow))
            && (start.is_none() || end.is_none())
        {
            return err(
                "contacts:reconnect activity sections notes, events, and emails require --start and --end",
            );
        }
        let activity_limit = optional_positive_usize_from_matches(matches, "activity-limit")?
            .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT);
        if activity_limit > SEARCH_LIMIT_MAX {
            return err(format!(
                "--activity-limit must be at most {SEARCH_LIMIT_MAX}"
            ));
        }
        let top = optional_positive_usize_from_matches(matches, "top")?
            .unwrap_or(CONTACT_RECONNECT_TOP_DEFAULT);
        if top > SEARCH_LIMIT_MAX {
            return err(format!("--top must be at most {SEARCH_LIMIT_MAX}"));
        }
        let low_activity_threshold =
            optional_nonnegative_usize_from_matches(matches, "low-activity-threshold")?
                .unwrap_or(CONTACT_RECONNECT_LOW_ACTIVITY_DEFAULT);
        if low_activity_threshold > SEARCH_LIMIT_MAX {
            return err(format!(
                "--low-activity-threshold must be at most {SEARCH_LIMIT_MAX}"
            ));
        }
        let source = if let Some(path) = input {
            ContactReconnectSource::Input {
                path,
                input_format: InputFormat::parse(
                    matches
                        .get_one::<String>("input-format")
                        .map(String::as_str)
                        .unwrap_or("auto"),
                )?,
            }
        } else if let Some(dir) = snapshot_dir {
            ContactReconnectSource::Snapshot { dir }
        } else {
            let spec = search_command_spec();
            let mut payload = parse_payload(&spec, matches)?;
            contact_reconnect_merge_live_include_fields(&mut payload);
            ContactReconnectSource::Live {
                payload,
                page_size: optional_usize_from_matches(matches, "page-size")?
                    .unwrap_or(SEARCH_LIMIT_MAX),
                analyze_limit: optional_positive_usize_from_matches(matches, "analyze-limit")?,
            }
        };
        Ok(Self {
            source,
            activity_sections,
            start,
            end,
            activity_limit,
            include_activity,
            top,
            low_activity_threshold,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactSegmentsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let input = PathBuf::from(
            matches
                .get_one::<String>("input")
                .expect("required by clap"),
        );
        let page_size =
            optional_usize_from_matches(matches, "page-size")?.unwrap_or(SEARCH_LIMIT_MAX);
        if page_size == 0 || page_size > SEARCH_LIMIT_MAX {
            return err(format!(
                "--page-size must be between 1 and {SEARCH_LIMIT_MAX}"
            ));
        }
        let sample_limit =
            optional_nonnegative_usize_from_matches(matches, "sample-limit")?.unwrap_or(5);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        if matches.get_flag("all-overlaps") && matches.get_one::<String>("top-overlaps").is_some() {
            return err("contacts:segments accepts --all-overlaps or --top-overlaps, not both");
        }
        Ok(Self {
            input,
            input_format: InputFormat::parse(
                matches
                    .get_one::<String>("input-format")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?,
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            page_size,
            sample_limit,
            min_overlap: optional_nonnegative_usize_from_matches(matches, "min-overlap")?
                .unwrap_or(1),
            min_jaccard: optional_ratio_from_matches(matches, "min-jaccard")?.unwrap_or(0.0),
            top_overlaps: if matches.get_flag("all-overlaps") {
                None
            } else {
                Some(optional_positive_usize_from_matches(matches, "top-overlaps")?.unwrap_or(20))
            },
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactSetsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let input = PathBuf::from(
            matches
                .get_one::<String>("input")
                .expect("required by clap"),
        );
        let page_size =
            optional_usize_from_matches(matches, "page-size")?.unwrap_or(SEARCH_LIMIT_MAX);
        if page_size == 0 || page_size > SEARCH_LIMIT_MAX {
            return err(format!(
                "--page-size must be between 1 and {SEARCH_LIMIT_MAX}"
            ));
        }
        let sample_limit =
            optional_nonnegative_usize_from_matches(matches, "sample-limit")?.unwrap_or(5);
        if sample_limit > SEARCH_LIMIT_MAX {
            return err(format!("--sample-limit must be at most {SEARCH_LIMIT_MAX}"));
        }
        if matches.get_flag("all-ids") && matches.get_one::<String>("id-limit").is_some() {
            return err("contacts:sets accepts --all-ids or --id-limit, not both");
        }
        let id_limit = if matches.get_flag("all-ids") {
            None
        } else {
            let value = optional_nonnegative_usize_from_matches(matches, "id-limit")?.unwrap_or(50);
            if value > SEARCH_LIMIT_MAX {
                return err(format!("--id-limit must be at most {SEARCH_LIMIT_MAX}"));
            }
            Some(value)
        };
        Ok(Self {
            input,
            input_format: InputFormat::parse(
                matches
                    .get_one::<String>("input-format")
                    .map(String::as_str)
                    .unwrap_or("auto"),
            )?,
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            page_size,
            sample_limit,
            segments: split_list_values(&collect_values(matches, "segments")),
            mode: ContactSetMode::parse(
                matches
                    .get_one::<String>("mode")
                    .map(String::as_str)
                    .unwrap_or("union"),
            )?,
            id_limit,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactSetMode {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "union" => Ok(Self::Union),
            "intersection" | "intersect" => Ok(Self::Intersection),
            "first-only" | "first" | "difference" | "diff" => Ok(Self::FirstOnly),
            "symmetric-diff" | "symmetric-difference" | "xor" => Ok(Self::SymmetricDiff),
            _ => err("--mode must be one of union, intersection, first-only, or symmetric-diff"),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Union => "union",
            Self::Intersection => "intersection",
            Self::FirstOnly => "first-only",
            Self::SymmetricDiff => "symmetric-diff",
        }
    }
}

pub(crate) async fn contacts_map(runtime: &Runtime, options: ContactMapOptions) -> Result<Value> {
    let (source, contacts) = contacts_for_map_source(runtime, &options).await?;
    Ok(contact_map_report(source, contacts, &options))
}

pub(crate) async fn contacts_for_map_source(
    runtime: &Runtime,
    options: &ContactMapOptions,
) -> Result<(Value, Vec<Value>)> {
    match &options.source {
        ContactMapSource::Input { path, input_format } => {
            contacts_for_input(path, *input_format, "contacts:map")
        }
        ContactMapSource::Snapshot { dir } => contacts_for_dedupe_snapshot(dir),
        ContactMapSource::Live {
            payload,
            page_size,
            analyze_limit,
        } => {
            let mut contacts = Vec::new();
            let mut filters = payload.clone();
            filters.remove("limit");
            let (exported, total) = export_contacts_each_limited(
                runtime,
                payload.clone(),
                *page_size,
                *analyze_limit,
                |row| {
                    contacts.push(row);
                    Ok(())
                },
            )
            .await?;
            Ok((
                json!({
                    "type": "live",
                    "matched_count": total,
                    "analyzed_count": exported,
                    "filters": Value::Object(filters),
                    "page_size": page_size,
                    "analyze_limit": analyze_limit,
                    "pagination": "exclude_contact_ids",
                }),
                contacts,
            ))
        }
    }
}

pub(crate) fn contact_map_dry_run_plan(options: &ContactMapOptions) -> Value {
    let load = match &options.source {
        ContactMapSource::Input { path, .. } => json!([
            {"local_file": path.display().to_string(), "purpose": "read contact rows from local input"}
        ]),
        ContactMapSource::Snapshot { dir } => json!([
            {"local_file": format!("{}/manifest.json", dir.display()), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"}
        ]),
        ContactMapSource::Live {
            payload,
            page_size,
            analyze_limit,
        } => json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(Some(payload)), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(Some(payload)), "page_size": page_size, "analyze_limit": analyze_limit, "purpose": "fetch matching contact rows with exclude_contact_ids pagination"}
        ]),
    };
    json!({
        "source": contact_map_source_kind(&options.source),
        "options": contact_map_options_value(options),
        "plan": {
            "load": load,
            "local": [
                {"name": "bucket", "purpose": "extract selected facet values from every contact"},
                {"name": "map", "purpose": "keep shared facet buckets, emit bucket nodes and bounded contact-to-bucket edges"}
            ]
        }
    })
}

pub(crate) fn contact_map_report(
    source: Value,
    contacts: Vec<Value>,
    options: &ContactMapOptions,
) -> Value {
    let mut contacts_by_id = BTreeMap::<String, ContactMapContact>::new();
    let mut buckets = BTreeMap::<(ContactFacetKind, String), ContactMapBucket>::new();
    let mut missing_id_count = 0_usize;
    let mut value_assignment_count = 0_usize;

    for (index, contact) in contacts.iter().enumerate() {
        let (contact_id, missing_id) = contact_map_contact_id(contact, index);
        if missing_id {
            missing_id_count = missing_id_count.saturating_add(1);
        }
        contacts_by_id
            .entry(contact_id.clone())
            .or_insert_with(|| ContactMapContact {
                id: contact_id.clone(),
                name: contact_name(contact).unwrap_or_default(),
            });
        for facet in &options.facets {
            let mut values = contact_facet_values(contact, *facet);
            if values.is_empty() && options.include_empty {
                values.push("(empty)".to_string());
            }
            for value in values {
                value_assignment_count = value_assignment_count.saturating_add(1);
                let key = (*facet, contact_facet_bucket_key(&value));
                let bucket = buckets.entry(key).or_insert_with(|| ContactMapBucket {
                    facet: *facet,
                    value: value.clone(),
                    contact_ids: BTreeSet::new(),
                    sample_contacts: Vec::new(),
                });
                if bucket.contact_ids.insert(contact_id.clone())
                    && bucket.sample_contacts.len() < options.sample_limit
                {
                    bucket
                        .sample_contacts
                        .push(contact_map_contact_sample(contact, &contact_id));
                }
            }
        }
    }

    let mut bucket_rows = buckets
        .into_values()
        .filter(|bucket| bucket.contact_ids.len() >= options.min_shared)
        .collect::<Vec<_>>();
    bucket_rows.sort_by(|left, right| {
        right
            .contact_ids
            .len()
            .cmp(&left.contact_ids.len())
            .then_with(|| left.facet.cmp(&right.facet))
            .then_with(|| {
                contact_facet_bucket_key(&left.value).cmp(&contact_facet_bucket_key(&right.value))
            })
    });
    let matched_bucket_count = bucket_rows.len();
    bucket_rows.truncate(options.top_buckets);

    let possible_edge_count = bucket_rows
        .iter()
        .map(|bucket| bucket.contact_ids.len())
        .sum::<usize>();
    let mut edges = Vec::new();
    let mut connected_contact_ids = BTreeSet::new();
    'edges: for bucket in &bucket_rows {
        let bucket_id = contact_map_bucket_node_id(bucket.facet, &bucket.value);
        for contact_id in &bucket.contact_ids {
            if edges.len() == options.edge_limit {
                break 'edges;
            }
            connected_contact_ids.insert(contact_id.clone());
            edges.push(json!({
                "id": format!("edge:{}:{}", contact_id, bucket_id),
                "source": format!("contact:{contact_id}"),
                "target": bucket_id,
                "kind": "shares",
                "facet": bucket.facet.as_str(),
                "value": bucket.value,
                "weight": 1,
            }));
        }
    }

    let contact_nodes = connected_contact_ids
        .iter()
        .filter_map(|id| contacts_by_id.get(id))
        .map(|contact| {
            json!({
                "id": format!("contact:{}", contact.id),
                "kind": "contact",
                "contact_id": contact.id,
                "label": if contact.name.trim().is_empty() { contact.id.clone() } else { contact.name.clone() },
            })
        })
        .collect::<Vec<_>>();
    let bucket_nodes = bucket_rows
        .iter()
        .map(|bucket| {
            json!({
                "id": contact_map_bucket_node_id(bucket.facet, &bucket.value),
                "kind": "bucket",
                "facet": bucket.facet.as_str(),
                "value": bucket.value,
                "label": format!("{}: {}", bucket.facet.as_str(), bucket.value),
                "contact_count": bucket.contact_ids.len(),
            })
        })
        .collect::<Vec<_>>();
    let bucket_values = bucket_rows
        .iter()
        .map(|bucket| {
            json!({
                "facet": bucket.facet.as_str(),
                "value": bucket.value,
                "contact_count": bucket.contact_ids.len(),
                "contact_ids": bucket.contact_ids.iter().take(options.sample_limit).cloned().collect::<Vec<_>>(),
                "sample_contacts": bucket.sample_contacts,
                "node_id": contact_map_bucket_node_id(bucket.facet, &bucket.value),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "source": source,
        "options": contact_map_options_value(options),
        "summary": {
            "contact_count": contacts_by_id.len(),
            "raw_row_count": contacts.len(),
            "missing_id_count": missing_id_count,
            "facet_count": options.facets.len(),
            "value_assignment_count": value_assignment_count,
            "matched_bucket_count": matched_bucket_count,
            "returned_bucket_count": bucket_values.len(),
            "possible_edge_count": possible_edge_count,
            "edge_count": edges.len(),
            "edge_truncated": possible_edge_count > edges.len(),
            "connected_contact_count": connected_contact_ids.len(),
            "isolated_contact_count": contacts_by_id.len().saturating_sub(connected_contact_ids.len()),
            "ok": missing_id_count == 0,
        },
        "nodes": {
            "contacts": contact_nodes,
            "buckets": bucket_nodes,
        },
        "edges": edges,
        "buckets": bucket_values,
    })
}

pub(crate) fn contact_map_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    let summary = report.get("summary").unwrap_or(&Value::Null);
    rows.push(json!({
        "row_type": "summary",
        "facet": Value::Null,
        "value": Value::Null,
        "contact_count": summary.get("contact_count").cloned().unwrap_or(Value::Null),
        "edge_count": summary.get("edge_count").cloned().unwrap_or(Value::Null),
        "matched_bucket_count": summary.get("matched_bucket_count").cloned().unwrap_or(Value::Null),
        "returned_bucket_count": summary.get("returned_bucket_count").cloned().unwrap_or(Value::Null),
        "source": Value::Null,
        "target": Value::Null,
        "sample_contact_ids": Value::Null,
        "sample_contact_names": Value::Null,
        "edge_truncated": summary.get("edge_truncated").cloned().unwrap_or(Value::Null),
    }));
    if let Some(buckets) = report.get("buckets").and_then(Value::as_array) {
        for bucket in buckets {
            rows.push(json!({
                "row_type": "bucket",
                "facet": bucket.get("facet").cloned().unwrap_or(Value::Null),
                "value": bucket.get("value").cloned().unwrap_or(Value::Null),
                "contact_count": bucket.get("contact_count").cloned().unwrap_or(Value::Null),
                "edge_count": Value::Null,
                "matched_bucket_count": Value::Null,
                "returned_bucket_count": Value::Null,
                "source": Value::Null,
                "target": bucket.get("node_id").cloned().unwrap_or(Value::Null),
                "sample_contact_ids": bucket.get("sample_contacts").map(contact_map_sample_cell_ids).unwrap_or_default(),
                "sample_contact_names": bucket.get("sample_contacts").map(contact_map_sample_cell_names).unwrap_or_default(),
                "edge_truncated": Value::Null,
            }));
        }
    }
    if let Some(edges) = report.get("edges").and_then(Value::as_array) {
        for edge in edges {
            rows.push(json!({
                "row_type": "edge",
                "facet": edge.get("facet").cloned().unwrap_or(Value::Null),
                "value": edge.get("value").cloned().unwrap_or(Value::Null),
                "contact_count": Value::Null,
                "edge_count": Value::Null,
                "matched_bucket_count": Value::Null,
                "returned_bucket_count": Value::Null,
                "source": edge.get("source").cloned().unwrap_or(Value::Null),
                "target": edge.get("target").cloned().unwrap_or(Value::Null),
                "sample_contact_ids": Value::Null,
                "sample_contact_names": Value::Null,
                "edge_truncated": Value::Null,
            }));
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_map_options_value(options: &ContactMapOptions) -> Value {
    json!({
        "source": contact_map_source_kind(&options.source),
        "facets": options.facets.iter().map(|facet| facet.as_str()).collect::<Vec<_>>(),
        "min_shared": options.min_shared,
        "top_buckets": options.top_buckets,
        "sample_limit": options.sample_limit,
        "edge_limit": options.edge_limit,
        "include_empty": options.include_empty,
    })
}

pub(crate) fn contact_map_source_kind(source: &ContactMapSource) -> &'static str {
    match source {
        ContactMapSource::Input { .. } => "input",
        ContactMapSource::Snapshot { .. } => "snapshot",
        ContactMapSource::Live { .. } => "live",
    }
}

pub(crate) fn contact_map_contact_id(contact: &Value, index: usize) -> (String, bool) {
    record_id(contact)
        .map(|id| (id, false))
        .unwrap_or_else(|| (format!("row:{}", index + 1), true))
}

pub(crate) fn contact_map_contact_sample(contact: &Value, id: &str) -> Value {
    json!({
        "id": id,
        "name": contact_name(contact).unwrap_or_default(),
    })
}

pub(crate) fn contact_map_bucket_node_id(facet: ContactFacetKind, value: &str) -> String {
    format!("bucket:{}:{}", facet.as_str(), value)
}

pub(crate) fn contact_map_sample_cell_ids(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| sample.get("id").and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn contact_map_sample_cell_names(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|sample| sample.get("name").and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) async fn contacts_reconnect(
    runtime: &Runtime,
    options: ContactReconnectOptions,
) -> Result<Value> {
    let (source, contacts) = contacts_for_reconnect_source(runtime, &options).await?;
    let contact_ids = contact_reconnect_activity_contact_ids(&contacts);
    let activity = if options.include_activity && !contact_ids.is_empty() {
        Some(contact_reconnect_activity(runtime, &contact_ids, &options).await?)
    } else {
        None
    };
    Ok(contact_reconnect_report(
        source, contacts, activity, &options,
    ))
}

pub(crate) async fn contacts_for_reconnect_source(
    runtime: &Runtime,
    options: &ContactReconnectOptions,
) -> Result<(Value, Vec<Value>)> {
    match &options.source {
        ContactReconnectSource::Input { path, input_format } => {
            contacts_for_input(path, *input_format, "contacts:reconnect")
        }
        ContactReconnectSource::Snapshot { dir } => contacts_for_dedupe_snapshot(dir),
        ContactReconnectSource::Live {
            payload,
            page_size,
            analyze_limit,
        } => {
            let mut contacts = Vec::new();
            let mut filters = payload.clone();
            filters.remove("limit");
            let (exported, total) = export_contacts_each_limited(
                runtime,
                payload.clone(),
                *page_size,
                *analyze_limit,
                |row| {
                    contacts.push(row);
                    Ok(())
                },
            )
            .await?;
            Ok((
                json!({
                    "type": "live",
                    "matched_count": total,
                    "analyzed_count": exported,
                    "filters": Value::Object(filters),
                    "page_size": page_size,
                    "analyze_limit": analyze_limit,
                    "pagination": "exclude_contact_ids",
                }),
                contacts,
            ))
        }
    }
}

pub(crate) fn contact_reconnect_dry_run_plan(options: &ContactReconnectOptions) -> Value {
    let load = match &options.source {
        ContactReconnectSource::Input { path, .. } => json!([
            {"local_file": path.display().to_string(), "purpose": "read contact rows from local input"}
        ]),
        ContactReconnectSource::Snapshot { dir } => json!([
            {"local_file": format!("{}/manifest.json", dir.display()), "purpose": "verify snapshot hashes"},
            {"local_file": "full-contacts.jsonl or contacts.jsonl", "purpose": "read snapshot contacts"}
        ]),
        ContactReconnectSource::Live {
            payload,
            page_size,
            analyze_limit,
        } => json!([
            {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(Some(payload)), "purpose": "count matching contacts"},
            {"route": "/tools/v2/search", "payload": live_search_page_dry_run_payload(Some(payload)), "page_size": page_size, "analyze_limit": analyze_limit, "purpose": "fetch contacts to rank with exclude_contact_ids pagination"}
        ]),
    };
    let activity = if options.include_activity {
        json!({
            "enabled": true,
            "chunk_size": CONTACT_RECONNECT_ACTIVITY_CHUNK_SIZE,
            "routes": options.activity_sections.iter().map(|route| {
                let payload = match route.kind {
                    SnapshotMomentKind::DateWindow => json!({
                        "contact_ids": "selected numeric contact IDs",
                        "start": options.start.clone().unwrap_or_default(),
                        "end": options.end.clone().unwrap_or_default(),
                    }),
                    SnapshotMomentKind::Paged => json!({
                        "contact_ids": "selected numeric contact IDs",
                        "limit": options.activity_limit,
                        "page": "1..has_next",
                    }),
                };
                json!({
                    "label": route.label,
                    "route": format!("/tools/v2{}", route.route),
                    "kind": contact_activity_kind(route.kind),
                    "payload": payload,
                    "purpose": "fetch read-only activity for ranked contacts",
                })
            }).collect::<Vec<_>>(),
        })
    } else {
        json!({
            "enabled": false,
            "purpose": "rank only visible contact profile signals",
        })
    };

    json!({
        "source": contact_reconnect_source_kind(&options.source),
        "options": contact_reconnect_options_value(options),
        "plan": {
            "load": load,
            "activity": activity,
            "local": [
                {"name": "score", "purpose": "score missing activity, low activity, missing channels, missing IDs, and scheduled reminders"},
                {"name": "rank", "purpose": "return the top ranked contacts with reasons and suggested actions"}
            ]
        }
    })
}

pub(crate) fn contact_reconnect_report(
    source: Value,
    contacts: Vec<Value>,
    activity: Option<Value>,
    options: &ContactReconnectOptions,
) -> Value {
    let feed = activity
        .as_ref()
        .and_then(|activity| activity.get("feed"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut activity_by_id = BTreeMap::<u64, Vec<Value>>::new();
    let mut unknown_activity_count = 0_usize;
    for row in &feed {
        if let Some(contact_id) = row.get("contact_id").and_then(Value::as_u64) {
            activity_by_id
                .entry(contact_id)
                .or_default()
                .push(row.clone());
        } else {
            unknown_activity_count += 1;
        }
    }

    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    let mut missing_id_count = 0_usize;
    for (index, contact) in contacts.iter().enumerate() {
        let contact_id = contact_id_from_value(contact);
        if contact_id.is_none() {
            missing_id_count += 1;
        }
        let contact_key = contact_id
            .map(|id| format!("id:{id}"))
            .unwrap_or_else(|| format!("row:{}", index + 1));
        if !seen.insert(contact_key.clone()) {
            continue;
        }
        let contact_activity = contact_id
            .and_then(|id| activity_by_id.get(&id).cloned())
            .unwrap_or_default();
        rows.push(contact_reconnect_row(
            contact,
            contact_id,
            contact_key,
            contact_activity,
            options,
        ));
    }
    rows.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| {
                left.activity_count
                    .unwrap_or(usize::MAX)
                    .cmp(&right.activity_count.unwrap_or(usize::MAX))
            })
            .then_with(|| {
                left.latest_date
                    .as_deref()
                    .unwrap_or_default()
                    .cmp(right.latest_date.as_deref().unwrap_or_default())
            })
            .then_with(|| left.contact_key.cmp(&right.contact_key))
    });

    let contact_count = rows.len();
    let no_activity_count = rows
        .iter()
        .filter(|row| contact_reconnect_has_reason(&row.value, "no_activity_in_selected_sections"))
        .count();
    let low_activity_count = rows
        .iter()
        .filter(|row| contact_reconnect_has_reason(&row.value, "low_activity_in_selected_sections"))
        .count();
    let no_channel_count = rows
        .iter()
        .filter(|row| contact_reconnect_has_reason(&row.value, "no_visible_contact_channel"))
        .count();
    let scheduled_count = rows
        .iter()
        .filter(|row| contact_reconnect_has_reason(&row.value, "upcoming_reminder"))
        .count();
    let contact_with_activity_count = rows
        .iter()
        .filter(|row| row.activity_count.unwrap_or_default() > 0)
        .count();
    rows.truncate(options.top);
    let ranked_contacts = rows.into_iter().map(|row| row.value).collect::<Vec<_>>();

    json!({
        "source": source,
        "options": contact_reconnect_options_value(options),
        "activity": activity,
        "summary": {
            "raw_row_count": contacts.len(),
            "contact_count": contact_count,
            "missing_id_count": missing_id_count,
            "activity_checked": options.include_activity,
            "activity_feed_count": feed.len(),
            "unknown_activity_count": unknown_activity_count,
            "contact_with_activity_count": contact_with_activity_count,
            "no_activity_count": no_activity_count,
            "low_activity_count": low_activity_count,
            "no_channel_count": no_channel_count,
            "scheduled_count": scheduled_count,
            "returned_contact_count": ranked_contacts.len(),
            "ok": missing_id_count == 0 && unknown_activity_count == 0,
        },
        "contacts": ranked_contacts,
    })
}

pub(crate) fn contact_reconnect_row(
    contact: &Value,
    contact_id: Option<u64>,
    contact_key: String,
    mut activity: Vec<Value>,
    options: &ContactReconnectOptions,
) -> ContactReconnectRow {
    moments_feed_sort_rows(&mut activity, MomentsFeedSort::Desc);
    let activity_count = if options.include_activity && contact_id.is_some() {
        Some(activity.len())
    } else {
        None
    };
    let latest = activity.first();
    let latest_date = latest.and_then(|row| moments_feed_row_string(row, "date"));
    let section_counts = contact_reconnect_section_counts(&activity);
    let has_upcoming_reminder = section_counts
        .get("reminders_upcoming")
        .copied()
        .unwrap_or_default()
        > 0;
    let channels = contact_channel_facet_values(contact);
    let channel_count = channels.len();
    let email_domains = contact_quality_email_domains(contact)
        .into_iter()
        .collect::<Vec<_>>();
    let companies = contact_company_facet_values(contact)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let name = contact_name(contact).unwrap_or_default();

    let mut score = 0_i64;
    let mut reasons = Vec::<String>::new();
    let mut suggested_actions = Vec::<String>::new();
    if contact_id.is_none() {
        score += 20;
        reasons.push("missing_id".to_string());
        suggested_actions.push("fix or re-export the contact ID before automation".to_string());
    }
    if !options.include_activity {
        reasons.push("activity_not_checked".to_string());
    } else if contact_id.is_none() {
        reasons.push("activity_unavailable_missing_id".to_string());
    } else if activity_count == Some(0) {
        score += 100;
        reasons.push("no_activity_in_selected_sections".to_string());
        suggested_actions.push("send a check-in or create a reminder".to_string());
    } else if activity_count
        .is_some_and(|count| count > 0 && count <= options.low_activity_threshold)
    {
        score += 60;
        reasons.push("low_activity_in_selected_sections".to_string());
        suggested_actions.push("review the latest touchpoint and follow up if useful".to_string());
    }
    if channels.is_empty() {
        score += 25;
        reasons.push("no_visible_contact_channel".to_string());
        suggested_actions.push("add an email, phone, or LinkedIn before outreach".to_string());
    }
    if name.trim().is_empty() {
        score += 10;
        reasons.push("missing_name".to_string());
        suggested_actions.push("clean up the contact name".to_string());
    }
    if options.include_activity && activity_count.unwrap_or_default() > 0 && latest_date.is_none() {
        score += 10;
        reasons.push("no_dated_activity".to_string());
    }
    if has_upcoming_reminder {
        score += 15;
        reasons.push("upcoming_reminder".to_string());
        suggested_actions.push("review the upcoming reminder".to_string());
    }
    if suggested_actions.is_empty() {
        suggested_actions.push("no immediate action suggested".to_string());
    }

    let status = if contact_id.is_none() && options.include_activity {
        "needs-id"
    } else if contact_reconnect_reason_in(&reasons, "no_activity_in_selected_sections") {
        "needs-touch"
    } else if contact_reconnect_reason_in(&reasons, "low_activity_in_selected_sections") {
        "light-touch"
    } else if contact_reconnect_reason_in(&reasons, "no_visible_contact_channel") {
        "needs-channel"
    } else if has_upcoming_reminder {
        "scheduled"
    } else if !options.include_activity {
        "profile-only"
    } else {
        "warm"
    };

    let row_key = contact_key.clone();
    let value = json!({
        "contact_key": contact_key,
        "contact_id": contact_id.map(|id| Value::Number(Number::from(id))).unwrap_or(Value::Null),
        "contact_name": if name.trim().is_empty() { Value::Null } else { Value::String(name) },
        "score": score,
        "status": status,
        "reasons": reasons,
        "suggested_actions": suggested_actions,
        "activity_checked": options.include_activity && contact_id.is_some(),
        "activity_count": activity_count.map(|count| Value::Number(Number::from(count as u64))).unwrap_or(Value::Null),
        "latest_activity_date": latest_date.clone().map(Value::String).unwrap_or(Value::Null),
        "latest_activity_section": latest.and_then(|row| moments_feed_row_string(row, "section")).map(Value::String).unwrap_or(Value::Null),
        "latest_activity_summary": latest.and_then(|row| moments_feed_row_string(row, "summary")).map(Value::String).unwrap_or(Value::Null),
        "activity_sections": section_counts,
        "channels": channels,
        "channel_count": channel_count,
        "email_domains": email_domains,
        "companies": companies,
    });

    ContactReconnectRow {
        score,
        activity_count,
        latest_date,
        contact_key: row_key,
        value,
    }
}

pub(crate) fn contact_reconnect_flat_rows(report: &Value) -> Value {
    let contacts = report
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        contacts
            .into_iter()
            .map(|contact| {
                json!({
                    "contact_key": contact.get("contact_key").cloned().unwrap_or(Value::Null),
                    "contact_id": contact.get("contact_id").cloned().unwrap_or(Value::Null),
                    "contact_name": contact.get("contact_name").cloned().unwrap_or(Value::Null),
                    "score": contact.get("score").cloned().unwrap_or(Value::Null),
                    "status": contact.get("status").cloned().unwrap_or(Value::Null),
                    "reasons": contact_reconnect_list_cell(contact.get("reasons")),
                    "suggested_actions": contact_reconnect_list_cell(contact.get("suggested_actions")),
                    "activity_checked": contact.get("activity_checked").cloned().unwrap_or(Value::Null),
                    "activity_count": contact.get("activity_count").cloned().unwrap_or(Value::Null),
                    "latest_activity_date": contact.get("latest_activity_date").cloned().unwrap_or(Value::Null),
                    "latest_activity_section": contact.get("latest_activity_section").cloned().unwrap_or(Value::Null),
                    "latest_activity_summary": contact.get("latest_activity_summary").cloned().unwrap_or(Value::Null),
                    "activity_sections": contact_reconnect_section_cell(contact.get("activity_sections")),
                    "channels": contact_reconnect_list_cell(contact.get("channels")),
                    "channel_count": contact.get("channel_count").cloned().unwrap_or(Value::Null),
                    "email_domains": contact_reconnect_list_cell(contact.get("email_domains")),
                    "companies": contact_reconnect_list_cell(contact.get("companies")),
                })
            })
            .collect(),
    )
}

pub(crate) fn contact_reconnect_options_value(options: &ContactReconnectOptions) -> Value {
    json!({
        "source": contact_reconnect_source_kind(&options.source),
        "activity_sections": contact_activity_section_labels(&options.activity_sections),
        "start": options.start.clone(),
        "end": options.end.clone(),
        "activity_limit": options.activity_limit,
        "include_activity": options.include_activity,
        "top": options.top,
        "low_activity_threshold": options.low_activity_threshold,
    })
}

pub(crate) fn contact_reconnect_source_kind(source: &ContactReconnectSource) -> &'static str {
    match source {
        ContactReconnectSource::Input { .. } => "input",
        ContactReconnectSource::Snapshot { .. } => "snapshot",
        ContactReconnectSource::Live { .. } => "live",
    }
}

pub(crate) fn contact_reconnect_section_counts(activity: &[Value]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for row in activity {
        if let Some(section) = moments_feed_row_string(row, "section") {
            *counts.entry(section).or_default() += 1;
        }
    }
    counts
}

pub(crate) fn contact_reconnect_has_reason(contact: &Value, reason: &str) -> bool {
    contact
        .get("reasons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|value| value.as_str().is_some_and(|value| value == reason))
}

pub(crate) fn contact_reconnect_reason_in(reasons: &[String], reason: &str) -> bool {
    reasons.iter().any(|value| value == reason)
}

pub(crate) fn contact_reconnect_list_cell(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(value_string)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn contact_reconnect_section_cell(value: Option<&Value>) -> String {
    let Some(object) = value.and_then(Value::as_object) else {
        return String::new();
    };
    object
        .iter()
        .map(|(section, count)| format!("{section}={}", cell_string(count)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn contact_segment_definitions(
    options: &ContactSegmentsOptions,
) -> Result<Vec<ContactSegmentDefinition>> {
    contact_segment_definitions_from_input(
        &options.input,
        options.input_format,
        "contacts:segments",
    )
}

pub(crate) fn contact_segment_definitions_from_input(
    input: &Path,
    input_format: InputFormat,
    label: &'static str,
) -> Result<Vec<ContactSegmentDefinition>> {
    let text = fs::read_to_string(input)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", input.display()))?;
    let input_format = input_format.resolve(input, &text);
    let rows = read_apply_rows(&text, input_format, label)?;
    let rows = expand_contact_segment_rows(rows, label)?;
    if rows.is_empty() {
        return err(format!("{label} input did not contain any segment rows"));
    }

    let mut names = BTreeSet::new();
    let mut definitions = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        let row_number = index + 1;
        let name = contact_segment_name(row_number, row, label)?;
        if !names.insert(contact_segment_name_key(&name)) {
            return err(format!(
                "{label} row {row_number} repeats segment name {name}"
            ));
        }
        let mut payload = contact_segment_payload(row_number, row, label)?;
        payload = normalize_contact_segment_payload(payload, label)?;
        payload.remove("limit");
        definitions.push(ContactSegmentDefinition {
            row: row_number,
            name,
            payload,
        });
    }
    Ok(definitions)
}

pub(crate) fn expand_contact_segment_rows(
    rows: Vec<Map<String, Value>>,
    label: &'static str,
) -> Result<Vec<Map<String, Value>>> {
    if rows.len() == 1 {
        let mut row = rows.into_iter().next().unwrap_or_default();
        if let Some(Value::Array(items)) = row.remove("segments") {
            return items
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    value.as_object().cloned().ok_or_else(|| {
                        miette!(
                            "{label} JSON segments row {} must be an object",
                            index.saturating_add(1)
                        )
                    })
                })
                .collect();
        }
        return Ok(vec![row]);
    }
    Ok(rows)
}

pub(crate) fn contact_segment_name(
    row_number: usize,
    row: &Map<String, Value>,
    label: &'static str,
) -> Result<String> {
    row_string(row, &["segment", "segment_name", "label", "name", "id"])
        .map(|value| single_line(&value))
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| miette!("{label} row {row_number} needs a segment name field"))
}

pub(crate) fn contact_segment_payload(
    row_number: usize,
    row: &Map<String, Value>,
    label: &'static str,
) -> Result<Map<String, Value>> {
    if let Some(value) = row_value(row, &["payload", "filters", "search"]) {
        return contact_segment_payload_value(value, row_number, label);
    }
    let payload = contact_segment_payload_from_columns(row_number, row)?;
    if payload.is_empty() {
        return err(format!(
            "{label} row {row_number} needs a payload/filters/search object or search filter columns"
        ));
    }
    Ok(payload)
}

pub(crate) fn contact_segment_payload_value(
    value: &Value,
    row_number: usize,
    label: &'static str,
) -> Result<Map<String, Value>> {
    match value {
        Value::Object(object) => Ok(object.clone()),
        Value::String(text) => parse_json_object(text, &format!("{label} payload")),
        _ => err(format!(
            "{label} row {row_number} payload must be a JSON object"
        )),
    }
}

pub(crate) fn contact_segment_payload_from_columns(
    _row_number: usize,
    row: &Map<String, Value>,
) -> Result<Map<String, Value>> {
    let spec = search_command_spec();
    let mut payload = Map::new();
    for option in spec.options {
        if option.flag == "limit" {
            continue;
        }
        let aliases = contact_segment_option_aliases(&option);
        let Some(value) = row_value_by_aliases(row, &aliases) else {
            continue;
        };
        let values = contact_segment_option_values(value);
        let Some(coerced) = coerce_option(&option, &values)? else {
            continue;
        };
        payload.insert(camel_to_snake(option.name), coerced);
    }
    if payload.is_empty()
        && let Some(query) = row_string(row, &["query", "q"])
    {
        payload.insert(
            "keywords".to_string(),
            Value::Array(vec![Value::String(query)]),
        );
    }
    if payload.is_empty() {
        return Ok(payload);
    }
    Ok(nest_payload(payload, spec.nested))
}

pub(crate) fn contact_segment_option_aliases(option: &OptionSpec) -> Vec<String> {
    if option.flag == "name" {
        return [
            "contact-name",
            "contactName",
            "contact_name",
            "search-name",
            "searchName",
            "search_name",
            "name-filter",
            "nameFilter",
            "name_filter",
        ]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    }
    let mut aliases = vec![
        option.flag.to_string(),
        option.flag.replace('-', "_"),
        option.name.to_string(),
        camel_to_snake(option.name),
    ];
    aliases.sort();
    aliases.dedup();
    aliases
}

pub(crate) fn contact_segment_option_values(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().map(cell_string).collect(),
        Value::Null => Vec::new(),
        other => vec![cell_string(other)],
    }
}

pub(crate) fn normalize_contact_segment_payload(
    payload: Map<String, Value>,
    label: &'static str,
) -> Result<Map<String, Value>> {
    let spec = search_command_spec();
    let mut normalized = Map::new();
    for (key, value) in payload {
        let normalized_key = contact_segment_payload_key(&key, &spec);
        normalized.insert(normalized_key, value);
    }
    normalize_contact_segment_include_fields(&mut normalized, label)?;
    Ok(nest_payload(normalized, spec.nested))
}

pub(crate) fn contact_segment_payload_key(key: &str, spec: &CommandSpec) -> String {
    for option in &spec.options {
        let aliases = contact_segment_option_aliases(option);
        if aliases
            .iter()
            .any(|alias| key_matches(key, &[alias.as_str()]))
            || key_matches(key, &[option.name, &camel_to_snake(option.name)])
        {
            return camel_to_snake(option.name);
        }
    }
    key.to_string()
}

pub(crate) fn normalize_contact_segment_include_fields(
    payload: &mut Map<String, Value>,
    label: &'static str,
) -> Result<()> {
    let Some(value) = payload.remove("include_fields") else {
        return Ok(());
    };
    let raw = contact_segment_option_values(&value);
    let mut seen = BTreeSet::new();
    let mut fields = Vec::new();
    let mut invalid = Vec::new();
    for value in split_list_values(&raw) {
        let normalized = normalize_include_field(&value);
        if !SEARCH_INCLUDE_FIELDS.contains(&normalized.as_str()) {
            invalid.push(value);
            continue;
        }
        if seen.insert(normalized.clone()) {
            fields.push(normalized);
        }
    }
    if !invalid.is_empty() {
        return err(format!(
            "{label} invalid include_fields value(s): {}",
            invalid.join(", ")
        ));
    }
    if !fields.is_empty() {
        payload.insert(
            "include_fields".to_string(),
            Value::Array(fields.into_iter().map(Value::String).collect()),
        );
    }
    Ok(())
}

pub(crate) fn contact_segment_effective_payload(
    payload: &Map<String, Value>,
    include_fields: &[String],
    label: &'static str,
) -> Result<Map<String, Value>> {
    let mut payload = payload.clone();
    if !include_fields.is_empty() {
        let mut existing = payload
            .get("include_fields")
            .map(contact_segment_option_values)
            .unwrap_or_default();
        existing.extend(include_fields.iter().cloned());
        payload.insert(
            "include_fields".to_string(),
            Value::Array(existing.into_iter().map(Value::String).collect()),
        );
        normalize_contact_segment_include_fields(&mut payload, label)?;
    }
    payload.remove("limit");
    Ok(payload)
}

pub(crate) fn contact_segments_dry_run_plan(
    options: &ContactSegmentsOptions,
    definitions: &[ContactSegmentDefinition],
) -> Value {
    let segments = definitions
        .iter()
        .map(|definition| {
            let payload = contact_segment_effective_payload(
                &definition.payload,
                &options.include_fields,
                "contacts:segments",
            )
            .unwrap_or_else(|_| definition.payload.clone());
            json!({
                "row": definition.row,
                "name": definition.name,
                "filters": Value::Object(payload.clone()),
                "plan": [
                    {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(Some(&payload)), "purpose": "count matching contacts"},
                    {"route": "/tools/v2/search", "payload": contact_segment_page_dry_run_payload(&payload, options.page_size), "page_size": options.page_size, "purpose": "fetch matching contact IDs without writes"}
                ],
            })
        })
        .collect::<Vec<_>>();
    json!({
        "source": {
            "type": "input",
            "path": options.input.display().to_string(),
            "input_format": options.input_format.as_str(),
        },
        "segment_count": definitions.len(),
        "page_size": options.page_size,
        "sample_limit": options.sample_limit,
        "min_overlap": options.min_overlap,
        "min_jaccard": options.min_jaccard,
        "top_overlaps": options.top_overlaps,
        "segments": segments,
    })
}

pub(crate) fn contact_segment_page_dry_run_payload(
    payload: &Map<String, Value>,
    page_size: usize,
) -> Value {
    let mut payload = payload.clone();
    payload.insert(
        "limit".to_string(),
        Value::Number(Number::from(page_size as u64)),
    );
    payload.insert(
        "exclude_contact_ids".to_string(),
        Value::String("accumulated from prior pages".to_string()),
    );
    Value::Object(payload)
}

pub(crate) async fn contacts_segments(
    runtime: &Runtime,
    options: &ContactSegmentsOptions,
    definitions: Vec<ContactSegmentDefinition>,
) -> Result<Value> {
    let mut runs = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let payload = contact_segment_effective_payload(
            &definition.payload,
            &options.include_fields,
            "contacts:segments",
        )?;
        runs.push(
            contact_segment_run(
                runtime,
                options.page_size,
                options.sample_limit,
                "contacts:segments",
                definition.name,
                payload,
            )
            .await?,
        );
    }
    Ok(contact_segments_report(options, runs))
}

pub(crate) async fn contact_segment_run(
    runtime: &Runtime,
    page_size: usize,
    sample_limit: usize,
    label: &'static str,
    name: String,
    payload: Map<String, Value>,
) -> Result<ContactSegmentRun> {
    let mut ids = BTreeSet::new();
    let mut contact_summaries = BTreeMap::new();
    let mut sample_contacts = Vec::new();
    let (analyzed_count, matched_count) =
        export_contacts_each_limited(runtime, payload.clone(), page_size, None, |row| {
            let id = contact_id_from_value(&row)
                .ok_or_else(|| miette!("{label} search row did not include numeric id"))?;
            let is_new = ids.insert(id);
            let summary = dedupe_contact_summary(&row);
            contact_summaries
                .entry(id)
                .or_insert_with(|| summary.clone());
            if is_new && sample_contacts.len() < sample_limit {
                sample_contacts.push(summary);
            }
            Ok(())
        })
        .await?;
    Ok(ContactSegmentRun {
        name,
        payload,
        matched_count,
        analyzed_count,
        ids,
        contact_summaries,
        sample_contacts,
    })
}

pub(crate) fn contact_segments_report(
    options: &ContactSegmentsOptions,
    runs: Vec<ContactSegmentRun>,
) -> Value {
    let segment_values = runs.iter().map(contact_segment_value).collect::<Vec<_>>();
    let overlaps = contact_segment_overlaps(options, &runs);
    let total_contacts = runs
        .iter()
        .flat_map(|run| run.ids.iter().copied())
        .collect::<BTreeSet<_>>()
        .len();
    json!({
        "source": {
            "type": "input",
            "path": options.input.display().to_string(),
            "input_format": options.input_format.as_str(),
        },
        "summary": {
            "segment_count": runs.len(),
            "unique_contact_count": total_contacts,
            "overlap_count": overlaps.len(),
        },
        "options": {
            "include_fields": options.include_fields,
            "page_size": options.page_size,
            "sample_limit": options.sample_limit,
            "min_overlap": options.min_overlap,
            "min_jaccard": options.min_jaccard,
            "top_overlaps": options.top_overlaps,
        },
        "segments": segment_values,
        "overlaps": overlaps,
    })
}

pub(crate) fn contact_segment_value(run: &ContactSegmentRun) -> Value {
    json!({
        "name": run.name,
        "filters": Value::Object(run.payload.clone()),
        "matched_count": run.matched_count,
        "analyzed_count": run.analyzed_count,
        "contact_count": run.ids.len(),
        "sample_contacts": run.sample_contacts,
    })
}

pub(crate) fn contact_segment_overlaps(
    options: &ContactSegmentsOptions,
    runs: &[ContactSegmentRun],
) -> Vec<Value> {
    let mut pairs = Vec::new();
    for (left_index, left) in runs.iter().enumerate() {
        for right in runs.iter().skip(left_index + 1) {
            let overlap_ids = left
                .ids
                .intersection(&right.ids)
                .copied()
                .collect::<Vec<_>>();
            let overlap_count = overlap_ids.len();
            let union_count = left.ids.union(&right.ids).count();
            let jaccard = if union_count == 0 {
                1.0
            } else {
                overlap_count as f64 / union_count as f64
            };
            if overlap_count < options.min_overlap || jaccard < options.min_jaccard {
                continue;
            }
            let relationship = contact_segment_relationship(&left.ids, &right.ids, overlap_count);
            let sample_overlap_ids = overlap_ids
                .into_iter()
                .take(options.sample_limit)
                .collect::<Vec<_>>();
            pairs.push(json!({
                "left": left.name,
                "right": right.name,
                "left_count": left.ids.len(),
                "right_count": right.ids.len(),
                "overlap_count": overlap_count,
                "union_count": union_count,
                "jaccard": jaccard,
                "relationship": relationship,
                "sample_overlap_ids": sample_overlap_ids,
            }));
        }
    }
    pairs.sort_by(|left, right| {
        let left_overlap = left
            .get("overlap_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_overlap = right
            .get("overlap_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let left_jaccard = left
            .get("jaccard")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let right_jaccard = right
            .get("jaccard")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        right_overlap
            .cmp(&left_overlap)
            .then_with(|| {
                right_jaccard
                    .partial_cmp(&left_jaccard)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                left.get("left")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("left")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .then_with(|| {
                left.get("right")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("right")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });
    if let Some(top) = options.top_overlaps {
        pairs.truncate(top);
    }
    pairs
}

pub(crate) fn contact_segment_relationship(
    left: &BTreeSet<u64>,
    right: &BTreeSet<u64>,
    overlap_count: usize,
) -> &'static str {
    if left == right {
        "same_members"
    } else if overlap_count == 0 {
        "disjoint"
    } else if left.is_subset(right) {
        "left_subset_of_right"
    } else if right.is_subset(left) {
        "right_subset_of_left"
    } else {
        "overlap"
    }
}

pub(crate) fn contact_segments_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    if let Some(segments) = report.get("segments").and_then(Value::as_array) {
        for segment in segments {
            rows.push(json!({
                "row_type": "segment",
                "segment": segment.get("name").cloned().unwrap_or(Value::Null),
                "contact_count": segment.get("contact_count").cloned().unwrap_or(Value::Null),
                "matched_count": segment.get("matched_count").cloned().unwrap_or(Value::Null),
                "analyzed_count": segment.get("analyzed_count").cloned().unwrap_or(Value::Null),
                "filters": segment.get("filters").map(cell_string).unwrap_or_default(),
                "sample_contact_ids": contact_segments_sample_cell(segment, "id"),
                "sample_contact_names": contact_segments_sample_cell(segment, "name"),
            }));
        }
    }
    if let Some(overlaps) = report.get("overlaps").and_then(Value::as_array) {
        for overlap in overlaps {
            rows.push(json!({
                "row_type": "overlap",
                "left": overlap.get("left").cloned().unwrap_or(Value::Null),
                "right": overlap.get("right").cloned().unwrap_or(Value::Null),
                "left_count": overlap.get("left_count").cloned().unwrap_or(Value::Null),
                "right_count": overlap.get("right_count").cloned().unwrap_or(Value::Null),
                "overlap_count": overlap.get("overlap_count").cloned().unwrap_or(Value::Null),
                "union_count": overlap.get("union_count").cloned().unwrap_or(Value::Null),
                "jaccard": overlap.get("jaccard").cloned().unwrap_or(Value::Null),
                "relationship": overlap.get("relationship").cloned().unwrap_or(Value::Null),
                "sample_overlap_ids": overlap.get("sample_overlap_ids").map(cell_string).unwrap_or_default(),
            }));
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_segments_sample_cell(segment: &Value, key: &str) -> String {
    segment
        .get("sample_contacts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|sample| sample.get(key).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn contact_sets_selected_definitions(
    definitions: Vec<ContactSegmentDefinition>,
    selected: &[String],
) -> Result<Vec<ContactSegmentDefinition>> {
    if selected.is_empty() {
        return Ok(definitions);
    }

    let available = definitions
        .iter()
        .map(|definition| definition.name.clone())
        .collect::<Vec<_>>();
    let mut by_name = definitions
        .into_iter()
        .map(|definition| (contact_segment_name_key(&definition.name), definition))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let mut output = Vec::with_capacity(selected.len());
    for name in selected {
        let name = single_line(name);
        let key = contact_segment_name_key(&name);
        if !seen.insert(key.clone()) {
            return err(format!("contacts:sets repeats segment selector {name}"));
        }
        let Some(definition) = by_name.remove(&key) else {
            return err(format!(
                "contacts:sets could not find segment {name}; available segments: {}",
                available.join(", ")
            ));
        };
        output.push(definition);
    }
    Ok(output)
}

pub(crate) fn contact_segment_name_key(value: &str) -> String {
    single_line(value).to_lowercase()
}

pub(crate) fn contact_sets_validate_selection(
    options: &ContactSetsOptions,
    definitions: &[ContactSegmentDefinition],
) -> Result<()> {
    if definitions.is_empty() {
        return err("contacts:sets needs at least one selected segment");
    }
    if options.mode == ContactSetMode::FirstOnly && definitions.len() < 2 {
        return err("contacts:sets --mode first-only needs at least two selected segments");
    }
    Ok(())
}

pub(crate) fn contact_sets_dry_run_plan(
    options: &ContactSetsOptions,
    definitions: &[ContactSegmentDefinition],
) -> Value {
    let segments = definitions
        .iter()
        .map(|definition| {
            let payload = contact_segment_effective_payload(
                &definition.payload,
                &options.include_fields,
                "contacts:sets",
            )
            .unwrap_or_else(|_| definition.payload.clone());
            json!({
                "row": definition.row,
                "name": definition.name,
                "filters": Value::Object(payload.clone()),
                "plan": [
                    {"route": "/tools/v2/search", "payload": live_search_count_dry_run_payload(Some(&payload)), "purpose": "count matching contacts"},
                    {"route": "/tools/v2/search", "payload": contact_segment_page_dry_run_payload(&payload, options.page_size), "page_size": options.page_size, "purpose": "fetch matching contact IDs without writes"}
                ],
            })
        })
        .collect::<Vec<_>>();
    json!({
        "source": {
            "type": "input",
            "path": options.input.display().to_string(),
            "input_format": options.input_format.as_str(),
        },
        "mode": options.mode.as_str(),
        "selected_segments": definitions.iter().map(|definition| definition.name.clone()).collect::<Vec<_>>(),
        "segment_count": definitions.len(),
        "page_size": options.page_size,
        "sample_limit": options.sample_limit,
        "id_limit": options.id_limit,
        "all_ids": options.id_limit.is_none(),
        "segments": segments,
    })
}

pub(crate) async fn contacts_sets(
    runtime: &Runtime,
    options: &ContactSetsOptions,
    definitions: Vec<ContactSegmentDefinition>,
) -> Result<Value> {
    let mut runs = Vec::with_capacity(definitions.len());
    for definition in definitions {
        let payload = contact_segment_effective_payload(
            &definition.payload,
            &options.include_fields,
            "contacts:sets",
        )?;
        runs.push(
            contact_segment_run(
                runtime,
                options.page_size,
                options.sample_limit,
                "contacts:sets",
                definition.name,
                payload,
            )
            .await?,
        );
    }
    Ok(contact_sets_report(options, runs))
}

pub(crate) fn contact_sets_report(
    options: &ContactSetsOptions,
    runs: Vec<ContactSegmentRun>,
) -> Value {
    let result_ids = contact_sets_result_ids(options.mode, &runs);
    let contact_ids = contact_sets_limited_ids(&result_ids, options.id_limit);
    let sample_contacts = result_ids
        .iter()
        .take(options.sample_limit)
        .map(|id| contact_sets_summary_for_id(*id, &runs))
        .collect::<Vec<_>>();
    let selected_segments = runs.iter().map(|run| run.name.clone()).collect::<Vec<_>>();
    let segment_values = runs.iter().map(contact_segment_value).collect::<Vec<_>>();
    json!({
        "source": {
            "type": "input",
            "path": options.input.display().to_string(),
            "input_format": options.input_format.as_str(),
        },
        "mode": options.mode.as_str(),
        "selected_segments": selected_segments,
        "segment_count": runs.len(),
        "options": {
            "include_fields": options.include_fields,
            "page_size": options.page_size,
            "sample_limit": options.sample_limit,
            "segments": options.segments,
            "id_limit": options.id_limit,
        },
        "segments": segment_values,
        "result": {
            "contact_count": result_ids.len(),
            "returned_id_count": contact_ids.len(),
            "truncated": options.id_limit.is_some_and(|limit| result_ids.len() > limit),
            "contact_ids": contact_ids,
            "sample_contacts": sample_contacts,
        },
    })
}

pub(crate) fn contact_sets_result_ids(
    mode: ContactSetMode,
    runs: &[ContactSegmentRun],
) -> BTreeSet<u64> {
    match mode {
        ContactSetMode::Union => runs
            .iter()
            .flat_map(|run| run.ids.iter().copied())
            .collect(),
        ContactSetMode::Intersection => {
            let Some(first) = runs.first() else {
                return BTreeSet::new();
            };
            let mut ids = first.ids.clone();
            for run in &runs[1..] {
                ids = ids.intersection(&run.ids).copied().collect();
            }
            ids
        }
        ContactSetMode::FirstOnly => {
            let Some(first) = runs.first() else {
                return BTreeSet::new();
            };
            let mut ids = first.ids.clone();
            for run in &runs[1..] {
                for id in &run.ids {
                    ids.remove(id);
                }
            }
            ids
        }
        ContactSetMode::SymmetricDiff => {
            let mut counts = BTreeMap::<u64, usize>::new();
            for run in runs {
                for id in &run.ids {
                    *counts.entry(*id).or_default() += 1;
                }
            }
            counts
                .into_iter()
                .filter_map(|(id, count)| (count == 1).then_some(id))
                .collect()
        }
    }
}

pub(crate) fn contact_sets_limited_ids(ids: &BTreeSet<u64>, limit: Option<usize>) -> Vec<u64> {
    ids.iter()
        .copied()
        .take(limit.unwrap_or(usize::MAX))
        .collect()
}

pub(crate) fn contact_sets_summary_for_id(id: u64, runs: &[ContactSegmentRun]) -> Value {
    runs.iter()
        .find_map(|run| run.contact_summaries.get(&id).cloned())
        .unwrap_or_else(|| json!({"id": id.to_string()}))
}

pub(crate) fn contact_sets_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    let mode = report.get("mode").cloned().unwrap_or(Value::Null);
    if let Some(segments) = report.get("segments").and_then(Value::as_array) {
        for segment in segments {
            rows.push(json!({
                "row_type": "segment",
                "mode": mode.clone(),
                "segment": segment.get("name").cloned().unwrap_or(Value::Null),
                "contact_count": segment.get("contact_count").cloned().unwrap_or(Value::Null),
                "matched_count": segment.get("matched_count").cloned().unwrap_or(Value::Null),
                "analyzed_count": segment.get("analyzed_count").cloned().unwrap_or(Value::Null),
                "filters": segment.get("filters").map(cell_string).unwrap_or_default(),
                "sample_contact_ids": contact_segments_sample_cell(segment, "id"),
                "sample_contact_names": contact_segments_sample_cell(segment, "name"),
            }));
        }
    }
    let Some(result) = report.get("result") else {
        return Value::Array(rows);
    };
    rows.push(json!({
        "row_type": "result",
        "mode": mode.clone(),
        "segment_count": report.get("segment_count").cloned().unwrap_or(Value::Null),
        "selected_segments": report.get("selected_segments").map(cell_string).unwrap_or_default(),
        "contact_count": result.get("contact_count").cloned().unwrap_or(Value::Null),
        "returned_id_count": result.get("returned_id_count").cloned().unwrap_or(Value::Null),
        "truncated": result.get("truncated").cloned().unwrap_or(Value::Null),
    }));

    let mut samples = BTreeMap::new();
    if let Some(sample_contacts) = result.get("sample_contacts").and_then(Value::as_array) {
        for sample in sample_contacts {
            if let Some(id) = sample.get("id").and_then(Value::as_str) {
                samples.insert(id.to_string(), sample.clone());
            }
        }
    }
    if let Some(contact_ids) = result.get("contact_ids").and_then(Value::as_array) {
        for id in contact_ids {
            let contact_id = cell_string(id);
            let sample = samples.get(&contact_id);
            rows.push(json!({
                "row_type": "result_contact",
                "mode": mode.clone(),
                "contact_id": contact_id,
                "contact_name": sample.and_then(|value| value.get("name")).and_then(Value::as_str).unwrap_or_default(),
                "contact_emails": sample.and_then(|value| value.get("emails")).map(cell_string).unwrap_or_default(),
                "contact_url": sample.and_then(|value| value.get("url")).and_then(Value::as_str).unwrap_or_default(),
            }));
        }
    }
    Value::Array(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[u64]) -> BTreeSet<u64> {
        ids.iter().copied().collect()
    }

    fn run(name: &str, ids: &[u64]) -> ContactSegmentRun {
        ContactSegmentRun {
            name: name.to_string(),
            payload: Map::new(),
            matched_count: ids.len(),
            analyzed_count: ids.len(),
            ids: set(ids),
            contact_summaries: ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        json!({
                            "id": id.to_string(),
                            "name": format!("Contact {id}"),
                        }),
                    )
                })
                .collect(),
            sample_contacts: Vec::new(),
        }
    }

    fn definition(name: &str) -> ContactSegmentDefinition {
        ContactSegmentDefinition {
            row: 1,
            name: name.to_string(),
            payload: Map::new(),
        }
    }

    fn segments_options(sample_limit: usize) -> ContactSegmentsOptions {
        ContactSegmentsOptions {
            input: PathBuf::from("segments.json"),
            input_format: InputFormat::Json,
            include_fields: Vec::new(),
            page_size: 100,
            sample_limit,
            min_overlap: 1,
            min_jaccard: 0.0,
            top_overlaps: None,
            flat: false,
        }
    }

    fn sets_options(mode: ContactSetMode, id_limit: Option<usize>) -> ContactSetsOptions {
        ContactSetsOptions {
            input: PathBuf::from("segments.json"),
            input_format: InputFormat::Json,
            include_fields: Vec::new(),
            page_size: 100,
            sample_limit: 10,
            segments: Vec::new(),
            mode,
            id_limit,
            flat: false,
        }
    }

    fn map_options() -> ContactMapOptions {
        ContactMapOptions {
            source: ContactMapSource::Input {
                path: PathBuf::from("contacts.json"),
                input_format: InputFormat::Json,
            },
            facets: vec![ContactFacetKind::Company],
            min_shared: 1,
            top_buckets: 10,
            sample_limit: 10,
            edge_limit: 10,
            include_empty: false,
            flat: false,
        }
    }

    fn reconnect_options() -> ContactReconnectOptions {
        ContactReconnectOptions {
            source: ContactReconnectSource::Input {
                path: PathBuf::from("contacts.json"),
                input_format: InputFormat::Json,
            },
            activity_sections: Vec::new(),
            start: None,
            end: None,
            activity_limit: 10,
            include_activity: false,
            top: 10,
            low_activity_threshold: 1,
            flat: false,
        }
    }

    #[test]
    fn contact_map_summary_is_not_ok_when_contact_ids_are_missing() {
        let report = contact_map_report(
            json!({"type": "input"}),
            vec![
                json!({"id": 1, "name": "Ada", "organization": "Acme"}),
                json!({"name": "No Id", "organization": "Acme"}),
            ],
            &map_options(),
        );

        assert_eq!(report.pointer("/summary/missing_id_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/summary/ok"), Some(&json!(false)));
    }

    #[test]
    fn contact_map_groups_case_variants_as_shared_bucket() {
        let report = contact_map_report(
            json!({"type": "input"}),
            vec![
                json!({"id": 1, "name": "Ada", "organization": "Acme"}),
                json!({"id": 2, "name": "Grace", "organization": "acme"}),
            ],
            &ContactMapOptions {
                min_shared: 2,
                ..map_options()
            },
        );

        assert_eq!(
            report.pointer("/summary/matched_bucket_count"),
            Some(&json!(1))
        );
        assert_eq!(report.pointer("/summary/edge_count"), Some(&json!(2)));
        assert_eq!(report.pointer("/buckets/0/contact_count"), Some(&json!(2)));
    }

    #[test]
    fn contact_reconnect_summary_is_not_ok_when_contact_ids_are_missing() {
        let report = contact_reconnect_report(
            json!({"type": "input"}),
            vec![json!({"id": 1, "name": "Ada"}), json!({"name": "No Id"})],
            None,
            &reconnect_options(),
        );

        assert_eq!(report.pointer("/summary/missing_id_count"), Some(&json!(1)));
        assert_eq!(report.pointer("/summary/ok"), Some(&json!(false)));
    }

    #[test]
    fn contact_reconnect_summary_is_not_ok_when_activity_ids_are_missing() {
        let mut options = reconnect_options();
        options.include_activity = true;
        let report = contact_reconnect_report(
            json!({"type": "input"}),
            vec![json!({"id": 1, "name": "Ada"})],
            Some(json!({
                "feed": [
                    {"type": "email", "date": "2026-01-01"}
                ]
            })),
            &options,
        );

        assert_eq!(report.pointer("/summary/missing_id_count"), Some(&json!(0)));
        assert_eq!(
            report.pointer("/summary/unknown_activity_count"),
            Some(&json!(1))
        );
        assert_eq!(report.pointer("/summary/ok"), Some(&json!(false)));
    }

    #[test]
    fn contact_segment_overlaps_reports_relationship_and_sample_ids() {
        let options = segments_options(1);
        let overlaps = contact_segment_overlaps(
            &options,
            &[run("left", &[1, 2, 3]), run("right", &[2, 3, 4])],
        );

        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].get("left"), Some(&json!("left")));
        assert_eq!(overlaps[0].get("right"), Some(&json!("right")));
        assert_eq!(overlaps[0].get("overlap_count"), Some(&json!(2)));
        assert_eq!(overlaps[0].get("sample_overlap_ids"), Some(&json!([2])));
    }

    #[test]
    fn contact_sets_selected_definitions_matches_unicode_case_variants() -> Result<()> {
        let selected =
            contact_sets_selected_definitions(vec![definition("MÜNCHEN")], &["münchen".into()])?;

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "MÜNCHEN");
        Ok(())
    }

    #[test]
    fn contact_sets_selected_definitions_rejects_unicode_case_duplicate_selectors() {
        let error = contact_sets_selected_definitions(
            vec![definition("MÜNCHEN")],
            &["MÜNCHEN".into(), "münchen".into()],
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("repeats segment selector"));
    }

    #[test]
    fn contact_sets_result_ids_computes_all_modes() {
        let runs = vec![run("a", &[1, 2, 3]), run("b", &[2, 3, 4])];

        assert_eq!(
            contact_sets_result_ids(ContactSetMode::Union, &runs),
            set(&[1, 2, 3, 4])
        );
        assert_eq!(
            contact_sets_result_ids(ContactSetMode::Intersection, &runs),
            set(&[2, 3])
        );
        assert_eq!(
            contact_sets_result_ids(ContactSetMode::FirstOnly, &runs),
            set(&[1])
        );
        assert_eq!(
            contact_sets_result_ids(ContactSetMode::SymmetricDiff, &runs),
            set(&[1, 4])
        );
    }

    #[test]
    fn contact_sets_limited_ids_returns_sorted_numbers() {
        let ids = set(&[3, 1, 2]);

        assert_eq!(contact_sets_limited_ids(&ids, Some(2)), vec![1, 2]);
        assert_eq!(contact_sets_limited_ids(&ids, None), vec![1, 2, 3]);
    }

    #[test]
    fn contact_sets_report_and_flat_rows_include_sample_contact_details() {
        let report = contact_sets_report(
            &sets_options(ContactSetMode::Union, Some(2)),
            vec![run("a", &[1, 2]), run("b", &[2, 3])],
        );
        let rows = contact_sets_flat_rows(&report);

        assert_eq!(report.pointer("/result/contact_ids"), Some(&json!([1, 2])));
        assert_eq!(report.pointer("/result/contact_count"), Some(&json!(3)));
        assert_eq!(report.pointer("/result/truncated"), Some(&json!(true)));
        assert!(rows.as_array().is_some_and(|rows| rows.iter().any(|row| {
            row.get("row_type") == Some(&json!("result_contact"))
                && row.get("contact_id") == Some(&json!("1"))
                && row.get("contact_name") == Some(&json!("Contact 1"))
        })));
    }
}
