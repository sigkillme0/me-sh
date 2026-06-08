use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct PlanAuditOptions {
    pub(crate) input: PathBuf,
    pub(crate) input_format: InputFormat,
    pub(crate) max_writes: Option<usize>,
    pub(crate) max_contact_ids: Option<usize>,
    pub(crate) max_group_ids: Option<usize>,
    pub(crate) id_sample_limit: usize,
    pub(crate) duplicate_limit: usize,
    pub(crate) strict: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct PlanAuditAction {
    pub(crate) index: usize,
    pub(crate) path: String,
    pub(crate) route: String,
    pub(crate) route_path: String,
    pub(crate) payload: Value,
    pub(crate) planned: bool,
    pub(crate) known_route: bool,
    pub(crate) write: bool,
    pub(crate) contact_ids: BTreeSet<u64>,
    pub(crate) group_ids: BTreeSet<String>,
}

#[derive(Default)]
pub(crate) struct PlanRouteAudit {
    pub(crate) action_count: usize,
    pub(crate) planned_count: usize,
    pub(crate) write_count: usize,
    pub(crate) planned_write_count: usize,
    pub(crate) read_count: usize,
    pub(crate) unknown_count: usize,
    pub(crate) unique_contact_ids: BTreeSet<u64>,
    pub(crate) unique_group_ids: BTreeSet<String>,
    pub(crate) duplicate_payload_groups: usize,
}

impl PlanAuditOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let requested_format = InputFormat::parse(
            matches
                .get_one::<String>("input-format")
                .map(String::as_str)
                .unwrap_or("auto"),
        )?;
        Ok(Self {
            input: PathBuf::from(
                matches
                    .get_one::<String>("input")
                    .expect("required by clap"),
            ),
            input_format: requested_format,
            max_writes: optional_nonnegative_usize_from_matches(matches, "max-writes")?,
            max_contact_ids: optional_nonnegative_usize_from_matches(matches, "max-contact-ids")?,
            max_group_ids: optional_nonnegative_usize_from_matches(matches, "max-group-ids")?,
            id_sample_limit: optional_nonnegative_usize_from_matches(matches, "id-sample-limit")?
                .unwrap_or(PLAN_AUDIT_ID_SAMPLE_DEFAULT),
            duplicate_limit: optional_nonnegative_usize_from_matches(matches, "duplicate-limit")?
                .unwrap_or(PLAN_AUDIT_DUPLICATE_SAMPLE_DEFAULT),
            strict: matches.get_flag("strict"),
        })
    }
}

