use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct ContactApplyAction {
    pub(crate) row: usize,
    pub(crate) kind: ApplyKind,
    pub(crate) route: &'static str,
    pub(crate) payload: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactApplyPlan {
    pub(crate) input_format: InputFormat,
    pub(crate) actions: Vec<ContactApplyAction>,
}

#[derive(Clone, Debug)]
pub(crate) struct NotesBulkCreateOptions {
    pub(crate) target_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) content: String,
    pub(crate) reminder_date: Option<String>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct NotesBulkCreatePlan {
    pub(crate) target_source: String,
    pub(crate) explicit_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) search_exported_count: Option<usize>,
    pub(crate) search_match_count: Option<usize>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) actions: Vec<ContactApplyAction>,
    pub(crate) content: String,
    pub(crate) reminder_date: Option<String>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) concurrency: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactBulkStateOptions {
    pub(crate) kind: ApplyKind,
    pub(crate) command: &'static str,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) chunk_size: usize,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactBulkStatePlan {
    pub(crate) kind: ApplyKind,
    pub(crate) command: &'static str,
    pub(crate) target_source: String,
    pub(crate) explicit_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) search_exported_count: Option<usize>,
    pub(crate) search_match_count: Option<usize>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) actions: Vec<ContactApplyAction>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) chunk_size: usize,
    pub(crate) concurrency: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactBulkUpdateOptions {
    pub(crate) target_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) mutation_payload: Map<String, Value>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactBulkUpdatePlan {
    pub(crate) target_source: String,
    pub(crate) explicit_ids: Vec<u64>,
    pub(crate) search_payload: Option<Map<String, Value>>,
    pub(crate) search_exported_count: Option<usize>,
    pub(crate) search_match_count: Option<usize>,
    pub(crate) target_ids: Vec<u64>,
    pub(crate) actions: Vec<ContactApplyAction>,
    pub(crate) mutation_payload: Map<String, Value>,
    pub(crate) page_size: usize,
    pub(crate) target_limit: Option<usize>,
    pub(crate) concurrency: usize,
}

