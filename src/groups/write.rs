use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GroupApplyKind {
    Create,
    Update,
    Add,
    Remove,
}

impl GroupApplyKind {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "create" | "groups:create" => Ok(Self::Create),
            "update" | "groups:update" => Ok(Self::Update),
            "add" | "add-members" | "add_contacts" | "add-contacts" => Ok(Self::Add),
            "remove" | "remove-members" | "remove_contacts" | "remove-contacts" => Ok(Self::Remove),
            other => err(format!(
                "group action must be create, update, add, or remove; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }

    pub(crate) fn route(self) -> &'static str {
        match self {
            Self::Create => route::CREATE_GROUP,
            Self::Update | Self::Add | Self::Remove => route::UPDATE_GROUP,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GroupApplyAction {
    pub(crate) row: usize,
    pub(crate) kind: GroupApplyKind,
    pub(crate) route: &'static str,
    pub(crate) payload: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct GroupApplyPlan {
    pub(crate) input_format: InputFormat,
    pub(crate) actions: Vec<GroupApplyAction>,
}

pub(crate) fn group_apply_plan_from_file(
    path: &Path,
    requested_format: InputFormat,
    default_action: GroupApplyKind,
    ignore_unknown: bool,
) -> Result<GroupApplyPlan> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let input_format = requested_format.resolve(path, &text);
    let rows = read_apply_rows(&text, input_format, "groups:apply")?;
    if rows.is_empty() {
        return err("groups:apply input did not contain any action rows");
    }
    let actions = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            group_apply_action_from_row(index + 1, &row, default_action, ignore_unknown)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(GroupApplyPlan {
        input_format,
        actions,
    })
}

pub(crate) fn group_apply_action_from_row(
    row_number: usize,
    row: &Map<String, Value>,
    default_action: GroupApplyKind,
    ignore_unknown: bool,
) -> Result<GroupApplyAction> {
    let kind = row_string(row, &["action", "op", "operation"])
        .as_deref()
        .map(GroupApplyKind::parse)
        .transpose()?
        .unwrap_or(default_action);
    validate_group_apply_fields(row_number, row, kind, ignore_unknown)?;
    let payload = match kind {
        GroupApplyKind::Create => group_create_payload(row)?,
        GroupApplyKind::Update => group_update_payload(row)?,
        GroupApplyKind::Add => group_member_delta_payload(row, true)?,
        GroupApplyKind::Remove => group_member_delta_payload(row, false)?,
    };
    Ok(GroupApplyAction {
        row: row_number,
        kind,
        route: kind.route(),
        payload,
    })
}

pub(crate) fn validate_group_apply_fields(
    row_number: usize,
    row: &Map<String, Value>,
    kind: GroupApplyKind,
    ignore_unknown: bool,
) -> Result<()> {
    if ignore_unknown {
        return Ok(());
    }
    let unknown = row
        .keys()
        .filter(|key| !group_apply_key_allowed(kind, key))
        .cloned()
        .collect::<Vec<_>>();
    if unknown.is_empty() {
        Ok(())
    } else {
        err(format!(
            "groups:apply row {row_number} has unknown field(s): {}. Use --ignore-unknown to ignore extra columns.",
            unknown.join(", ")
        ))
    }
}

pub(crate) fn group_apply_key_allowed(kind: GroupApplyKind, key: &str) -> bool {
    key_matches(key, &["action", "op", "operation"])
        || match kind {
            GroupApplyKind::Create => group_title_key(key),
            GroupApplyKind::Update => {
                group_id_key(key)
                    || group_title_key(key)
                    || group_add_key(key)
                    || group_remove_key(key)
                    || group_contact_ids_key(key)
            }
            GroupApplyKind::Add => {
                group_id_key(key) || group_add_key(key) || group_contact_ids_key(key)
            }
            GroupApplyKind::Remove => {
                group_id_key(key) || group_remove_key(key) || group_contact_ids_key(key)
            }
        }
}

pub(crate) async fn apply_group_actions(
    runtime: &Runtime,
    actions: Vec<GroupApplyAction>,
    concurrency: usize,
) -> Result<Value> {
    let mut results = Vec::with_capacity(actions.len());
    let mut failures = 0_u64;
    let outcomes = run_bulk_tool_calls(
        runtime,
        actions,
        concurrency,
        "joining groups:apply write task",
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

pub(crate) fn group_apply_action_value(action: &GroupApplyAction) -> Value {
    json!({
        "row": action.row,
        "action": action.kind.as_str(),
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload.clone(),
    })
}

pub(crate) fn group_sync_target_ids_from_matches(matches: &ArgMatches) -> Result<Vec<u64>> {
    target_contact_ids_from_matches(matches, "contact-ids")
}

pub(crate) fn group_sync_search_payload_from_matches(
    matches: &ArgMatches,
    all_search: bool,
) -> Result<Map<String, Value>> {
    search_target_payload_from_matches(matches, all_search, "groups:sync")
}

pub(crate) async fn groups_sync_plan(
    runtime: &Runtime,
    options: &GroupSyncOptions,
) -> Result<GroupSyncPlan> {
    let (_, groups, _) = groups_for_audit_live(runtime).await?;
    let group = groups
        .into_iter()
        .find(|group| {
            record_id(group).and_then(|id| parse_contact_id(&id).ok()) == Some(options.group_id)
        })
        .ok_or_else(|| miette!("group {} was not found in live me.sh", options.group_id))?;
    let group_name = group_name(&group).unwrap_or_default();
    if normalize_group_name_key(&group_name).is_some_and(|value| value == "starred") {
        return err("groups:sync cannot write the special Starred group");
    }

    let (target_ids, search_target_count, target_source) =
        group_sync_desired_target_ids(runtime, options).await?;
    let members = group_members_fetch_one(
        runtime.clone(),
        group.clone(),
        Vec::new(),
        None,
        options.page_size,
    )
    .await?;
    let current_ids = group_sync_member_ids(&members)?;
    let (add_ids, remove_ids) = group_sync_delta(&current_ids, &target_ids, options.mode);
    let add_actions = group_sync_actions(options.group_id, &add_ids, true, options.chunk_size, 1);
    let remove_actions = group_sync_actions(
        options.group_id,
        &remove_ids,
        false,
        options.chunk_size,
        add_actions.len() + 1,
    );

    Ok(GroupSyncPlan {
        group_id: options.group_id,
        group_name,
        group,
        mode: options.mode,
        page_size: options.page_size,
        chunk_size: options.chunk_size,
        target_source,
        search_payload: options.search_payload.clone(),
        search_target_count,
        current_ids,
        target_ids,
        add_ids,
        remove_ids,
        add_actions,
        remove_actions,
    })
}

pub(crate) async fn group_sync_desired_target_ids(
    runtime: &Runtime,
    options: &GroupSyncOptions,
) -> Result<(Vec<u64>, Option<usize>, String)> {
    let mut ids = options.target_ids.clone();
    let mut search_count = None;
    if let Some(payload) = &options.search_payload {
        let before = ids.len();
        let mut search_ids = Vec::new();
        let (exported, total) = export_contacts_each_limited(
            runtime,
            payload.clone(),
            options.page_size,
            None,
            |row| {
                let id = contact_id_from_value(&row)
                    .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
                search_ids.push(id);
                Ok(())
            },
        )
        .await?;
        search_count = Some(exported);
        if exported != total {
            return err(format!(
                "groups:sync search target exported {exported} contacts but me.sh reported {total}"
            ));
        }
        ids.extend(search_ids);
        ids = dedupe_ids(ids);
        let source = if before == 0 {
            "search".to_string()
        } else {
            "explicit+search".to_string()
        };
        return Ok((ids, search_count, source));
    }
    let source = if ids.is_empty() {
        "empty".to_string()
    } else {
        "explicit".to_string()
    };
    Ok((ids, search_count, source))
}

pub(crate) fn group_sync_member_ids(group_members: &Value) -> Result<Vec<u64>> {
    let members = group_members
        .get("members")
        .and_then(Value::as_array)
        .ok_or_else(|| miette!("groups:sync current member read did not return members"))?;
    let mut ids = Vec::with_capacity(members.len());
    for member in members {
        let id = contact_id_from_value(member)
            .ok_or_else(|| miette!("group member row did not include numeric id"))?;
        ids.push(id);
    }
    Ok(dedupe_ids(ids))
}

pub(crate) fn group_sync_delta(
    current_ids: &[u64],
    target_ids: &[u64],
    mode: GroupSyncMode,
) -> (Vec<u64>, Vec<u64>) {
    let current = current_ids.iter().copied().collect::<BTreeSet<_>>();
    let target = target_ids.iter().copied().collect::<BTreeSet<_>>();
    let add = if matches!(mode, GroupSyncMode::Replace | GroupSyncMode::AddOnly) {
        target.difference(&current).copied().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let remove = if matches!(mode, GroupSyncMode::Replace | GroupSyncMode::RemoveOnly) {
        current.difference(&target).copied().collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    (add, remove)
}

pub(crate) fn group_sync_actions(
    group_id: u64,
    ids: &[u64],
    add: bool,
    chunk_size: usize,
    row_offset: usize,
) -> Vec<GroupApplyAction> {
    ids.chunks(chunk_size)
        .enumerate()
        .map(|(index, chunk)| {
            let mut payload = Map::new();
            payload.insert(
                "group_id".to_string(),
                Value::Number(Number::from(group_id)),
            );
            payload.insert(
                if add {
                    "add_contact_ids".to_string()
                } else {
                    "remove_contact_ids".to_string()
                },
                json!(chunk),
            );
            GroupApplyAction {
                row: row_offset + index,
                kind: if add {
                    GroupApplyKind::Add
                } else {
                    GroupApplyKind::Remove
                },
                route: route::UPDATE_GROUP,
                payload,
            }
        })
        .collect()
}

pub(crate) fn group_sync_plan_value(plan: &GroupSyncPlan) -> Value {
    let actions = plan
        .add_actions
        .iter()
        .chain(plan.remove_actions.iter())
        .map(group_apply_action_value)
        .collect::<Vec<_>>();
    let target_ids = plan.target_ids.iter().copied().collect::<BTreeSet<_>>();
    let unchanged_count = plan
        .current_ids
        .iter()
        .filter(|id| target_ids.contains(id))
        .count();
    json!({
        "source": "live",
        "group": {
            "id": plan.group_id,
            "name": plan.group_name.clone(),
            "raw": plan.group.clone(),
        },
        "mode": plan.mode.as_str(),
        "target_source": plan.target_source.clone(),
        "search": plan.search_payload.as_ref().map(|payload| json!({
            "payload": payload.clone(),
            "matched_count": plan.search_target_count,
        })),
        "pagination": "exclude_contact_ids",
        "page_size": plan.page_size,
        "chunk_size": plan.chunk_size,
        "summary": {
            "current_count": plan.current_ids.len(),
            "target_count": plan.target_ids.len(),
            "unchanged_count": unchanged_count,
            "add_count": plan.add_ids.len(),
            "remove_count": plan.remove_ids.len(),
            "write_chunk_count": actions.len(),
            "write_required": !actions.is_empty(),
        },
        "current_contact_ids": plan.current_ids.clone(),
        "target_contact_ids": plan.target_ids.clone(),
        "add_contact_ids": plan.add_ids.clone(),
        "remove_contact_ids": plan.remove_ids.clone(),
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog"},
            {"route": "/tools/v2/search", "payload": plan.search_payload.clone(), "enabled": plan.search_payload.is_some(), "purpose": "when --from-search is set, page through all matching contacts to build desired IDs"},
            {"route": "/tools/v2/search", "payload": {"group_ids": [plan.group_id], "limit": plan.page_size, "exclude_contact_ids": "accumulated from prior pages"}, "purpose": "read current group members"},
            {"route": "/tools/v2/update-group", "chunks": actions, "purpose": "apply add chunks first, then remove chunks after add success"}
        ],
    })
}

pub(crate) async fn apply_group_sync(
    runtime: &Runtime,
    plan: &GroupSyncPlan,
    concurrency: usize,
) -> Result<Value> {
    let dry_run = group_sync_plan_value(plan);
    if plan.add_actions.is_empty() && plan.remove_actions.is_empty() {
        return Ok(json!({
            "ok": true,
            "changed": false,
            "dry_run": dry_run,
            "write_result": {
                "ok": true,
                "changed_count": 0,
                "failure_count": 0,
                "results": [],
            },
        }));
    }

    let add_result = apply_group_actions(runtime, plan.add_actions.clone(), concurrency).await?;
    let add_ok = add_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let remove_result = if add_ok {
        apply_group_actions(runtime, plan.remove_actions.clone(), concurrency).await?
    } else if plan.remove_actions.is_empty() {
        json!({
            "ok": true,
            "changed_count": 0,
            "failure_count": 0,
            "results": [],
        })
    } else {
        json!({
            "ok": false,
            "changed_count": 0,
            "failure_count": plan.remove_actions.len(),
            "skipped": true,
            "reason": "remove phase skipped because add phase failed",
            "results": [],
        })
    };
    let remove_ok = remove_result
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(json!({
        "ok": add_ok && remove_ok,
        "changed": true,
        "dry_run": dry_run,
        "write_result": {
            "add": add_result,
            "remove": remove_result,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_sync_actions_chunk_add_payloads() {
        let actions = group_sync_actions(5, &[1, 2, 3], true, 2, 10);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].row, 10);
        assert_eq!(actions[0].kind, GroupApplyKind::Add);
        assert_eq!(actions[0].route, "/update-group");
        assert_eq!(
            Value::Object(actions[0].payload.clone()),
            json!({"group_id": 5, "add_contact_ids": [1, 2]})
        );
        assert_eq!(
            Value::Object(actions[1].payload.clone()),
            json!({"group_id": 5, "add_contact_ids": [3]})
        );
    }

    #[test]
    fn group_sync_actions_build_remove_payloads() {
        let actions = group_sync_actions(5, &[8, 9], false, 10, 1);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, GroupApplyKind::Remove);
        assert_eq!(
            Value::Object(actions[0].payload.clone()),
            json!({"group_id": 5, "remove_contact_ids": [8, 9]})
        );
    }
}