pub(crate) fn plan_audit(options: PlanAuditOptions) -> Result<Value> {
    let (input_format, input_value) = read_plan_audit_input(&options.input, options.input_format)?;
    let mut actions = Vec::new();
    collect_plan_audit_actions(&input_value, "$", false, &mut actions);

    let route_catalog = plan_audit_route_catalog();
    for action in &mut actions {
        action.known_route = route_catalog.contains_key(&action.route_path);
        action.write = route_catalog
            .get(&action.route_path)
            .copied()
            .unwrap_or_else(|| plan_audit_likely_write_route(&action.route_path));
    }

    let mut route_stats = BTreeMap::<String, PlanRouteAudit>::new();
    let mut payload_counts = BTreeMap::<(String, String), usize>::new();
    let mut unique_contact_ids = BTreeSet::new();
    let mut unique_group_ids = BTreeSet::new();
    let mut unique_write_contact_ids = BTreeSet::new();
    let mut unique_write_group_ids = BTreeSet::new();
    let mut write_contact_counts = BTreeMap::<u64, usize>::new();
    let mut write_group_counts = BTreeMap::<String, usize>::new();

    for action in &actions {
        unique_contact_ids.extend(action.contact_ids.iter().copied());
        unique_group_ids.extend(action.group_ids.iter().cloned());
        if action.write {
            unique_write_contact_ids.extend(action.contact_ids.iter().copied());
            unique_write_group_ids.extend(action.group_ids.iter().cloned());
            for id in &action.contact_ids {
                *write_contact_counts.entry(*id).or_default() += 1;
            }
            for id in &action.group_ids {
                *write_group_counts.entry(id.clone()).or_default() += 1;
            }
        }

        let stats = route_stats.entry(action.route.clone()).or_default();
        stats.action_count = stats.action_count.saturating_add(1);
        if action.planned {
            stats.planned_count = stats.planned_count.saturating_add(1);
        }
        if action.write && action.planned {
            stats.planned_write_count = stats.planned_write_count.saturating_add(1);
        } else if action.write {
            stats.write_count = stats.write_count.saturating_add(1);
        } else if action.known_route {
            stats.read_count = stats.read_count.saturating_add(1);
        } else {
            stats.unknown_count = stats.unknown_count.saturating_add(1);
        }
        stats
            .unique_contact_ids
            .extend(action.contact_ids.iter().copied());
        stats
            .unique_group_ids
            .extend(action.group_ids.iter().cloned());

        let payload_key = serde_json::to_string(&action.payload).into_diagnostic()?;
        *payload_counts
            .entry((action.route.clone(), payload_key))
            .or_default() += 1;
    }

    let mut duplicate_rows = Vec::new();
    let mut duplicate_payload_group_count = 0_usize;
    for ((route, payload), count) in payload_counts {
        if count <= 1 {
            continue;
        }
        duplicate_payload_group_count = duplicate_payload_group_count.saturating_add(1);
        if let Some(stats) = route_stats.get_mut(&route) {
            stats.duplicate_payload_groups = stats.duplicate_payload_groups.saturating_add(1);
        }
        if duplicate_rows.len() < options.duplicate_limit {
            duplicate_rows.push(json!({
                "route": route,
                "count": count,
                "payload": parse_maybe_json(&payload),
            }));
        }
    }

    let route_rows = route_stats
        .iter()
        .map(|(route, stats)| {
            json!({
                "route": route,
                "class": plan_audit_route_class(stats),
                "action_count": stats.action_count,
                "planned_count": stats.planned_count,
                "write_count": stats.write_count,
                "planned_write_count": stats.planned_write_count,
                "read_count": stats.read_count,
                "unknown_count": stats.unknown_count,
                "unique_contact_id_count": stats.unique_contact_ids.len(),
                "unique_contact_id_sample": plan_audit_sample_u64(&stats.unique_contact_ids, options.id_sample_limit),
                "unique_group_id_count": stats.unique_group_ids.len(),
                "unique_group_id_sample": plan_audit_sample_string(&stats.unique_group_ids, options.id_sample_limit),
                "duplicate_payload_groups": stats.duplicate_payload_groups,
            })
        })
        .collect::<Vec<_>>();

    let write_count = actions
        .iter()
        .filter(|action| action.write && !action.planned)
        .count();
    let planned_count = actions.iter().filter(|action| action.planned).count();
    let planned_write_count = actions
        .iter()
        .filter(|action| action.write && action.planned)
        .count();
    let read_count = actions
        .iter()
        .filter(|action| !action.planned && !action.write && action.known_route)
        .count();
    let unknown_count = actions
        .iter()
        .filter(|action| !action.planned && !action.write && !action.known_route)
        .count();

    let duplicate_contact_targets =
        plan_audit_duplicate_count_rows_u64(&write_contact_counts, options.id_sample_limit);
    let duplicate_group_targets =
        plan_audit_duplicate_count_rows_string(&write_group_counts, options.id_sample_limit);

    let mut warnings = Vec::new();
    if actions.is_empty() {
        warnings.push(plan_audit_warning(
            "error",
            "no_routes",
            "input did not contain any route-bearing dry-run actions",
        ));
    }
    if unknown_count > 0 {
        warnings.push(plan_audit_warning(
            "warn",
            "unknown_routes",
            format!("{unknown_count} route action(s) were not in the mesh route catalog"),
        ));
    }
    if duplicate_payload_group_count > 0 {
        warnings.push(plan_audit_warning(
            "warn",
            "duplicate_payloads",
            format!("{duplicate_payload_group_count} route/payload pair(s) appear more than once"),
        ));
    }
    if !duplicate_contact_targets.is_empty() {
        warnings.push(plan_audit_warning(
            "warn",
            "duplicate_contact_write_targets",
            format!(
                "{} contact ID(s) are targeted by multiple write actions",
                duplicate_contact_targets.len()
            ),
        ));
    }
    let limited_write_count = write_count.saturating_add(planned_write_count);
    if let Some(max) = options.max_writes
        && limited_write_count > max
    {
        warnings.push(plan_audit_warning(
            "error",
            "max_writes_exceeded",
            format!("write action count {limited_write_count} exceeds --max-writes {max}"),
        ));
    }
    if let Some(max) = options.max_contact_ids
        && unique_write_contact_ids.len() > max
    {
        warnings.push(plan_audit_warning(
            "error",
            "max_contact_ids_exceeded",
            format!(
                "unique write contact ID count {} exceeds --max-contact-ids {max}",
                unique_write_contact_ids.len()
            ),
        ));
    }
    if let Some(max) = options.max_group_ids
        && unique_write_group_ids.len() > max
    {
        warnings.push(plan_audit_warning(
            "error",
            "max_group_ids_exceeded",
            format!(
                "unique write group ID count {} exceeds --max-group-ids {max}",
                unique_write_group_ids.len()
            ),
        ));
    }

    let error_count = warnings
        .iter()
        .filter(|warning| warning.get("severity").and_then(Value::as_str) == Some("error"))
        .count();
    let warning_count = warnings.len().saturating_sub(error_count);
    let action_samples = actions
        .iter()
        .take(options.id_sample_limit)
        .map(|action| {
            json!({
                "index": action.index,
                "path": action.path,
                "route": action.route,
                "class": plan_audit_action_class(action),
                "planned": action.planned,
                "contact_ids": plan_audit_sample_u64(&action.contact_ids, options.id_sample_limit),
                "group_ids": plan_audit_sample_string(&action.group_ids, options.id_sample_limit),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "source": "local",
        "input": options.input.display().to_string(),
        "input_format": input_format.as_str(),
        "strict": options.strict,
        "strict_failed": options.strict && !warnings.is_empty(),
        "limits": {
            "max_writes": options.max_writes,
            "max_contact_ids": options.max_contact_ids,
            "max_group_ids": options.max_group_ids,
            "id_sample_limit": options.id_sample_limit,
            "duplicate_limit": options.duplicate_limit,
        },
        "summary": {
            "action_count": actions.len(),
            "route_count": route_rows.len(),
            "write_count": write_count,
            "planned_count": planned_count,
            "planned_write_count": planned_write_count,
            "read_count": read_count,
            "unknown_route_count": unknown_count,
            "unique_contact_id_count": unique_contact_ids.len(),
            "unique_write_contact_id_count": unique_write_contact_ids.len(),
            "unique_group_id_count": unique_group_ids.len(),
            "unique_write_group_id_count": unique_write_group_ids.len(),
            "duplicate_payload_group_count": duplicate_payload_group_count,
            "warning_count": warning_count,
            "error_count": error_count,
            "ok": warnings.is_empty(),
        },
        "samples": {
            "contact_ids": plan_audit_sample_u64(&unique_contact_ids, options.id_sample_limit),
            "write_contact_ids": plan_audit_sample_u64(&unique_write_contact_ids, options.id_sample_limit),
            "group_ids": plan_audit_sample_string(&unique_group_ids, options.id_sample_limit),
            "write_group_ids": plan_audit_sample_string(&unique_write_group_ids, options.id_sample_limit),
            "duplicate_contact_write_targets": duplicate_contact_targets,
            "duplicate_group_write_targets": duplicate_group_targets,
        },
        "routes": route_rows,
        "duplicates": duplicate_rows,
        "action_samples": action_samples,
        "warnings": warnings,
    }))
}

pub(crate) fn read_plan_audit_input(
    path: &Path,
    requested_format: InputFormat,
) -> Result<(InputFormat, Value)> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let input_format = requested_format.resolve(path, &text);
    let value = match input_format {
        InputFormat::Json => serde_json::from_str(&text)
            .into_diagnostic()
            .wrap_err("plan:audit JSON input must be valid JSON")?,
        InputFormat::Jsonl => {
            let mut rows = Vec::new();
            for (line_index, line) in text.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let value = serde_json::from_str(trimmed)
                    .into_diagnostic()
                    .wrap_err_with(|| {
                        format!("plan:audit JSONL line {} is invalid", line_index + 1)
                    })?;
                rows.push(value);
            }
            Value::Array(rows)
        }
        InputFormat::Csv => Value::Array(
            read_apply_delimited_rows(&text, b',', "plan:audit")?
                .into_iter()
                .map(Value::Object)
                .collect(),
        ),
        InputFormat::Tsv => Value::Array(
            read_apply_delimited_rows(&text, b'\t', "plan:audit")?
                .into_iter()
                .map(Value::Object)
                .collect(),
        ),
        InputFormat::Auto => unreachable!("auto input format must be resolved before reading"),
    };
    Ok((input_format, value))
}