impl NotesBulkCreateOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let content = matches
            .get_one::<String>("content")
            .expect("required by clap")
            .trim()
            .to_string();
        if content.is_empty() {
            return err("--content must not be empty");
        }
        let target_ids = target_contact_ids_from_matches(matches, "contact-ids")?;
        let from_search = matches.get_flag("from-search");
        let all_search = matches.get_flag("all-search");
        if all_search && !from_search {
            return err("notes:bulk-create --all-search requires --from-search");
        }
        let search_payload = if from_search {
            Some(search_target_payload_from_matches(
                matches,
                all_search,
                "notes:bulk-create",
            )?)
        } else {
            None
        };
        if target_ids.is_empty() && search_payload.is_none() {
            return err("notes:bulk-create requires --contact-ids, --input, or --from-search");
        }
        Ok(Self {
            target_ids,
            search_payload,
            content,
            reminder_date: matches
                .get_one::<String>("reminder-date")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            target_limit: optional_positive_usize_from_matches(matches, "target-limit")?,
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactBulkStateOptions {
    pub(crate) fn from_matches(
        matches: &ArgMatches,
        kind: ApplyKind,
        command: &'static str,
    ) -> Result<Self> {
        if !matches!(kind, ApplyKind::Archive | ApplyKind::Restore) {
            return err("contact bulk state action must be archive or restore");
        }
        let target_ids = target_contact_ids_from_matches(matches, "contact-ids")?;
        let from_search = matches.get_flag("from-search");
        let all_search = matches.get_flag("all-search");
        if all_search && !from_search {
            return err(format!("{command} --all-search requires --from-search"));
        }
        let search_payload = if from_search {
            Some(search_target_payload_from_matches(
                matches, all_search, command,
            )?)
        } else {
            None
        };
        if target_ids.is_empty() && search_payload.is_none() {
            return err(format!(
                "{command} requires --contact-ids, --input, or --from-search"
            ));
        }
        Ok(Self {
            kind,
            command,
            target_ids,
            search_payload,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            target_limit: optional_positive_usize_from_matches(matches, "target-limit")?,
            chunk_size: optional_usize_from_matches(matches, "chunk-size")?.unwrap_or(500),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactBulkUpdateOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let target_ids = target_contact_ids_from_matches(matches, "contact-ids")?;
        let from_search = matches.get_flag("from-search");
        let all_search = matches.get_flag("all-search");
        if all_search && !from_search {
            return err("contacts:bulk-update --all-search requires --from-search");
        }
        let search_payload = if from_search {
            Some(search_target_payload_from_matches(
                matches,
                all_search,
                "contacts:bulk-update",
            )?)
        } else {
            None
        };
        if target_ids.is_empty() && search_payload.is_none() {
            return err("contacts:bulk-update requires --contact-ids, --input, or --from-search");
        }
        let mutation_spec = CommandSpec {
            name: "contacts:bulk-update",
            tool_name: "updateContact",
            route_path: route::UPDATE_CONTACT,
            description: "Apply the same contact field updates to many contacts.",
            options: contact_mutation_options(),
            nested: &[],
            destructive: true,
        };
        let mutation_payload = parse_payload(&mutation_spec, matches)?;
        if mutation_payload.is_empty() {
            return err("contacts:bulk-update requires at least one contact field to update");
        }
        Ok(Self {
            target_ids,
            search_payload,
            mutation_payload,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            target_limit: optional_positive_usize_from_matches(matches, "target-limit")?,
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

pub(crate) fn contact_apply_plan_from_file(
    path: &Path,
    requested_format: InputFormat,
    default_action: ApplyKind,
    ignore_unknown: bool,
) -> Result<ContactApplyPlan> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let input_format = requested_format.resolve(path, &text);
    let rows = read_apply_rows(&text, input_format, "contacts:apply")?;
    if rows.is_empty() {
        return err("contacts:apply input did not contain any action rows");
    }
    let actions = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            contact_apply_action_from_row(index + 1, &row, default_action, ignore_unknown)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(ContactApplyPlan {
        input_format,
        actions,
    })
}

pub(crate) fn contact_apply_action_from_row(
    row_number: usize,
    row: &Map<String, Value>,
    default_action: ApplyKind,
    ignore_unknown: bool,
) -> Result<ContactApplyAction> {
    let kind = row_string(row, &["action", "op", "operation"])
        .as_deref()
        .map(ApplyKind::parse)
        .transpose()?
        .unwrap_or(default_action);
    validate_contact_apply_fields(row_number, row, kind, ignore_unknown)?;
    let payload = match kind {
        ApplyKind::Create => contact_apply_contact_payload(row, false)?,
        ApplyKind::Update => contact_apply_contact_payload(row, true)?,
        ApplyKind::Archive | ApplyKind::Restore => contact_apply_ids_payload(row)?,
        ApplyKind::Note => contact_apply_note_payload(row)?,
    };
    Ok(ContactApplyAction {
        row: row_number,
        kind,
        route: kind.route(),
        payload,
    })
}

pub(crate) fn validate_contact_apply_fields(
    row_number: usize,
    row: &Map<String, Value>,
    kind: ApplyKind,
    ignore_unknown: bool,
) -> Result<()> {
    if ignore_unknown {
        return Ok(());
    }
    let unknown = row
        .keys()
        .filter(|key| !contact_apply_key_allowed(kind, key))
        .cloned()
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        Ok(())
    } else {
        err(format!(
            "contacts:apply row {row_number} has unknown field(s): {}. Use --ignore-unknown to ignore extra columns.",
            unknown.join(", ")
        ))
    }
}

pub(crate) fn contact_apply_key_allowed(kind: ApplyKind, key: &str) -> bool {
    key_matches(key, &["action", "op", "operation"])
        || match kind {
            ApplyKind::Create | ApplyKind::Update => {
                contact_apply_mutation_key(key) || contact_apply_id_key(key)
            }
            ApplyKind::Archive | ApplyKind::Restore => {
                contact_apply_id_key(key) || key_matches(key, &["contact-ids", "contactIds", "ids"])
            }
            ApplyKind::Note => {
                contact_apply_id_key(key)
                    || key_matches(key, &["content", "note", "text", "body"])
                    || key_matches(
                        key,
                        &["reminder-date", "reminderDate", "reminder", "reminder-at"],
                    )
            }
        }
}

pub(crate) fn contact_apply_mutation_key(key: &str) -> bool {
    contact_apply_mutation_field(key).is_some()
}

pub(crate) fn contact_apply_mutation_field(key: &str) -> Option<(&'static str, ValueKind)> {
    let normalized = normalize_apply_key(key);
    match normalized.as_str() {
        "firstname" | "first" => Some(("first_name", ValueKind::String)),
        "lastname" | "last" => Some(("last_name", ValueKind::String)),
        "phone" | "phones" | "phonenumber" | "phonenumbers" => {
            Some(("phone", ValueKind::ArrayString))
        }
        "email" | "emails" => Some(("email", ValueKind::ArrayString)),
        "linkedin" | "linkedinurl" => Some(("linkedin", ValueKind::String)),
        "locations" | "location" => Some(("locations", ValueKind::Json)),
        "bio" | "biography" => Some(("bio", ValueKind::String)),
        "website" | "websites" | "url" | "urls" => Some(("website", ValueKind::ArrayString)),
        "title" => Some(("title", ValueKind::String)),
        "organization" | "company" => Some(("organization", ValueKind::String)),
        "birthday" => Some(("birthday", ValueKind::String)),
        _ => None,
    }
}

pub(crate) fn contact_apply_id_key(key: &str) -> bool {
    key_matches(key, &["id", "contact-id", "contactId", "contact"])
}

pub(crate) fn contact_apply_contact_payload(
    row: &Map<String, Value>,
    require_id: bool,
) -> Result<Map<String, Value>> {
    let mut payload = Map::new();
    if require_id {
        let id = row_u64(row, &["contact-id", "contactId", "id", "contact"])?
            .ok_or_else(|| miette!("contacts:apply update rows require contact_id or id"))?;
        payload.set("contact_id", id);
    }
    for (key, value) in row {
        let Some((payload_key, kind)) = contact_apply_mutation_field(key) else {
            continue;
        };
        match kind {
            ValueKind::String => {
                if let Some(value) = value_string(value).map(|value| value.trim().to_string())
                    && !value.is_empty()
                {
                    payload.insert(payload_key.to_string(), Value::String(value));
                }
            }
            ValueKind::ArrayString => {
                let values = string_array_from_value(value);
                if !values.is_empty() {
                    payload.insert(
                        payload_key.to_string(),
                        Value::Array(values.into_iter().map(Value::String).collect()),
                    );
                }
            }
            ValueKind::Json => {
                if let Some(value) = json_value_from_input(value, payload_key)? {
                    payload.insert(payload_key.to_string(), value);
                }
            }
            ValueKind::Number
            | ValueKind::Boolean
            | ValueKind::ArrayNumber
            | ValueKind::ArrayMixed => {
                unreachable!("contact mutation rows only use string, string-array, and JSON fields")
            }
        }
    }
    let field_count = payload
        .keys()
        .filter(|key| key.as_str() != "contact_id")
        .count();
    if field_count == 0 {
        return err("contacts:apply contact rows must contain at least one contact field");
    }
    Ok(payload)
}

pub(crate) fn contact_apply_ids_payload(row: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut ids = row_u64_array(row, &["contact-ids", "contactIds", "ids"])?;
    if ids.is_empty()
        && let Some(id) = row_u64(row, &["contact-id", "contactId", "id", "contact"])?
    {
        ids.push(id);
    }
    if ids.is_empty() {
        return err("contacts:apply archive/restore rows require contact_ids or contact_id");
    }
    let mut payload = Map::new();
    payload.set("contact_ids", json!(ids));
    Ok(payload)
}

pub(crate) fn contact_apply_note_payload(row: &Map<String, Value>) -> Result<Map<String, Value>> {
    let contact_id = row_u64(row, &["contact-id", "contactId", "id", "contact"])?
        .ok_or_else(|| miette!("contacts:apply note rows require contact_id or id"))?;
    let content = row_string(row, &["content", "note", "text", "body"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| miette!("contacts:apply note rows require content or note"))?;
    let mut payload = Map::new();
    payload.insert(
        "contact_id".to_string(),
        Value::Number(Number::from(contact_id)),
    );
    payload.set("content", content);
    if let Some(reminder_date) = row_string(
        row,
        &["reminder-date", "reminderDate", "reminder", "reminder-at"],
    )
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    {
        payload.set("reminder_date", reminder_date);
    }
    Ok(payload)
}

pub(crate) async fn apply_contact_actions(
    runtime: &Runtime,
    actions: Vec<ContactApplyAction>,
    concurrency: usize,
) -> Result<Value> {
    let mut results = Vec::with_capacity(actions.len());
    let mut failures = 0_u64;
    let outcomes = run_bulk_tool_calls(
        runtime,
        actions,
        concurrency,
        "joining contacts:apply write task",
        |action| (action.route, Value::Object(action.payload.clone())),
    )
    .await?;
    for (action, result) in outcomes {
        match result {
            Ok(data) => {
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "route": format!("/tools/v2{}", action.route),
                    "ok": true,
                    "result_id": record_id(&data),
                    "result": data,
                }));
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "route": format!("/tools/v2{}", action.route),
                    "ok": false,
                    "error": error.to_string(),
                }));
            }
        }
    }
    Ok(json!({
        "ok": failures == 0,
        "changed_count": results.len().saturating_sub(failures as usize),
        "failure_count": failures,
        "results": results,
    }))
}

pub(crate) fn contact_apply_action_value(action: &ContactApplyAction) -> Value {
    json!({
        "row": action.row,
        "action": action.kind.as_str(),
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload.clone(),
    })
}

pub(crate) async fn notes_bulk_create_plan(
    runtime: &Runtime,
    options: &NotesBulkCreateOptions,
) -> Result<NotesBulkCreatePlan> {
    let explicit_ids = options.target_ids.clone();
    let mut target_ids = explicit_ids.clone();
    if let Some(limit) = options.target_limit {
        target_ids.truncate(limit);
    }

    let mut search_exported_count = None;
    let mut search_match_count = None;
    if let Some(search_payload) = &options.search_payload {
        let remaining = options
            .target_limit
            .map(|limit| limit.saturating_sub(target_ids.len()));
        if remaining != Some(0) {
            let mut payload = search_payload.clone();
            append_exclude_contact_ids(&mut payload, &target_ids)?;
            let mut search_ids = Vec::new();
            let (exported, total) = export_contacts_each_limited(
                runtime,
                payload,
                options.page_size,
                remaining,
                |row| {
                    let id = contact_id_from_value(&row)
                        .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
                    search_ids.push(id);
                    Ok(())
                },
            )
            .await?;
            search_exported_count = Some(exported);
            search_match_count = Some(total);
            target_ids.extend(search_ids);
            target_ids = dedupe_ids(target_ids);
            if let Some(limit) = options.target_limit {
                target_ids.truncate(limit);
            }
        } else {
            search_exported_count = Some(0);
            search_match_count = Some(0);
        }
    }

    if target_ids.is_empty() {
        return err("notes:bulk-create resolved zero target contacts");
    }

    let actions = target_ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            notes_bulk_create_action(
                index + 1,
                *id,
                &options.content,
                options.reminder_date.as_ref(),
            )
        })
        .collect::<Vec<_>>();
    let target_source = match (explicit_ids.is_empty(), options.search_payload.is_some()) {
        (false, true) => "explicit+search",
        (false, false) => "explicit",
        (true, true) => "search",
        (true, false) => "none",
    }
    .to_string();

    Ok(NotesBulkCreatePlan {
        target_source,
        explicit_ids,
        search_payload: options.search_payload.clone(),
        search_exported_count,
        search_match_count,
        target_ids,
        actions,
        content: options.content.clone(),
        reminder_date: options.reminder_date.clone(),
        page_size: options.page_size,
        target_limit: options.target_limit,
        concurrency: options.concurrency,
    })
}