pub(crate) fn collect_plan_audit_actions(
    value: &Value,
    path: &str,
    planned: bool,
    actions: &mut Vec<PlanAuditAction>,
) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_plan_audit_actions(item, &format!("{path}[{index}]"), planned, actions);
            }
        }
        Value::Object(object) => {
            if let Some((route, route_path)) = plan_audit_route_from_object(object) {
                if let Some(requests) = object.get("requests").and_then(Value::as_array) {
                    for (index, request) in requests.iter().enumerate() {
                        actions.push(plan_audit_action(
                            actions.len() + 1,
                            format!("{path}.requests[{index}]"),
                            route.clone(),
                            route_path.clone(),
                            plan_audit_payload_value(request),
                            planned,
                        ));
                    }
                } else if let Some(chunks) = object.get("chunks").and_then(Value::as_array) {
                    for (index, chunk) in chunks.iter().enumerate() {
                        let chunk_path = format!("{path}.chunks[{index}]");
                        if chunk
                            .as_object()
                            .and_then(plan_audit_route_from_object)
                            .is_some()
                        {
                            collect_plan_audit_actions(chunk, &chunk_path, planned, actions);
                        } else {
                            actions.push(plan_audit_action(
                                actions.len() + 1,
                                chunk_path,
                                route.clone(),
                                route_path.clone(),
                                plan_audit_payload_value(chunk),
                                planned,
                            ));
                        }
                    }
                } else {
                    let payload = object
                        .get("payload")
                        .map(plan_audit_payload_value)
                        .unwrap_or_else(|| Value::Object(object.clone()));
                    actions.push(plan_audit_action(
                        actions.len() + 1,
                        path.to_string(),
                        route,
                        route_path,
                        payload,
                        planned,
                    ));
                }
            }

            for (key, child) in object {
                if matches!(key.as_str(), "payload" | "requests" | "chunks") {
                    continue;
                }
                collect_plan_audit_actions(
                    child,
                    &format!("{path}.{key}"),
                    planned || key == "plan",
                    actions,
                );
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

pub(crate) fn plan_audit_action(
    index: usize,
    path: String,
    route: String,
    route_path: String,
    payload: Value,
    planned: bool,
) -> PlanAuditAction {
    let mut contact_ids = BTreeSet::new();
    let mut group_ids = BTreeSet::new();
    collect_plan_audit_payload_ids(&payload, &mut contact_ids, &mut group_ids);
    PlanAuditAction {
        index,
        path,
        route,
        route_path,
        payload,
        planned,
        known_route: false,
        write: false,
        contact_ids,
        group_ids,
    }
}

pub(crate) fn plan_audit_route_from_object(
    object: &Map<String, Value>,
) -> Option<(String, String)> {
    let raw = object
        .get("route")
        .and_then(Value::as_str)
        .or_else(|| object.get("route_path").and_then(Value::as_str))?
        .trim();
    if raw.is_empty() {
        return None;
    }
    let route = if raw == "/tools/v2" || raw.starts_with("/tools/") {
        raw.to_string()
    } else if raw.starts_with('/') {
        format!("/tools/v2{raw}")
    } else {
        format!("/tools/v2/{raw}")
    };
    let route_path = route
        .strip_prefix("/tools/v2/")
        .map(|path| format!("/{path}"))
        .unwrap_or_else(|| route.clone());
    Some((route, route_path))
}

pub(crate) fn plan_audit_payload_value(value: &Value) -> Value {
    value
        .as_str()
        .map(parse_maybe_json)
        .unwrap_or_else(|| value.clone())
}

pub(crate) fn plan_audit_route_catalog() -> BTreeMap<String, bool> {
    let mut catalog = BTreeMap::new();
    for spec in command_specs() {
        catalog
            .entry(spec.route_path.to_string())
            .and_modify(|write| *write = *write || spec.destructive)
            .or_insert(spec.destructive);
    }
    for probe in ROUTE_DOCTOR_PROBE_TEMPLATES {
        catalog.entry(probe.route.to_string()).or_insert(false);
    }
    catalog.entry("/get-contact".to_string()).or_insert(false);
    catalog
}

pub(crate) fn plan_audit_likely_write_route(route_path: &str) -> bool {
    route_path.contains("create")
        || route_path.contains("update")
        || route_path.contains("archive")
        || route_path.contains("restore")
        || route_path.contains("merge")
        || route_path.ends_with("/note")
}

pub(crate) fn plan_audit_route_class(stats: &PlanRouteAudit) -> &'static str {
    if stats.write_count > 0 {
        "write"
    } else if stats.planned_write_count > 0 {
        "planned_write"
    } else if stats.unknown_count > 0 {
        "unknown"
    } else {
        "read"
    }
}

pub(crate) fn plan_audit_action_class(action: &PlanAuditAction) -> &'static str {
    match (action.planned, action.write, action.known_route) {
        (true, true, _) => "planned_write",
        (true, false, true) => "planned_read",
        (true, false, false) => "planned_unknown",
        (false, true, _) => "write",
        (false, false, true) => "read",
        (false, false, false) => "unknown",
    }
}