pub(crate) fn notes_bulk_create_action(
    row: usize,
    contact_id: u64,
    content: &str,
    reminder_date: Option<&String>,
) -> ContactApplyAction {
    let mut payload = Map::new();
    payload.insert(
        "contact_id".to_string(),
        Value::Number(Number::from(contact_id)),
    );
    payload.set("content", content.to_string());
    if let Some(reminder_date) = reminder_date {
        payload.insert(
            "reminder_date".to_string(),
            Value::String(reminder_date.clone()),
        );
    }
    ContactApplyAction {
        row,
        kind: ApplyKind::Note,
        route: route::NOTE,
        payload,
    }
}

pub(crate) fn notes_bulk_create_plan_value(plan: &NotesBulkCreatePlan) -> Value {
    json!({
        "source": "live",
        "target_source": plan.target_source,
        "filters": {
            "contact_ids": plan.explicit_ids,
            "from_search": plan.search_payload.is_some(),
            "search_payload": plan.search_payload,
            "target_limit": plan.target_limit,
        },
        "note": {
            "content": plan.content,
            "reminder_date": plan.reminder_date,
        },
        "summary": {
            "target_count": plan.target_ids.len(),
            "explicit_count": plan.explicit_ids.len(),
            "search_exported_count": plan.search_exported_count,
            "search_match_count": plan.search_match_count,
            "write_required": !plan.actions.is_empty(),
        },
        "page_size": plan.page_size,
        "concurrency": plan.concurrency,
        "plan": [
            {
                "route": "/tools/v2/search",
                "enabled": plan.search_payload.is_some(),
                "payload": "same search filters with limit set to page_size and exclude_contact_ids accumulated from explicit/prior IDs",
                "page_size": plan.page_size,
                "purpose": "resolve note target contact IDs without writes",
            },
            {
                "route": "/tools/v2/note",
                "payload": {"contact_id": "one target ID per request", "content": plan.content, "reminder_date": plan.reminder_date},
                "concurrency": plan.concurrency,
                "purpose": "create one note/reminder per target contact; requires --yes outside dry-run",
            }
        ],
        "actions": plan.actions.iter().map(notes_bulk_create_action_value).collect::<Vec<_>>(),
    })
}