pub(crate) fn collect_plan_audit_payload_ids(
    value: &Value,
    contact_ids: &mut BTreeSet<u64>,
    group_ids: &mut BTreeSet<String>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_plan_audit_payload_ids(item, contact_ids, group_ids);
            }
        }
        Value::Object(object) => {
            for (key, child) in object {
                let normalized = normalize_apply_key(key);
                match normalized.as_str() {
                    "contactid" | "contactids" | "addcontactids" | "removecontactids" => {
                        collect_plan_audit_u64_values(child, contact_ids);
                    }
                    "groupid" | "groupids" | "targetgroupids" | "targetgroupid" => {
                        collect_plan_audit_group_values(child, group_ids);
                    }
                    _ => {}
                }
                collect_plan_audit_payload_ids(child, contact_ids, group_ids);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

pub(crate) fn collect_plan_audit_u64_values(value: &Value, ids: &mut BTreeSet<u64>) {
    match value {
        Value::Number(number) => {
            if let Some(id) = number.as_u64() {
                ids.insert(id);
            }
        }
        Value::String(text) => {
            if let Ok(value) = serde_json::from_str::<Value>(text) {
                collect_plan_audit_u64_values(&value, ids);
                return;
            }
            for part in text.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
                if let Ok(id) = part.trim().parse::<u64>() {
                    ids.insert(id);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_plan_audit_u64_values(item, ids);
            }
        }
        Value::Object(_) | Value::Bool(_) | Value::Null => {}
    }
}

pub(crate) fn collect_plan_audit_group_values(value: &Value, ids: &mut BTreeSet<String>) {
    match value {
        Value::Number(number) => {
            ids.insert(number.to_string());
        }
        Value::String(text) => {
            if let Ok(value) = serde_json::from_str::<Value>(text) {
                collect_plan_audit_group_values(&value, ids);
                return;
            }
            for part in text.split(|ch: char| ch == ',' || ch.is_ascii_whitespace()) {
                let part = part.trim();
                if !part.is_empty() {
                    ids.insert(part.to_string());
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_plan_audit_group_values(item, ids);
            }
        }
        Value::Object(_) | Value::Bool(_) | Value::Null => {}
    }
}

pub(crate) fn plan_audit_sample_u64(ids: &BTreeSet<u64>, limit: usize) -> Vec<u64> {
    ids.iter().copied().take(limit).collect()
}

pub(crate) fn plan_audit_sample_string(ids: &BTreeSet<String>, limit: usize) -> Vec<String> {
    ids.iter().take(limit).cloned().collect()
}

pub(crate) fn plan_audit_duplicate_count_rows_u64(
    counts: &BTreeMap<u64, usize>,
    limit: usize,
) -> Vec<Value> {
    counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .take(limit)
        .map(|(id, count)| json!({"id": id, "write_count": count}))
        .collect()
}

pub(crate) fn plan_audit_duplicate_count_rows_string(
    counts: &BTreeMap<String, usize>,
    limit: usize,
) -> Vec<Value> {
    counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .take(limit)
        .map(|(id, count)| json!({"id": id, "write_count": count}))
        .collect()
}

pub(crate) fn plan_audit_warning(
    severity: &'static str,
    code: &'static str,
    message: impl Into<String>,
) -> Value {
    json!({
        "severity": severity,
        "code": code,
        "message": message.into(),
    })
}

pub(crate) fn plan_audit_flat_rows(report: &Value) -> Value {
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let mut rows = vec![json!({
        "row_type": "summary",
        "route": Value::Null,
        "class": Value::Null,
        "action_count": summary.get("action_count").cloned().unwrap_or(Value::Null),
        "planned_count": summary.get("planned_count").cloned().unwrap_or(Value::Null),
        "write_count": summary.get("write_count").cloned().unwrap_or(Value::Null),
        "planned_write_count": summary.get("planned_write_count").cloned().unwrap_or(Value::Null),
        "read_count": summary.get("read_count").cloned().unwrap_or(Value::Null),
        "unknown_count": summary.get("unknown_route_count").cloned().unwrap_or(Value::Null),
        "unique_contact_id_count": summary.get("unique_contact_id_count").cloned().unwrap_or(Value::Null),
        "unique_write_contact_id_count": summary.get("unique_write_contact_id_count").cloned().unwrap_or(Value::Null),
        "unique_group_id_count": summary.get("unique_group_id_count").cloned().unwrap_or(Value::Null),
        "unique_write_group_id_count": summary.get("unique_write_group_id_count").cloned().unwrap_or(Value::Null),
        "duplicate_payload_groups": summary.get("duplicate_payload_group_count").cloned().unwrap_or(Value::Null),
        "severity": Value::Null,
        "code": Value::Null,
        "message": Value::Null,
    })];

    if let Some(routes) = report.get("routes").and_then(Value::as_array) {
        for route in routes {
            rows.push(json!({
                "row_type": "route",
                "route": route.get("route").cloned().unwrap_or(Value::Null),
                "class": route.get("class").cloned().unwrap_or(Value::Null),
                "action_count": route.get("action_count").cloned().unwrap_or(Value::Null),
                "planned_count": route.get("planned_count").cloned().unwrap_or(Value::Null),
                "write_count": route.get("write_count").cloned().unwrap_or(Value::Null),
                "planned_write_count": route.get("planned_write_count").cloned().unwrap_or(Value::Null),
                "read_count": route.get("read_count").cloned().unwrap_or(Value::Null),
                "unknown_count": route.get("unknown_count").cloned().unwrap_or(Value::Null),
                "unique_contact_id_count": route.get("unique_contact_id_count").cloned().unwrap_or(Value::Null),
                "unique_write_contact_id_count": Value::Null,
                "unique_group_id_count": route.get("unique_group_id_count").cloned().unwrap_or(Value::Null),
                "unique_write_group_id_count": Value::Null,
                "duplicate_payload_groups": route.get("duplicate_payload_groups").cloned().unwrap_or(Value::Null),
                "severity": Value::Null,
                "code": Value::Null,
                "message": Value::Null,
            }));
        }
    }

    if let Some(duplicates) = report.get("duplicates").and_then(Value::as_array) {
        for duplicate in duplicates {
            rows.push(json!({
                "row_type": "duplicate",
                "route": duplicate.get("route").cloned().unwrap_or(Value::Null),
                "class": Value::Null,
                "action_count": duplicate.get("count").cloned().unwrap_or(Value::Null),
                "planned_count": Value::Null,
                "write_count": Value::Null,
                "planned_write_count": Value::Null,
                "read_count": Value::Null,
                "unknown_count": Value::Null,
                "unique_contact_id_count": Value::Null,
                "unique_write_contact_id_count": Value::Null,
                "unique_group_id_count": Value::Null,
                "unique_write_group_id_count": Value::Null,
                "duplicate_payload_groups": Value::Null,
                "severity": Value::Null,
                "code": "duplicate_payload",
                "message": duplicate.get("payload").map(cell_string).unwrap_or_default(),
            }));
        }
    }

    if let Some(warnings) = report.get("warnings").and_then(Value::as_array) {
        for warning in warnings {
            rows.push(json!({
                "row_type": "warning",
                "route": Value::Null,
                "class": Value::Null,
                "action_count": Value::Null,
                "planned_count": Value::Null,
                "write_count": Value::Null,
                "planned_write_count": Value::Null,
                "read_count": Value::Null,
                "unknown_count": Value::Null,
                "unique_contact_id_count": Value::Null,
                "unique_write_contact_id_count": Value::Null,
                "unique_group_id_count": Value::Null,
                "unique_write_group_id_count": Value::Null,
                "duplicate_payload_groups": Value::Null,
                "severity": warning.get("severity").cloned().unwrap_or(Value::Null),
                "code": warning.get("code").cloned().unwrap_or(Value::Null),
                "message": warning.get("message").cloned().unwrap_or(Value::Null),
            }));
        }
    }

    Value::Array(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn object(values: &[(&str, Value)]) -> Map<String, Value> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    fn temp_plan_file(name: &str, value: &Value) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "meshx-plan-audit-{name}-{}-{nonce}.json",
            std::process::id()
        ));
        fs::write(&path, serde_json::to_string(value).unwrap()).unwrap();
        path
    }

    #[test]
    fn plan_audit_route_from_object_normalizes_mesh_routes() {
        assert_eq!(
            plan_audit_route_from_object(&object(&[("route", json!("search"))])),
            Some(("/tools/v2/search".to_string(), "/search".to_string()))
        );
        assert_eq!(
            plan_audit_route_from_object(&object(&[("route", json!("/search"))])),
            Some(("/tools/v2/search".to_string(), "/search".to_string()))
        );
        assert_eq!(
            plan_audit_route_from_object(&object(&[("route", json!("/tools/v2/search"))])),
            Some(("/tools/v2/search".to_string(), "/search".to_string()))
        );
    }

    #[test]
    fn plan_audit_route_from_object_does_not_strip_similar_prefixes() {
        assert_eq!(
            plan_audit_route_from_object(&object(&[("route", json!("/tools/v20/search"))])),
            Some((
                "/tools/v20/search".to_string(),
                "/tools/v20/search".to_string()
            ))
        );
    }

    #[test]
    fn plan_audit_payload_id_collection_accepts_nested_json_strings() {
        let action = plan_audit_action(
            1,
            "$".to_string(),
            "/tools/v2/update-group".to_string(),
            "/update-group".to_string(),
            json!({
                "group_id": "team-a",
                "add_contact_ids": "[1, \"2\"]",
                "nested": {
                    "remove_contact_ids": "3, 4"
                }
            }),
            false,
        );

        assert_eq!(action.contact_ids, [1, 2, 3, 4].into_iter().collect());
        assert_eq!(
            action.group_ids,
            ["team-a".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn plan_audit_action_class_separates_planned_and_actual_writes() {
        let mut action = plan_audit_action(
            1,
            "$".to_string(),
            "/tools/v2/create-contact".to_string(),
            "/create-contact".to_string(),
            json!({"contact_id": 1}),
            true,
        );
        action.known_route = true;
        action.write = true;

        assert_eq!(plan_audit_action_class(&action), "planned_write");
        action.planned = false;
        assert_eq!(plan_audit_action_class(&action), "write");
    }

    #[test]
    fn collect_plan_audit_actions_expands_route_bearing_chunks() {
        let input = json!({
            "plan": [
                {
                    "route": "/tools/v2/update-group",
                    "chunks": [
                        {
                            "route": "/tools/v2/update-group",
                            "payload": {"group_id": 7, "add_contact_ids": [1, 2]},
                        },
                        {
                            "route": "/tools/v2/update-group",
                            "payload": {"group_id": 7, "remove_contact_ids": [3]},
                        },
                    ],
                },
            ],
        });
        let mut actions = Vec::new();

        collect_plan_audit_actions(&input, "$", false, &mut actions);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].path, "$.plan[0].chunks[0]");
        assert_eq!(
            actions[0].payload,
            json!({"group_id": 7, "add_contact_ids": [1, 2]})
        );
        assert_eq!(actions[0].contact_ids, [1, 2].into_iter().collect());
        assert_eq!(
            actions[0].group_ids,
            ["7".to_string()].into_iter().collect()
        );
        assert!(actions.iter().all(|action| action.planned));
    }

    #[test]
    fn plan_audit_max_writes_counts_planned_writes() -> Result<()> {
        let path = temp_plan_file(
            "planned-writes",
            &json!({
                "plan": [
                    {
                        "route": "/tools/v2/update-contact",
                        "payload": {"contact_id": 42},
                    },
                ],
            }),
        );

        let report = plan_audit(PlanAuditOptions {
            input: path.clone(),
            input_format: InputFormat::Json,
            max_writes: Some(0),
            max_contact_ids: None,
            max_group_ids: None,
            id_sample_limit: PLAN_AUDIT_ID_SAMPLE_DEFAULT,
            duplicate_limit: PLAN_AUDIT_DUPLICATE_SAMPLE_DEFAULT,
            strict: false,
        })?;
        fs::remove_file(path).ok();

        assert_eq!(
            report.pointer("/summary/planned_write_count"),
            Some(&json!(1))
        );
        assert_eq!(report.pointer("/summary/error_count"), Some(&json!(1)));
        assert!(
            report
                .get("warnings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|warning| warning.get("code").and_then(Value::as_str)
                    == Some("max_writes_exceeded"))
        );
        Ok(())
    }

    #[test]
    fn plan_audit_write_id_limits_count_planned_writes() -> Result<()> {
        let path = temp_plan_file(
            "planned-write-ids",
            &json!({
                "plan": [
                    {
                        "route": "/tools/v2/update-group",
                        "payload": {
                            "group_id": "team-a",
                            "add_contact_ids": [1, 2],
                        },
                    },
                ],
            }),
        );

        let report = plan_audit(PlanAuditOptions {
            input: path.clone(),
            input_format: InputFormat::Json,
            max_writes: None,
            max_contact_ids: Some(1),
            max_group_ids: Some(0),
            id_sample_limit: PLAN_AUDIT_ID_SAMPLE_DEFAULT,
            duplicate_limit: PLAN_AUDIT_DUPLICATE_SAMPLE_DEFAULT,
            strict: false,
        })?;
        fs::remove_file(path).ok();

        assert_eq!(
            report.pointer("/summary/unique_write_contact_id_count"),
            Some(&json!(2))
        );
        assert_eq!(
            report.pointer("/summary/unique_write_group_id_count"),
            Some(&json!(1))
        );
        assert_eq!(report.pointer("/summary/error_count"), Some(&json!(2)));
        assert_eq!(
            report.pointer("/samples/write_contact_ids"),
            Some(&json!([1, 2]))
        );
        assert_eq!(
            report.pointer("/samples/write_group_ids"),
            Some(&json!(["team-a"]))
        );
        for code in ["max_contact_ids_exceeded", "max_group_ids_exceeded"] {
            assert!(
                report
                    .get("warnings")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|warning| warning.get("code").and_then(Value::as_str) == Some(code))
            );
        }
        Ok(())
    }

    #[test]
    fn plan_audit_duplicate_payloads_count_planned_actions() -> Result<()> {
        let path = temp_plan_file(
            "planned-duplicates",
            &json!({
                "plan": [
                    {
                        "route": "/tools/v2/update-contact",
                        "payload": {"contact_id": 42, "title": "cto"},
                    },
                    {
                        "route": "/tools/v2/update-contact",
                        "payload": {"contact_id": 42, "title": "cto"},
                    },
                ],
            }),
        );

        let report = plan_audit(PlanAuditOptions {
            input: path.clone(),
            input_format: InputFormat::Json,
            max_writes: None,
            max_contact_ids: None,
            max_group_ids: None,
            id_sample_limit: PLAN_AUDIT_ID_SAMPLE_DEFAULT,
            duplicate_limit: PLAN_AUDIT_DUPLICATE_SAMPLE_DEFAULT,
            strict: false,
        })?;
        fs::remove_file(path).ok();

        assert_eq!(
            report.pointer("/summary/duplicate_payload_group_count"),
            Some(&json!(1))
        );
        assert!(
            report
                .get("warnings")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .any(|warning| warning.get("code").and_then(Value::as_str)
                    == Some("duplicate_payloads"))
        );
        Ok(())
    }
}