pub(crate) async fn apply_notes_bulk_create(
    runtime: &Runtime,
    plan: &NotesBulkCreatePlan,
) -> Result<Value> {
    let mut results = Vec::with_capacity(plan.actions.len());
    let mut failures = 0_u64;
    let outcomes = run_bulk_tool_calls(
        runtime,
        plan.actions.clone(),
        plan.concurrency,
        "joining notes:bulk-create write task",
        |action| (action.route, Value::Object(action.payload.clone())),
    )
    .await?;
    for (action, result) in outcomes {
        let contact_id = action
            .payload
            .get("contact_id")
            .cloned()
            .unwrap_or(Value::Null);
        match result {
            Ok(data) => {
                results.push(json!({
                    "row": action.row,
                    "contact_id": contact_id,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": true,
                    "content": plan.content,
                    "reminder_date": plan.reminder_date,
                    "result_id": record_id(&data),
                    "result": data,
                }));
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                results.push(json!({
                    "row": action.row,
                    "contact_id": contact_id,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": false,
                    "content": plan.content,
                    "reminder_date": plan.reminder_date,
                    "error": error.to_string(),
                }));
            }
        }
    }
    Ok(json!({
        "source": "live",
        "target_source": plan.target_source,
        "summary": {
            "target_count": plan.target_ids.len(),
            "changed_count": results.len().saturating_sub(failures as usize),
            "failure_count": failures,
            "ok": failures == 0,
        },
        "note": {
            "content": plan.content,
            "reminder_date": plan.reminder_date,
        },
        "results": results,
    }))
}

pub(crate) fn notes_bulk_create_action_value(action: &ContactApplyAction) -> Value {
    json!({
        "row": action.row,
        "contact_id": action.payload.get("contact_id").cloned().unwrap_or(Value::Null),
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload,
    })
}

pub(crate) fn notes_bulk_create_rows(report: &Value) -> Value {
    let rows = report
        .get("results")
        .or_else(|| report.get("actions"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(notes_bulk_create_row)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(rows)
}

pub(crate) fn notes_bulk_create_row(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let payload = object.get("payload").unwrap_or(&Value::Null);
    Some(json!({
        "row": object.get("row").cloned().unwrap_or(Value::Null),
        "contact_id": object
            .get("contact_id")
            .cloned()
            .or_else(|| payload.get("contact_id").cloned())
            .unwrap_or(Value::Null),
        "route": object.get("route").cloned().unwrap_or(Value::Null),
        "ok": object.get("ok").cloned().unwrap_or(Value::Null),
        "result_id": object.get("result_id").cloned().unwrap_or(Value::Null),
        "error": object.get("error").cloned().unwrap_or(Value::Null),
        "content": object
            .get("content")
            .cloned()
            .or_else(|| payload.get("content").cloned())
            .unwrap_or(Value::Null),
        "reminder_date": object
            .get("reminder_date")
            .cloned()
            .or_else(|| payload.get("reminder_date").cloned())
            .unwrap_or(Value::Null),
    }))
}

pub(crate) async fn contact_bulk_state_plan(
    runtime: &Runtime,
    options: &ContactBulkStateOptions,
) -> Result<ContactBulkStatePlan> {
    let explicit_ids = options.target_ids.clone();
    let mut target_ids = explicit_ids.clone();
    if let Some(limit) = options.target_limit {
        target_ids.truncate(limit);
    }

    let mut search_exported_count = None;
    let mut search_match_count = None;
    if let Some(search_payload) = &options.search_payload {
        let remaining = options
            .target_limit
            .map(|limit| limit.saturating_sub(target_ids.len()));
        if remaining != Some(0) {
            let mut payload = search_payload.clone();
            append_exclude_contact_ids(&mut payload, &target_ids)?;
            let mut search_ids = Vec::new();
            let (exported, total) = export_contacts_each_limited(
                runtime,
                payload,
                options.page_size,
                remaining,
                |row| {
                    let id = contact_id_from_value(&row)
                        .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
                    search_ids.push(id);
                    Ok(())
                },
            )
            .await?;
            search_exported_count = Some(exported);
            search_match_count = Some(total);
            target_ids.extend(search_ids);
            target_ids = dedupe_ids(target_ids);
            if let Some(limit) = options.target_limit {
                target_ids.truncate(limit);
            }
        } else {
            search_exported_count = Some(0);
            search_match_count = Some(0);
        }
    }

    if target_ids.is_empty() {
        return err(format!("{} resolved zero target contacts", options.command));
    }

    let actions = target_ids
        .chunks(options.chunk_size)
        .enumerate()
        .map(|(index, ids)| contact_bulk_state_action(index + 1, options.kind, ids))
        .collect::<Vec<_>>();
    let target_source = match (explicit_ids.is_empty(), options.search_payload.is_some()) {
        (false, true) => "explicit+search",
        (false, false) => "explicit",
        (true, true) => "search",
        (true, false) => "none",
    }
    .to_string();

    Ok(ContactBulkStatePlan {
        kind: options.kind,
        command: options.command,
        target_source,
        explicit_ids,
        search_payload: options.search_payload.clone(),
        search_exported_count,
        search_match_count,
        target_ids,
        actions,
        page_size: options.page_size,
        target_limit: options.target_limit,
        chunk_size: options.chunk_size,
        concurrency: options.concurrency,
    })
}

pub(crate) fn contact_bulk_state_action(
    row: usize,
    kind: ApplyKind,
    contact_ids: &[u64],
) -> ContactApplyAction {
    let mut payload = Map::new();
    payload.set("contact_ids", json!(contact_ids));
    ContactApplyAction {
        row,
        kind,
        route: kind.route(),
        payload,
    }
}

pub(crate) fn contact_bulk_state_plan_value(plan: &ContactBulkStatePlan) -> Value {
    json!({
        "source": "live",
        "action": plan.kind.as_str(),
        "target_source": plan.target_source,
        "filters": {
            "contact_ids": plan.explicit_ids,
            "from_search": plan.search_payload.is_some(),
            "search_payload": plan.search_payload,
            "target_limit": plan.target_limit,
        },
        "summary": {
            "target_count": plan.target_ids.len(),
            "explicit_count": plan.explicit_ids.len(),
            "search_exported_count": plan.search_exported_count,
            "search_match_count": plan.search_match_count,
            "chunk_count": plan.actions.len(),
            "write_required": !plan.actions.is_empty(),
        },
        "page_size": plan.page_size,
        "chunk_size": plan.chunk_size,
        "concurrency": plan.concurrency,
        "plan": [
            {
                "route": "/tools/v2/search",
                "enabled": plan.search_payload.is_some(),
                "payload": "same search filters with limit set to page_size and exclude_contact_ids accumulated from explicit/prior IDs",
                "page_size": plan.page_size,
                "purpose": "resolve target contact IDs without writes",
            },
            {
                "route": format!("/tools/v2{}", plan.kind.route()),
                "payload": {"contact_ids": "up to chunk_size target IDs per request"},
                "chunk_size": plan.chunk_size,
                "concurrency": plan.concurrency,
                "purpose": format!("{} selected target contacts; requires --yes outside dry-run", plan.kind.as_str()),
            }
        ],
        "actions": plan.actions.iter().map(contact_bulk_state_action_value).collect::<Vec<_>>(),
    })
}

pub(crate) async fn apply_contact_bulk_state(
    runtime: &Runtime,
    plan: &ContactBulkStatePlan,
) -> Result<Value> {
    let mut results = Vec::with_capacity(plan.actions.len());
    let mut failures = 0_u64;
    let mut changed_count = 0_usize;
    let mut failed_target_count = 0_usize;
    let outcomes = run_bulk_tool_calls(
        runtime,
        plan.actions.clone(),
        plan.concurrency,
        &format!("joining {} write task", plan.command),
        |action| (action.route, Value::Object(action.payload.clone())),
    )
    .await?;
    for (action, result) in outcomes {
        let contact_ids = action
            .payload
            .get("contact_ids")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        let target_count = contact_ids.as_array().map(Vec::len).unwrap_or_default();
        match result {
            Ok(data) => {
                changed_count = changed_count.saturating_add(target_count);
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "contact_ids": contact_ids,
                    "target_count": target_count,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": true,
                    "result_id": record_id(&data),
                    "result": data,
                }));
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                failed_target_count = failed_target_count.saturating_add(target_count);
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "contact_ids": contact_ids,
                    "target_count": target_count,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": false,
                    "error": error.to_string(),
                }));
            }
        }
    }
    Ok(json!({
        "source": "live",
        "action": plan.kind.as_str(),
        "target_source": plan.target_source,
        "summary": {
            "target_count": plan.target_ids.len(),
            "chunk_count": plan.actions.len(),
            "changed_count": changed_count,
            "failed_target_count": failed_target_count,
            "failure_count": failures,
            "ok": failures == 0,
        },
        "results": results,
    }))
}

pub(crate) fn contact_bulk_state_action_value(action: &ContactApplyAction) -> Value {
    let contact_ids = action
        .payload
        .get("contact_ids")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let target_count = contact_ids.as_array().map(Vec::len).unwrap_or_default();
    json!({
        "row": action.row,
        "action": action.kind.as_str(),
        "contact_ids": contact_ids,
        "target_count": target_count,
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload,
    })
}

pub(crate) fn contact_bulk_state_rows(report: &Value) -> Value {
    let rows = report
        .get("results")
        .or_else(|| report.get("actions"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(contact_bulk_state_row)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(rows)
}

pub(crate) fn contact_bulk_state_row(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let payload = object.get("payload").unwrap_or(&Value::Null);
    let contact_ids = object
        .get("contact_ids")
        .cloned()
        .or_else(|| payload.get("contact_ids").cloned())
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let target_count = object.get("target_count").cloned().unwrap_or_else(|| {
        Value::Number(Number::from(
            contact_ids.as_array().map(Vec::len).unwrap_or_default(),
        ))
    });
    Some(json!({
        "row": object.get("row").cloned().unwrap_or(Value::Null),
        "action": object.get("action").cloned().unwrap_or(Value::Null),
        "route": object.get("route").cloned().unwrap_or(Value::Null),
        "ok": object.get("ok").cloned().unwrap_or(Value::Null),
        "target_count": target_count,
        "contact_ids": contact_ids,
        "result_id": object.get("result_id").cloned().unwrap_or(Value::Null),
        "error": object.get("error").cloned().unwrap_or(Value::Null),
    }))
}

pub(crate) async fn contact_bulk_update_plan(
    runtime: &Runtime,
    options: &ContactBulkUpdateOptions,
) -> Result<ContactBulkUpdatePlan> {
    let explicit_ids = options.target_ids.clone();
    let mut target_ids = explicit_ids.clone();
    if let Some(limit) = options.target_limit {
        target_ids.truncate(limit);
    }

    let mut search_exported_count = None;
    let mut search_match_count = None;
    if let Some(search_payload) = &options.search_payload {
        let remaining = options
            .target_limit
            .map(|limit| limit.saturating_sub(target_ids.len()));
        if remaining != Some(0) {
            let mut payload = search_payload.clone();
            append_exclude_contact_ids(&mut payload, &target_ids)?;
            let mut search_ids = Vec::new();
            let (exported, total) = export_contacts_each_limited(
                runtime,
                payload,
                options.page_size,
                remaining,
                |row| {
                    let id = contact_id_from_value(&row)
                        .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
                    search_ids.push(id);
                    Ok(())
                },
            )
            .await?;
            search_exported_count = Some(exported);
            search_match_count = Some(total);
            target_ids.extend(search_ids);
            target_ids = dedupe_ids(target_ids);
            if let Some(limit) = options.target_limit {
                target_ids.truncate(limit);
            }
        } else {
            search_exported_count = Some(0);
            search_match_count = Some(0);
        }
    }

    if target_ids.is_empty() {
        return err("contacts:bulk-update resolved zero target contacts");
    }

    let actions = target_ids
        .iter()
        .enumerate()
        .map(|(index, id)| contact_bulk_update_action(index + 1, *id, &options.mutation_payload))
        .collect::<Vec<_>>();
    let target_source = match (explicit_ids.is_empty(), options.search_payload.is_some()) {
        (false, true) => "explicit+search",
        (false, false) => "explicit",
        (true, true) => "search",
        (true, false) => "none",
    }
    .to_string();

    Ok(ContactBulkUpdatePlan {
        target_source,
        explicit_ids,
        search_payload: options.search_payload.clone(),
        search_exported_count,
        search_match_count,
        target_ids,
        actions,
        mutation_payload: options.mutation_payload.clone(),
        page_size: options.page_size,
        target_limit: options.target_limit,
        concurrency: options.concurrency,
    })
}

pub(crate) fn contact_bulk_update_action(
    row: usize,
    contact_id: u64,
    mutation_payload: &Map<String, Value>,
) -> ContactApplyAction {
    let mut payload = mutation_payload.clone();
    payload.insert(
        "contact_id".to_string(),
        Value::Number(Number::from(contact_id)),
    );
    ContactApplyAction {
        row,
        kind: ApplyKind::Update,
        route: ApplyKind::Update.route(),
        payload,
    }
}

pub(crate) fn contact_bulk_update_plan_value(plan: &ContactBulkUpdatePlan) -> Value {
    json!({
        "source": "live",
        "action": "update",
        "target_source": plan.target_source,
        "filters": {
            "contact_ids": plan.explicit_ids,
            "from_search": plan.search_payload.is_some(),
            "search_payload": plan.search_payload,
            "target_limit": plan.target_limit,
        },
        "updates": plan.mutation_payload,
        "summary": {
            "target_count": plan.target_ids.len(),
            "explicit_count": plan.explicit_ids.len(),
            "search_exported_count": plan.search_exported_count,
            "search_match_count": plan.search_match_count,
            "write_required": !plan.actions.is_empty(),
        },
        "page_size": plan.page_size,
        "concurrency": plan.concurrency,
        "plan": [
            {
                "route": "/tools/v2/search",
                "enabled": plan.search_payload.is_some(),
                "payload": "same search filters with limit set to page_size and exclude_contact_ids accumulated from explicit/prior IDs",
                "page_size": plan.page_size,
                "purpose": "resolve target contact IDs without writes",
            },
            {
                "route": "/tools/v2/update-contact",
                "payload": {"contact_id": "one target ID per request", "updates": plan.mutation_payload},
                "concurrency": plan.concurrency,
                "purpose": "update selected target contacts; requires --yes outside dry-run",
            }
        ],
        "actions": plan.actions.iter().map(contact_bulk_update_action_value).collect::<Vec<_>>(),
    })
}

pub(crate) async fn apply_contact_bulk_update(
    runtime: &Runtime,
    plan: &ContactBulkUpdatePlan,
) -> Result<Value> {
    let mut results = Vec::with_capacity(plan.actions.len());
    let mut failures = 0_u64;
    let outcomes = run_bulk_tool_calls(
        runtime,
        plan.actions.clone(),
        plan.concurrency,
        "joining contacts:bulk-update write task",
        |action| (action.route, Value::Object(action.payload.clone())),
    )
    .await?;
    for (action, result) in outcomes {
        let contact_id = action
            .payload
            .get("contact_id")
            .cloned()
            .unwrap_or(Value::Null);
        match result {
            Ok(data) => {
                results.push(json!({
                    "row": action.row,
                    "action": "update",
                    "contact_id": contact_id,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": true,
                    "updates": plan.mutation_payload,
                    "result_id": record_id(&data),
                    "result": data,
                }));
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                results.push(json!({
                    "row": action.row,
                    "action": "update",
                    "contact_id": contact_id,
                    "route": format!("/tools/v2{}", action.route),
                    "ok": false,
                    "updates": plan.mutation_payload,
                    "error": error.to_string(),
                }));
            }
        }
    }
    Ok(json!({
        "source": "live",
        "action": "update",
        "target_source": plan.target_source,
        "summary": {
            "target_count": plan.target_ids.len(),
            "changed_count": results.len().saturating_sub(failures as usize),
            "failure_count": failures,
            "ok": failures == 0,
        },
        "updates": plan.mutation_payload,
        "results": results,
    }))
}

pub(crate) fn contact_bulk_update_action_value(action: &ContactApplyAction) -> Value {
    json!({
        "row": action.row,
        "action": "update",
        "contact_id": action.payload.get("contact_id").cloned().unwrap_or(Value::Null),
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload,
    })
}

pub(crate) fn contact_bulk_update_rows(report: &Value) -> Value {
    let rows = report
        .get("results")
        .or_else(|| report.get("actions"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(contact_bulk_update_row)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(rows)
}

pub(crate) fn contact_bulk_update_row(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let payload = object.get("payload").unwrap_or(&Value::Null);
    Some(json!({
        "row": object.get("row").cloned().unwrap_or(Value::Null),
        "action": object.get("action").cloned().unwrap_or_else(|| Value::String("update".to_string())),
        "contact_id": object
            .get("contact_id")
            .cloned()
            .or_else(|| payload.get("contact_id").cloned())
            .unwrap_or(Value::Null),
        "route": object.get("route").cloned().unwrap_or(Value::Null),
        "ok": object.get("ok").cloned().unwrap_or(Value::Null),
        "result_id": object.get("result_id").cloned().unwrap_or(Value::Null),
        "error": object.get("error").cloned().unwrap_or(Value::Null),
        "updates": object
            .get("updates")
            .cloned()
            .unwrap_or_else(|| contact_bulk_update_payload_updates(payload)),
    }))
}

pub(crate) fn contact_bulk_update_payload_updates(payload: &Value) -> Value {
    let Some(object) = payload.as_object() else {
        return Value::Null;
    };
    let updates = object
        .iter()
        .filter(|(key, _)| key.as_str() != "contact_id")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    Value::Object(updates)
}

pub(crate) fn contact_map_merge_live_include_fields(
    payload: &mut Map<String, Value>,
    facets: &[ContactFacetKind],
) -> Result<()> {
    let mut fields = payload
        .get("include_fields")
        .map(|value| {
            string_array_from_value(value)
                .into_iter()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for facet in facets {
        for field in contact_map_include_fields_for_facet(*facet) {
            fields.insert(field.to_string());
        }
    }
    if !fields.is_empty() {
        payload.insert(
            "include_fields".to_string(),
            Value::Array(fields.into_iter().map(Value::String).collect()),
        );
    }
    Ok(())
}

pub(crate) fn contact_reconnect_merge_live_include_fields(payload: &mut Map<String, Value>) {
    let mut fields = payload
        .get("include_fields")
        .map(|value| {
            string_array_from_value(value)
                .into_iter()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for field in [
        "emails",
        "phone_numbers",
        "social_links",
        "interaction_history",
    ] {
        fields.insert(field.to_string());
    }
    payload.insert(
        "include_fields".to_string(),
        Value::Array(fields.into_iter().map(Value::String).collect()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[(&str, Value)]) -> Map<String, Value> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn contact_apply_ids_payload_accepts_plural_or_single_id() -> Result<()> {
        assert_eq!(
            Value::Object(contact_apply_ids_payload(&row(&[(
                "contact-ids",
                json!("1, 2")
            )]))?),
            json!({"contact_ids": [1, 2]})
        );
        assert_eq!(
            Value::Object(contact_apply_ids_payload(&row(&[(
                "contact-id",
                json!("3")
            )]))?),
            json!({"contact_ids": [3]})
        );
        assert!(contact_apply_ids_payload(&Map::new()).is_err());
        Ok(())
    }

    #[test]
    fn contact_bulk_state_action_builds_route_and_contact_ids() {
        let action = contact_bulk_state_action(4, ApplyKind::Archive, &[9, 8]);

        assert_eq!(action.row, 4);
        assert_eq!(action.kind, ApplyKind::Archive);
        assert_eq!(action.route, "/archive-contact");
        assert_eq!(
            Value::Object(action.payload),
            json!({"contact_ids": [9, 8]})
        );
    }
}
