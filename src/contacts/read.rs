use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct ContactActivityOptions {
    pub(crate) contact_ids: Vec<u64>,
    pub(crate) sections: Vec<&'static SnapshotMomentRoute>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) limit: usize,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactProfileOptions {
    pub(crate) contact_ids: Vec<u64>,
    pub(crate) activity_sections: Vec<&'static SnapshotMomentRoute>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) activity_limit: usize,
    pub(crate) group_scan_limit: usize,
    pub(crate) concurrency: usize,
    pub(crate) include_groups: bool,
    pub(crate) include_activity: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ContactGroupsOptions {
    pub(crate) contact_ids: Vec<u64>,
    pub(crate) group_query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) include_fields: Vec<String>,
    pub(crate) group_scan_limit: usize,
    pub(crate) concurrency: usize,
    pub(crate) flat: bool,
}

impl ContactActivityOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let contact_ids = dedupe_ids(contact_ids_from_matches(matches, "contact-ids")?);
        let sections = contact_activity_sections_from_matches(matches)?;
        let start = matches.get_one::<String>("start").cloned();
        let end = matches.get_one::<String>("end").cloned();
        if sections
            .iter()
            .any(|route| matches!(route.kind, SnapshotMomentKind::DateWindow))
            && (start.is_none() || end.is_none())
        {
            return err(
                "contacts:activity sections notes, events, and emails require --start and --end",
            );
        }
        let limit =
            optional_usize_from_matches(matches, "limit")?.unwrap_or(MOMENT_PAGE_SIZE_DEFAULT);
        Ok(Self {
            contact_ids,
            sections,
            start,
            end,
            limit,
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactProfileOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let contact_ids = dedupe_ids(contact_ids_from_matches(matches, "contact-ids")?);
        let include_activity = !matches.get_flag("skip-activity");
        let activity_sections = if include_activity {
            contact_profile_activity_sections_from_matches(matches)?
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
                "contacts:profile activity sections notes, events, and emails require --start and --end",
            );
        }
        Ok(Self {
            contact_ids,
            activity_sections,
            start,
            end,
            activity_limit: optional_usize_from_matches(matches, "activity-limit")?
                .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT),
            group_scan_limit: optional_positive_usize_from_matches(matches, "group-scan-limit")?
                .unwrap_or(PROFILE_GROUP_SCAN_LIMIT_DEFAULT),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            include_groups: !matches.get_flag("skip-groups"),
            include_activity,
            flat: matches.get_flag("flat"),
        })
    }
}

impl ContactGroupsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        Ok(Self {
            contact_ids: dedupe_ids(contact_ids_from_matches(matches, "contact-ids")?),
            group_query: matches
                .get_one::<String>("group-query")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            group_ids: group_audit_selectors_from_matches(matches)?,
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            group_scan_limit: optional_positive_usize_from_matches(matches, "group-scan-limit")?
                .unwrap_or(PROFILE_GROUP_SCAN_LIMIT_DEFAULT),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
            flat: matches.get_flag("flat"),
        })
    }
}

pub(crate) fn contact_activity_sections_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<&'static SnapshotMomentRoute>> {
    moment_sections_from_matches(matches, "sections", ALL_MOMENT_SECTIONS)
}

pub(crate) fn contact_profile_activity_sections_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<&'static SnapshotMomentRoute>> {
    moment_sections_from_matches(
        matches,
        "activity-sections",
        PROFILE_ACTIVITY_DEFAULT_SECTIONS,
    )
}

pub(crate) fn contact_activity_section_label(value: &str, flag: &str) -> Result<&'static str> {
    match normalize_activity_section(value).as_str() {
        "notes" | "note" => Ok("notes"),
        "events" | "event" => Ok("events"),
        "emails" | "email" => Ok("emails"),
        "events_upcoming" | "event_upcoming" | "upcoming_events" | "upcoming_event" => {
            Ok("events_upcoming")
        }
        "emails_recent" | "email_recent" | "recent_emails" | "recent_email" => Ok("emails_recent"),
        "reminders_recent" | "reminder_recent" | "recent_reminders" | "recent_reminder" => {
            Ok("reminders_recent")
        }
        "reminders_upcoming" | "reminder_upcoming" | "upcoming_reminders" | "upcoming_reminder" => {
            Ok("reminders_upcoming")
        }
        other => err(format!(
            "--{flag} must contain all, notes, events, emails, events-upcoming, emails-recent, reminders-recent, or reminders-upcoming; got {other}"
        )),
    }
}

pub(crate) fn normalize_activity_section(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ':', '/'], "_")
}

pub(crate) fn contact_activity_section_labels(
    sections: &[&'static SnapshotMomentRoute],
) -> Vec<&'static str> {
    sections.iter().map(|route| route.label).collect()
}

pub(crate) fn contact_activity_dry_run_plan(options: &ContactActivityOptions) -> Value {
    json!({
        "source": "live",
        "filters": contact_activity_filters(options),
        "contact_count": options.contact_ids.len(),
        "plan": [
            {
                "route": "/tools/v2/get-contact",
                "payload": {"contact_id": "one selected contact ID per request"},
                "concurrency": options.concurrency,
                "purpose": "fetch full contact records without writes"
            },
            {
                "routes": options.sections.iter().map(|route| json!({
                    "label": route.label,
                    "route": format!("/tools/v2{}", route.route),
                    "kind": contact_activity_kind(route.kind),
                    "payload": Value::Object(contact_activity_moment_plan_payload(options, route)),
                    "purpose": "fetch related moment rows without writes",
                })).collect::<Vec<_>>()
            }
        ],
    })
}

pub(crate) async fn contacts_activity(
    runtime: &Runtime,
    options: ContactActivityOptions,
) -> Result<Value> {
    let contacts = fetch_contacts(runtime, &options.contact_ids, options.concurrency)
        .await?
        .into_iter()
        .zip(options.contact_ids.iter().copied())
        .map(|(data, id)| normalize_full_contact(id, data))
        .collect::<Vec<_>>();

    let mut moments = Map::new();
    let mut counts = Map::new();
    let mut total_rows = 0_usize;
    for route in &options.sections {
        let section = contact_activity_fetch_route(runtime, &options, route).await?;
        let count = section
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        total_rows += count as usize;
        counts.insert(route.label.to_string(), Value::Number(Number::from(count)));
        moments.insert(route.label.to_string(), section);
    }

    Ok(json!({
        "source": "live",
        "filters": contact_activity_filters(&options),
        "contact_count": contacts.len(),
        "summary": {
            "section_count": options.sections.len(),
            "activity_count": total_rows,
            "counts": counts,
        },
        "contacts": contacts,
        "moments": moments,
    }))
}

pub(crate) async fn contact_activity_fetch_route(
    runtime: &Runtime,
    options: &ContactActivityOptions,
    route: &SnapshotMomentRoute,
) -> Result<Value> {
    let mut rows = Vec::new();
    let mut pages = 0_usize;

    match route.kind {
        SnapshotMomentKind::DateWindow => {
            let payload = contact_activity_moment_date_payload(options)?;
            let data = runtime
                .call_tool(route.route, Value::Object(payload))
                .await
                .wrap_err_with(|| format!("fetching {}", route.route))?;
            pages = 1;
            rows.extend(snapshot_moment_rows_from_response(route, &data)?);
        }
        SnapshotMomentKind::Paged => {
            let mut page = 1_usize;
            loop {
                let payload = contact_activity_moment_paged_payload(options, page);
                let data = runtime
                    .call_tool(route.route, Value::Object(payload))
                    .await
                    .wrap_err_with(|| format!("fetching {} page {page}", route.route))?;
                pages += 1;
                let page_rows = snapshot_moment_rows_from_response(route, &data)?;
                rows.extend(page_rows.iter().cloned());
                if !moment_response_has_next(&data) {
                    break;
                }
                if page_rows.is_empty() {
                    return err(format!(
                        "{} returned has_next=true with no rows on page {page}",
                        route.route
                    ));
                }
                page += 1;
            }
        }
    }

    Ok(json!({
        "label": route.label,
        "route": route.route,
        "kind": contact_activity_kind(route.kind),
        "pages": pages,
        "count": rows.len(),
        "rows": rows,
    }))
}

pub(crate) fn contact_activity_moment_date_payload(
    options: &ContactActivityOptions,
) -> Result<Map<String, Value>> {
    let start = options
        .start
        .as_ref()
        .ok_or_else(|| miette!("missing --start"))?;
    let end = options
        .end
        .as_ref()
        .ok_or_else(|| miette!("missing --end"))?;
    let mut payload = contact_activity_moment_base_payload(options);
    payload.set("start", start.clone());
    payload.set("end", end.clone());
    Ok(payload)
}

pub(crate) fn contact_activity_moment_paged_payload(
    options: &ContactActivityOptions,
    page: usize,
) -> Map<String, Value> {
    let mut payload = contact_activity_moment_base_payload(options);
    payload.insert(
        "limit".to_string(),
        Value::Number(Number::from(options.limit as u64)),
    );
    payload.set("page", page as u64);
    payload
}

pub(crate) fn contact_activity_moment_plan_payload(
    options: &ContactActivityOptions,
    route: &SnapshotMomentRoute,
) -> Map<String, Value> {
    let mut payload = contact_activity_moment_base_payload(options);
    match route.kind {
        SnapshotMomentKind::DateWindow => {
            payload.insert(
                "start".to_string(),
                Value::String(options.start.clone().unwrap_or_default()),
            );
            payload.insert(
                "end".to_string(),
                Value::String(options.end.clone().unwrap_or_default()),
            );
        }
        SnapshotMomentKind::Paged => {
            payload.insert(
                "limit".to_string(),
                Value::Number(Number::from(options.limit as u64)),
            );
            payload.set("page", "1..has_next".to_string());
        }
    }
    payload
}

pub(crate) fn contact_activity_moment_base_payload(
    options: &ContactActivityOptions,
) -> Map<String, Value> {
    let mut payload = Map::new();
    payload.set("contact_ids", json!(options.contact_ids));
    payload
}

pub(crate) fn contact_activity_filters(options: &ContactActivityOptions) -> Value {
    json!({
        "contact_ids": options.contact_ids.clone(),
        "sections": contact_activity_section_labels(&options.sections),
        "start": options.start.clone(),
        "end": options.end.clone(),
        "limit": options.limit,
    })
}

pub(crate) fn contact_activity_kind(kind: SnapshotMomentKind) -> &'static str {
    match kind {
        SnapshotMomentKind::DateWindow => "date_window",
        SnapshotMomentKind::Paged => "paged",
    }
}

pub(crate) fn contact_profile_dry_run_plan(options: &ContactProfileOptions) -> Value {
    json!({
        "source": "live",
        "filters": contact_profile_filters(options),
        "plan": [
            {
                "route": "/tools/v2/get-contact",
                "payload": {"contact_id": "one selected contact ID per request"},
                "concurrency": options.concurrency,
                "purpose": "fetch full contact records without writes",
            },
            {
                "enabled": options.include_groups,
                "route": "/tools/v2/get-groups",
                "payload": {},
                "purpose": "read live group catalog for membership scan",
            },
            {
                "enabled": options.include_groups,
                "route": "/tools/v2/search",
                "payload": {"group_ids": "one live group per request", "limit": SEARCH_LIMIT_MAX, "exclude_contact_ids": "accumulated from prior pages"},
                "scan_limit_per_group": options.group_scan_limit,
                "concurrency": options.concurrency,
                "purpose": "scan group members and filter selected contact IDs locally",
            },
            {
                "enabled": options.include_activity,
                "routes": options.activity_sections.iter().map(|route| json!({
                    "label": route.label,
                    "route": format!("/tools/v2{}", route.route),
                    "kind": contact_activity_kind(route.kind),
                    "payload": Value::Object(contact_profile_activity_plan_payload(options, route)),
                    "purpose": "fetch selected contacts' moment activity without writes",
                })).collect::<Vec<_>>(),
            },
        ],
    })
}

pub(crate) async fn contacts_profile(
    runtime: &Runtime,
    options: ContactProfileOptions,
) -> Result<Value> {
    let contacts = fetch_contacts(runtime, &options.contact_ids, options.concurrency)
        .await?
        .into_iter()
        .zip(options.contact_ids.iter().copied())
        .map(|(data, id)| normalize_full_contact(id, data))
        .collect::<Vec<_>>();
    let groups = if options.include_groups {
        Some(contact_profile_groups(runtime, &options).await?)
    } else {
        None
    };
    let activity = if options.include_activity {
        Some(
            moments_feed(
                runtime,
                MomentsFeedOptions {
                    contact_ids: options.contact_ids.clone(),
                    sections: options.activity_sections.clone(),
                    start: options.start.clone(),
                    end: options.end.clone(),
                    limit: options.activity_limit,
                    item_limit: None,
                    sort: MomentsFeedSort::Desc,
                    flat: false,
                },
            )
            .await?,
        )
    } else {
        None
    };

    let group_membership_count = groups
        .as_ref()
        .and_then(|groups| groups.get("summary"))
        .and_then(|summary| summary.get("membership_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let group_scan_incomplete = groups
        .as_ref()
        .and_then(|groups| groups.get("summary"))
        .and_then(|summary| summary.get("possibly_incomplete"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let group_error_count = groups
        .as_ref()
        .and_then(|groups| groups.get("errors"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or_default();
    let activity_count = activity
        .as_ref()
        .and_then(|activity| activity.get("summary"))
        .and_then(|summary| summary.get("activity_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let feed_count = activity
        .as_ref()
        .and_then(|activity| activity.get("summary"))
        .and_then(|summary| summary.get("feed_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();

    Ok(json!({
        "source": "live",
        "filters": contact_profile_filters(&options),
        "summary": {
            "contact_count": contacts.len(),
            "group_membership_count": group_membership_count,
            "group_scan_incomplete": group_scan_incomplete,
            "group_error_count": group_error_count,
            "activity_count": activity_count,
            "feed_count": feed_count,
        },
        "contacts": contacts,
        "groups": groups,
        "activity": activity,
    }))
}

pub(crate) async fn contact_profile_groups(
    runtime: &Runtime,
    options: &ContactProfileOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_group_count = groups.len();
    let group_options = GroupMembersOptions {
        query: None,
        group_ids: Vec::new(),
        all_groups: true,
        include_fields: Vec::new(),
        limit_per_group: Some(options.group_scan_limit),
        page_size: SEARCH_LIMIT_MAX,
        concurrency: options.concurrency,
        flat: false,
    };
    let (scanned_groups, errors) =
        group_members_fetch_selected(runtime, &groups, &group_options).await?;
    let target_ids = options.contact_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut memberships = Vec::new();
    let mut by_contact = BTreeMap::<String, Vec<Value>>::new();
    let mut truncated_group_count = 0_usize;
    for group in &scanned_groups {
        let truncated = group
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if truncated {
            truncated_group_count += 1;
        }
        let group_data = group.get("group").cloned().unwrap_or(Value::Null);
        let group_id = group_data.get("id").cloned().unwrap_or(Value::Null);
        let group_name = group_data.get("name").cloned().unwrap_or(Value::Null);
        let selector = group.get("selector").cloned().unwrap_or(Value::Null);
        let total_count = group.get("total_count").cloned().unwrap_or(Value::Null);
        let returned_count = group.get("returned_count").cloned().unwrap_or(Value::Null);
        let members = group
            .get("members")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for member in members {
            let Some(contact_id) = contact_id_from_value(&member) else {
                continue;
            };
            if !target_ids.contains(&contact_id) {
                continue;
            }
            let membership = json!({
                "contact_id": contact_id,
                "group_id": group_id.clone(),
                "group_name": group_name.clone(),
                "group_selector": selector.clone(),
                "group_total": total_count.clone(),
                "group_returned": returned_count.clone(),
                "group_truncated": truncated,
                "member": member,
            });
            by_contact
                .entry(contact_id.to_string())
                .or_default()
                .push(membership.clone());
            memberships.push(membership);
        }
    }
    let possibly_incomplete = truncated_group_count > 0;
    Ok(json!({
        "source": source,
        "scan": {
            "discovered_group_count": discovered_group_count,
            "scanned_group_count": scanned_groups.len(),
            "scan_limit_per_group": options.group_scan_limit,
            "truncated_group_count": truncated_group_count,
        },
        "summary": {
            "membership_count": memberships.len(),
            "possibly_incomplete": possibly_incomplete,
            "error_count": errors.len(),
        },
        "groups_by_contact": by_contact,
        "memberships": memberships,
        "errors": errors,
    }))
}

pub(crate) fn contact_groups_dry_run_plan(options: &ContactGroupsOptions) -> Value {
    json!({
        "source": "live",
        "filters": contact_groups_filters(options),
        "plan": [
            {
                "route": "/tools/v2/get-groups",
                "payload": {},
                "purpose": "read live group catalog without writes",
            },
            {
                "local": "contacts:groups",
                "purpose": "select groups by --group-query and/or --group-ids, defaulting to all live groups",
            },
            {
                "route": "/tools/v2/search",
                "payload": {"group_ids": "one selected group per request", "include_fields": options.include_fields, "limit": SEARCH_LIMIT_MAX, "exclude_contact_ids": "accumulated from prior pages"},
                "scan_limit_per_group": options.group_scan_limit,
                "concurrency": options.concurrency,
                "purpose": "scan group members and keep only selected contact IDs locally",
            },
        ],
    })
}

pub(crate) async fn contacts_groups(
    runtime: &Runtime,
    options: &ContactGroupsOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_group_count = groups.len();
    let mut selected_groups = groups
        .into_iter()
        .filter(|group| contact_groups_group_matches(group, options))
        .collect::<Vec<_>>();
    selected_groups.sort_by(compare_groups_by_name_then_id);
    let selected_group_count = selected_groups.len();
    let group_options = GroupMembersOptions {
        query: None,
        group_ids: Vec::new(),
        all_groups: true,
        include_fields: options.include_fields.clone(),
        limit_per_group: Some(options.group_scan_limit),
        page_size: SEARCH_LIMIT_MAX,
        concurrency: options.concurrency,
        flat: false,
    };
    let (scanned_groups, errors) =
        group_members_fetch_selected(runtime, &selected_groups, &group_options).await?;
    let target_ids = options.contact_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut membership_rows = Vec::new();
    let mut by_contact = BTreeMap::<u64, Vec<Value>>::new();
    let mut contact_names = BTreeMap::<u64, String>::new();
    let mut truncated_group_count = 0_usize;

    for group in &scanned_groups {
        let truncated = group
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if truncated {
            truncated_group_count += 1;
        }
        let group_data = group.get("group").cloned().unwrap_or(Value::Null);
        let group_id = group_data.get("id").cloned().unwrap_or(Value::Null);
        let group_name = group_data.get("name").cloned().unwrap_or(Value::Null);
        let selector = group.get("selector").cloned().unwrap_or(Value::Null);
        let total_count = group.get("total_count").cloned().unwrap_or(Value::Null);
        let returned_count = group.get("returned_count").cloned().unwrap_or(Value::Null);
        let members = group
            .get("members")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for member in members {
            let Some(contact_id) = contact_id_from_value(&member) else {
                continue;
            };
            if !target_ids.contains(&contact_id) {
                continue;
            }
            if let Some(name) = contact_name(&member) {
                contact_names.entry(contact_id).or_insert(name);
            }
            let membership = json!({
                "contact_id": contact_id,
                "group_id": group_id.clone(),
                "group_name": group_name.clone(),
                "group_selector": selector.clone(),
                "group_total": total_count.clone(),
                "group_returned": returned_count.clone(),
                "group_truncated": truncated,
                "member": member,
            });
            by_contact
                .entry(contact_id)
                .or_default()
                .push(membership.clone());
            membership_rows.push(membership);
        }
    }

    let possibly_incomplete = truncated_group_count > 0;
    let contacts = options
        .contact_ids
        .iter()
        .map(|id| {
            let memberships = by_contact.remove(id).unwrap_or_default();
            json!({
                "contact_id": id,
                "contact_name": contact_names.get(id).cloned().unwrap_or_default(),
                "group_count": memberships.len(),
                "possibly_incomplete": possibly_incomplete,
                "groups": memberships,
            })
        })
        .collect::<Vec<_>>();
    let contact_with_groups_count = contacts
        .iter()
        .filter(|contact| {
            contact
                .get("group_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
        })
        .count();

    Ok(json!({
        "source": source,
        "filters": contact_groups_filters(options),
        "scan": {
            "discovered_group_count": discovered_group_count,
            "selected_group_count": selected_group_count,
            "scanned_group_count": scanned_groups.len(),
            "scan_limit_per_group": options.group_scan_limit,
            "truncated_group_count": truncated_group_count,
        },
        "summary": {
            "contact_count": contacts.len(),
            "contact_with_groups_count": contact_with_groups_count,
            "membership_count": membership_rows.len(),
            "possibly_incomplete": possibly_incomplete,
            "error_count": errors.len(),
        },
        "contacts": contacts,
        "memberships": membership_rows,
        "errors": errors,
    }))
}

pub(crate) fn contact_groups_group_matches(group: &Value, options: &ContactGroupsOptions) -> bool {
    if let Some(query) = &options.group_query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    options.group_ids.is_empty()
        || options
            .group_ids
            .iter()
            .any(|selector| selector.matches_group(group))
}

pub(crate) fn contact_groups_filters(options: &ContactGroupsOptions) -> Value {
    json!({
        "contact_ids": options.contact_ids,
        "group_query": options.group_query,
        "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
        "include_fields": options.include_fields,
        "group_scan_limit": options.group_scan_limit,
        "concurrency": options.concurrency,
    })
}

pub(crate) fn contact_groups_flat_rows(report: &Value) -> Value {
    let contacts = report
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let scan = report.get("scan").unwrap_or(&Value::Null);
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let mut rows = Vec::new();
    for contact in contacts {
        let groups = contact
            .get("groups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if groups.is_empty() {
            rows.push(contact_groups_flat_row(&contact, None, scan, summary));
        } else {
            for group in groups {
                rows.push(contact_groups_flat_row(
                    &contact,
                    Some(&group),
                    scan,
                    summary,
                ));
            }
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_groups_flat_row(
    contact: &Value,
    group: Option<&Value>,
    scan: &Value,
    summary: &Value,
) -> Value {
    json!({
        "contact_id": contact.get("contact_id").cloned().unwrap_or(Value::Null),
        "contact_name": contact.get("contact_name").cloned().unwrap_or(Value::Null),
        "contact_group_count": contact.get("group_count").cloned().unwrap_or(Value::Null),
        "contact_possibly_incomplete": contact.get("possibly_incomplete").cloned().unwrap_or(Value::Null),
        "group_id": group.and_then(|value| value.get("group_id")).cloned().unwrap_or(Value::Null),
        "group_name": group.and_then(|value| value.get("group_name")).cloned().unwrap_or(Value::Null),
        "group_selector": group.and_then(|value| value.get("group_selector")).cloned().unwrap_or(Value::Null),
        "group_total": group.and_then(|value| value.get("group_total")).cloned().unwrap_or(Value::Null),
        "group_returned": group.and_then(|value| value.get("group_returned")).cloned().unwrap_or(Value::Null),
        "group_truncated": group.and_then(|value| value.get("group_truncated")).cloned().unwrap_or(Value::Bool(false)),
        "selected_group_count": scan.get("selected_group_count").cloned().unwrap_or(Value::Null),
        "truncated_group_count": scan.get("truncated_group_count").cloned().unwrap_or(Value::Null),
        "membership_count": summary.get("membership_count").cloned().unwrap_or(Value::Null),
        "error_count": summary.get("error_count").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn contact_profile_activity_plan_payload(
    options: &ContactProfileOptions,
    route: &SnapshotMomentRoute,
) -> Map<String, Value> {
    let feed_options = MomentsFeedOptions {
        contact_ids: options.contact_ids.clone(),
        sections: Vec::new(),
        start: options.start.clone(),
        end: options.end.clone(),
        limit: options.activity_limit,
        item_limit: None,
        sort: MomentsFeedSort::Desc,
        flat: false,
    };
    moments_feed_moment_plan_payload(&feed_options, route)
}

pub(crate) fn contact_profile_filters(options: &ContactProfileOptions) -> Value {
    json!({
        "contact_ids": options.contact_ids.clone(),
        "include_groups": options.include_groups,
        "include_activity": options.include_activity,
        "activity_sections": contact_activity_section_labels(&options.activity_sections),
        "start": options.start.clone(),
        "end": options.end.clone(),
        "activity_limit": options.activity_limit,
        "group_scan_limit": options.group_scan_limit,
        "concurrency": options.concurrency,
    })
}

pub(crate) fn contact_profile_flat_rows(report: &Value) -> Value {
    let contacts = report
        .get("contacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let summary = report.get("summary").and_then(Value::as_object);
    let group_scan_incomplete = summary
        .and_then(|summary| summary.get("group_scan_incomplete"))
        .cloned()
        .unwrap_or(Value::Bool(false));
    let group_error_count = summary
        .and_then(|summary| summary.get("group_error_count"))
        .cloned()
        .unwrap_or(Value::Number(Number::from(0)));
    let groups_by_contact = report
        .get("groups")
        .and_then(|groups| groups.get("groups_by_contact"))
        .and_then(Value::as_object);
    let feed = report
        .get("activity")
        .and_then(|activity| activity.get("feed"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut rows = Vec::new();
    for contact in contacts {
        let contact_id = contact_id_from_value(&contact);
        let contact_id_key = contact_id.map(|id| id.to_string());
        let groups = contact_id_key
            .as_ref()
            .and_then(|id| groups_by_contact.and_then(|groups| groups.get(id)))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let group_names = groups
            .iter()
            .filter_map(|group| group.get("group_name"))
            .filter_map(value_string)
            .filter(|name| !name.trim().is_empty())
            .collect::<Vec<_>>();
        let contact_feed = contact_id
            .map(|id| contact_profile_feed_for_contact(&feed, id))
            .unwrap_or_default();
        let latest = contact_feed.first();
        rows.push(json!({
            "contact_id": contact_id.map(|id| Value::Number(Number::from(id))).unwrap_or(Value::Null),
            "contact_name": contact_name(&contact).map(Value::String).unwrap_or(Value::Null),
            "group_count": groups.len(),
            "group_error_count": group_error_count.clone(),
            "group_scan_incomplete": group_scan_incomplete.clone(),
            "groups": group_names.join(", "),
            "activity_count": contact_feed.len(),
            "latest_activity_date": latest.and_then(|row| row.get("date")).cloned().unwrap_or(Value::Null),
            "latest_activity_section": latest.and_then(|row| row.get("section")).cloned().unwrap_or(Value::Null),
            "latest_activity_summary": latest.and_then(|row| row.get("summary")).cloned().unwrap_or(Value::Null),
        }));
    }
    Value::Array(rows)
}

pub(crate) fn contact_profile_feed_for_contact(feed: &[Value], contact_id: u64) -> Vec<Value> {
    feed.iter()
        .filter(|row| {
            row.get("contact_id")
                .and_then(Value::as_u64)
                .is_some_and(|id| id == contact_id)
        })
        .cloned()
        .collect()
}

pub(crate) fn contact_activity_flat_rows(report: &Value) -> Value {
    let contact_names = contact_activity_contact_names(report);
    let Some(moments) = report.get("moments").and_then(Value::as_object) else {
        return Value::Array(Vec::new());
    };
    let mut rows = Vec::new();
    for (section, data) in moments {
        let route = data.get("route").cloned().unwrap_or(Value::Null);
        let section_count = data.get("count").cloned().unwrap_or(Value::Null);
        let pages = data.get("pages").cloned().unwrap_or(Value::Null);
        let items = data
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            rows.push(json!({
                "section": section,
                "route": route,
                "section_count": section_count,
                "pages": pages,
                "activity_id": Value::Null,
                "contact_id": Value::Null,
                "contact_name": Value::Null,
                "date": Value::Null,
                "title": Value::Null,
                "summary": Value::Null,
            }));
            continue;
        }
        for item in items {
            let contact_id = contact_activity_row_contact_id(&item);
            let contact_name = contact_activity_row_contact_name(&item)
                .or_else(|| contact_id.and_then(|id| contact_names.get(&id.to_string()).cloned()));
            rows.push(json!({
                "section": section,
                "route": route.clone(),
                "section_count": section_count.clone(),
                "pages": pages.clone(),
                "activity_id": record_id(&item).map(Value::String).unwrap_or(Value::Null),
                "contact_id": contact_id.map(|id| Value::Number(Number::from(id))).unwrap_or(Value::Null),
                "contact_name": contact_name.map(Value::String).unwrap_or(Value::Null),
                "date": contact_activity_row_date(&item).map(Value::String).unwrap_or(Value::Null),
                "title": contact_activity_row_title(&item).map(Value::String).unwrap_or(Value::Null),
                "summary": contact_activity_row_summary(&item).map(Value::String).unwrap_or(Value::Null),
            }));
        }
    }
    Value::Array(rows)
}

pub(crate) fn contact_activity_contact_names(report: &Value) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    if let Some(contacts) = report.get("contacts").and_then(Value::as_array) {
        for contact in contacts {
            if let (Some(id), Some(name)) = (record_id(contact), contact_name(contact)) {
                names.insert(id, name);
            }
        }
    }
    names
}

pub(crate) fn contact_activity_row_contact_id(row: &Value) -> Option<u64> {
    let object = row.as_object()?;
    for aliases in [
        &[
            "contact_id",
            "contactId",
            "person_id",
            "personId",
            "contact",
        ][..],
        &["user_id", "userId"][..],
    ] {
        if let Some(value) = row_value(object, aliases)
            && let Some(id) = contact_activity_value_id(value)
        {
            return Some(id);
        }
    }
    for key in ["contact", "person", "recipient", "sender"] {
        if let Some(value) = object.get(key)
            && let Some(id) = contact_activity_value_id(value)
        {
            return Some(id);
        }
    }
    for key in ["contacts", "people", "participants"] {
        if let Some(Value::Array(values)) = object.get(key) {
            for value in values {
                if let Some(id) = contact_activity_value_id(value) {
                    return Some(id);
                }
            }
        }
    }
    for key in ["note", "event", "email", "reminder", "activity"] {
        if let Some(value) = object.get(key)
            && let Some(id) = contact_activity_nested_contact_id(value)
        {
            return Some(id);
        }
    }
    None
}

pub(crate) fn contact_activity_value_id(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => parse_contact_id(text).ok(),
        Value::Object(_) => contact_id_from_value(value),
        _ => None,
    }
}

pub(crate) fn contact_activity_nested_contact_id(value: &Value) -> Option<u64> {
    let object = value.as_object()?;
    for key in ["contact", "person", "recipient", "sender"] {
        if let Some(value) = object.get(key)
            && let Some(id) = contact_activity_value_id(value)
        {
            return Some(id);
        }
    }
    for key in ["contacts", "people", "participants"] {
        if let Some(Value::Array(values)) = object.get(key) {
            for value in values {
                if let Some(id) = contact_activity_value_id(value) {
                    return Some(id);
                }
            }
        }
    }
    for key in ["note", "event", "email", "reminder", "activity"] {
        if let Some(value) = object.get(key)
            && let Some(id) = contact_activity_nested_contact_id(value)
        {
            return Some(id);
        }
    }
    None
}

pub(crate) fn contact_activity_row_contact_name(row: &Value) -> Option<String> {
    let object = row.as_object()?;
    row_string(
        object,
        &["contact_name", "contactName", "person_name", "personName"],
    )
    .or_else(|| {
        ["contact", "person", "recipient", "sender"]
            .into_iter()
            .filter_map(|key| object.get(key))
            .filter_map(contact_name)
            .find(|name| !name.trim().is_empty())
    })
    .or_else(|| contact_activity_nested_contact_name(row))
}

pub(crate) fn contact_activity_row_date(row: &Value) -> Option<String> {
    contact_activity_row_string(
        row,
        &[
            "date",
            "start",
            "start_at",
            "startAt",
            "start_time",
            "startTime",
            "due_date",
            "dueDate",
            "reminder_date",
            "reminderDate",
            "sent_at",
            "sentAt",
            "created_at",
            "createdAt",
            "timestamp",
        ],
    )
}

pub(crate) fn contact_activity_row_title(row: &Value) -> Option<String> {
    contact_activity_row_string(
        row,
        &["title", "subject", "name", "headline", "summary", "content"],
    )
    .map(|value| truncate_chars(&single_line(&value), 120))
}

pub(crate) fn contact_activity_row_summary(row: &Value) -> Option<String> {
    contact_activity_row_string(
        row,
        &[
            "summary",
            "content",
            "body",
            "text",
            "description",
            "snippet",
            "subject",
            "title",
        ],
    )
    .map(|value| truncate_chars(&single_line(&value), 240))
}

pub(crate) fn contact_activity_row_string(row: &Value, aliases: &[&str]) -> Option<String> {
    let object = row.as_object()?;
    row_string(object, aliases)
        .or_else(|| contact_activity_nested_row_string(row, aliases))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn contact_activity_nested_contact_name(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    for key in ["contact", "person", "recipient", "sender"] {
        if let Some(value) = object.get(key)
            && let Some(name) = contact_name(value)
            && !name.trim().is_empty()
        {
            return Some(name);
        }
    }
    for key in ["contacts", "people", "participants"] {
        if let Some(Value::Array(values)) = object.get(key) {
            for value in values {
                if let Some(name) = contact_name(value)
                    && !name.trim().is_empty()
                {
                    return Some(name);
                }
            }
        }
    }
    for key in ["note", "event", "email", "reminder", "activity"] {
        if let Some(value) = object.get(key)
            && let Some(name) = contact_activity_nested_contact_name(value)
        {
            return Some(name);
        }
    }
    None
}

pub(crate) fn contact_activity_nested_row_string(
    value: &Value,
    aliases: &[&str],
) -> Option<String> {
    let object = value.as_object()?;
    for key in ["note", "event", "email", "reminder", "activity"] {
        if let Some(value) = object.get(key)
            && let Some(text) = contact_activity_value_string(value, aliases)
        {
            return Some(text);
        }
    }
    None
}

pub(crate) fn contact_activity_value_string(value: &Value, aliases: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    row_string(object, aliases).or_else(|| contact_activity_nested_row_string(value, aliases))
}

pub(crate) fn contacts_resolve_has_search_filter(payload: &Map<String, Value>) -> bool {
    payload.iter().any(|(key, value)| match key.as_str() {
        "include_fields" | "exclude_contact_ids" | "sort" => false,
        _ => !contacts_resolve_filter_value_empty(value),
    })
}

pub(crate) fn contacts_resolve_filter_value_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(items) => items.is_empty(),
        Value::Object(object) => object.values().all(contacts_resolve_filter_value_empty),
        Value::String(value) => value.trim().is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

pub(crate) fn contacts_resolve_dry_run(options: &ContactResolveOptions) -> Value {
    let mut count_payload = options.payload.clone();
    count_payload.set("limit", 0);
    json!({
        "source": "live",
        "filters": options.payload.clone(),
        "candidate_limit": options.candidate_limit,
        "one": options.one,
        "all": options.all,
        "plan": [
            {
                "route": "/tools/v2/search",
                "payload": count_payload,
                "purpose": "count matching contacts; --one stops here unless the count is exactly one"
            },
            {
                "route": "/tools/v2/search",
                "payload": "same filters with limit set to candidate_limit and exclude_contact_ids accumulated from prior pages",
                "page_size": options.candidate_limit.min(SEARCH_LIMIT_MAX),
                "purpose": "fetch bounded candidate rows without writes"
            },
            {
                "local": "contacts:resolve",
                "purpose": "classify zero, single, or ambiguous matches and emit candidate IDs"
            }
        ],
    })
}

pub(crate) async fn contacts_resolve(
    runtime: &Runtime,
    options: &ContactResolveOptions,
) -> Result<Value> {
    if options.one {
        let total = runtime.search_total(options.payload.clone()).await?;
        if total != 1 {
            return err(format!("contacts:resolve --one found {total} matches"));
        }
    }

    let mut candidates = Vec::new();
    let page_size = options.candidate_limit.min(SEARCH_LIMIT_MAX);
    let (fetched, total) = export_contacts_each_limited(
        runtime,
        options.payload.clone(),
        page_size,
        Some(options.candidate_limit),
        |row| {
            candidates.push(contact_resolve_candidate(row));
            Ok(())
        },
    )
    .await?;

    if options.one && total != 1 {
        return err(format!(
            "contacts:resolve --one expected exactly one match, found {total}; fetched {fetched} candidates"
        ));
    }

    let ids = candidates
        .iter()
        .filter_map(|candidate| candidate.get("id").and_then(Value::as_u64))
        .map(|id| Value::Number(Number::from(id)))
        .collect::<Vec<_>>();
    let resolved_id = if total == 1 {
        ids.first().cloned().unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let status = if total == 0 {
        "none"
    } else if total == 1 {
        "resolved"
    } else {
        "ambiguous"
    };
    Ok(json!({
        "source": "live",
        "filters": options.payload.clone(),
        "summary": {
            "status": status,
            "total_matches": total,
            "candidate_count": fetched,
            "candidate_limit": options.candidate_limit,
            "truncated": fetched < total,
            "resolved_id": resolved_id,
            "ids": ids,
        },
        "candidates": candidates,
    }))
}

pub(crate) fn contact_resolve_candidate(contact: Value) -> Value {
    let id = contact_id_from_value(&contact)
        .map(|id| Value::Number(Number::from(id)))
        .unwrap_or(Value::Null);
    let name = contact_name(&contact)
        .map(Value::String)
        .unwrap_or(Value::Null);
    let url = contact.get("url").cloned().unwrap_or(Value::Null);
    let score = contact.get("score").cloned().unwrap_or(Value::Null);
    let summary = dedupe_contact_summary(&contact);
    json!({
        "id": id,
        "name": name,
        "url": url,
        "score": score,
        "summary": summary,
        "raw": contact,
    })
}

pub(crate) fn contacts_resolve_rows(report: &Value) -> Value {
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let candidates = report
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if candidates.is_empty() {
        return Value::Array(vec![contacts_resolve_row(summary, None)]);
    }
    Value::Array(
        candidates
            .iter()
            .map(|candidate| contacts_resolve_row(summary, Some(candidate)))
            .collect(),
    )
}

pub(crate) fn contacts_resolve_row(summary: &Value, candidate: Option<&Value>) -> Value {
    json!({
        "status": summary.get("status").cloned().unwrap_or(Value::Null),
        "total_matches": summary.get("total_matches").cloned().unwrap_or(Value::Null),
        "candidate_count": summary.get("candidate_count").cloned().unwrap_or(Value::Null),
        "candidate_limit": summary.get("candidate_limit").cloned().unwrap_or(Value::Null),
        "truncated": summary.get("truncated").cloned().unwrap_or(Value::Null),
        "resolved_id": summary.get("resolved_id").cloned().unwrap_or(Value::Null),
        "id": candidate.and_then(|value| value.get("id")).cloned().unwrap_or(Value::Null),
        "name": candidate.and_then(|value| value.get("name")).cloned().unwrap_or(Value::Null),
        "url": candidate.and_then(|value| value.get("url")).cloned().unwrap_or(Value::Null),
        "score": candidate.and_then(|value| value.get("score")).cloned().unwrap_or(Value::Null),
        "summary": candidate.and_then(|value| value.get("summary")).cloned().unwrap_or(Value::Null),
    })
}

pub(crate) async fn contact_reconnect_activity(
    runtime: &Runtime,
    contact_ids: &[u64],
    options: &ContactReconnectOptions,
) -> Result<Value> {
    let mut feed = Vec::new();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut activity_count = 0_u64;
    let mut chunk_count = 0_usize;

    for chunk in contact_ids.chunks(CONTACT_RECONNECT_ACTIVITY_CHUNK_SIZE) {
        chunk_count += 1;
        let report = moments_feed(
            runtime,
            MomentsFeedOptions {
                contact_ids: chunk.to_vec(),
                sections: options.activity_sections.clone(),
                start: options.start.clone(),
                end: options.end.clone(),
                limit: options.activity_limit,
                item_limit: None,
                sort: MomentsFeedSort::Desc,
                flat: false,
            },
        )
        .await?;
        activity_count += report
            .get("summary")
            .and_then(|summary| summary.get("activity_count"))
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if let Some(section_counts) = report
            .get("summary")
            .and_then(|summary| summary.get("counts"))
            .and_then(Value::as_object)
        {
            for (section, count) in section_counts {
                *counts.entry(section.clone()).or_default() += count.as_u64().unwrap_or_default();
            }
        }
        feed.extend(
            report
                .get("feed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .cloned(),
        );
    }
    moments_feed_sort_rows(&mut feed, MomentsFeedSort::Desc);

    Ok(json!({
        "source": "live",
        "summary": {
            "contact_id_count": contact_ids.len(),
            "chunk_count": chunk_count,
            "chunk_size": CONTACT_RECONNECT_ACTIVITY_CHUNK_SIZE,
            "section_count": options.activity_sections.len(),
            "activity_count": activity_count,
            "feed_count": feed.len(),
            "counts": counts,
        },
        "feed": feed,
    }))
}

pub(crate) fn contact_reconnect_activity_contact_ids(contacts: &[Value]) -> Vec<u64> {
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for contact in contacts {
        if let Some(id) = contact_id_from_value(contact)
            && seen.insert(id)
        {
            ids.push(id);
        }
    }
    ids
}

pub(crate) fn unwrap_rows_for_export(data: Value) -> Value {
    if let Value::Object(object) = &data {
        for key in ["results", "contacts", "items", "data"] {
            if let Some(Value::Array(items)) = object.get(key) {
                return Value::Array(items.clone());
            }
        }
    }
    data
}

pub(crate) async fn export_all_contacts(
    runtime: &Runtime,
    payload: Map<String, Value>,
    page_size: usize,
) -> Result<Value> {
    let mut exported = Vec::new();
    export_all_contacts_each(runtime, payload, page_size, |row| {
        exported.push(row);
        Ok(())
    })
    .await?;
    Ok(Value::Array(exported))
}

pub(crate) async fn bulk_get_contacts_json_array(
    runtime: &Runtime,
    matches: &ArgMatches,
    ids: &[u64],
    concurrency: usize,
    pretty: bool,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result =
            write_bulk_contacts_json_array(runtime, ids, concurrency, pretty, &mut file)
                .await
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
        write_bulk_contacts_json_array(runtime, ids, concurrency, pretty, &mut stdout).await?;
        stdout
            .flush()
            .into_diagnostic()
            .wrap_err("flushing stdout")?;
        Ok(())
    }
}

pub(crate) async fn bulk_get_contacts_jsonl(
    runtime: &Runtime,
    matches: &ArgMatches,
    ids: &[u64],
    concurrency: usize,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result = fetch_contacts_each(runtime, ids, concurrency, |_, row| {
            write_jsonl_row(&mut file, &row)
        })
        .await
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
        fetch_contacts_each(runtime, ids, concurrency, |_, row| {
            write_jsonl_row(&mut stdout, &row)
        })
        .await?;
        stdout
            .flush()
            .into_diagnostic()
            .wrap_err("flushing stdout")?;
        Ok(())
    }
}

pub(crate) async fn bulk_get_contacts_delimited(
    runtime: &Runtime,
    matches: &ArgMatches,
    ids: &[u64],
    concurrency: usize,
    delimiter: u8,
) -> Result<()> {
    let output_path = matches.get_one::<String>("output").map(Path::new);
    let (spool_path, mut spool_file) = create_export_spool(output_path)?;
    let mut headers = BTreeSet::new();
    let export_result = fetch_contacts_each(runtime, ids, concurrency, |_, row| {
        collect_row_headers(&row, &mut headers)?;
        write_jsonl_row(&mut spool_file, &row)
    })
    .await
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

pub(crate) async fn export_all_contacts_jsonl(
    runtime: &Runtime,
    matches: &ArgMatches,
    mut payload: Map<String, Value>,
    page_size: usize,
    resume: bool,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let state_path = export_state_path(output_path);
        let resume_ids = if resume {
            prepare_export_resume(output_path, &state_path, &payload, page_size)?;
            resume_contact_ids_from_jsonl(output_path)?
        } else {
            write_export_state(&state_path, &payload, page_size)?;
            Vec::new()
        };
        append_exclude_contact_ids(&mut payload, &resume_ids)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resume)
            .truncate(!resume)
            .open(path)
            .into_diagnostic()
            .wrap_err_with(|| {
                if resume {
                    format!("opening {path} for append")
                } else {
                    format!("creating {path}")
                }
            })?;
        export_all_contacts_each(runtime, payload, page_size, |row| {
            write_jsonl_row(&mut file, &row)
        })
        .await?;
        file.flush()
            .into_diagnostic()
            .wrap_err_with(|| format!("flushing {path}"))?;
    } else {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        export_all_contacts_each(runtime, payload, page_size, |row| {
            write_jsonl_row(&mut stdout, &row)
        })
        .await?;
        stdout
            .flush()
            .into_diagnostic()
            .wrap_err("flushing stdout")?;
    }
    Ok(())
}

pub(crate) async fn export_all_contacts_json_array(
    runtime: &Runtime,
    matches: &ArgMatches,
    payload: Map<String, Value>,
    page_size: usize,
    pretty: bool,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result =
            write_json_array_contacts(runtime, payload, page_size, pretty, &mut file)
                .await
                .and_then(|_| {
                    file.flush()
                        .into_diagnostic()
                        .wrap_err_with(|| format!("flushing {}", temp_path.display()))
                });
        if let Err(error) = write_result {
            cleanup_export_spool_best_effort(&temp_path);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temp_path, output_path) {
            cleanup_export_spool_best_effort(&temp_path);
            return Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("moving {} to {path}", temp_path.display()));
        }
    } else {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        write_json_array_contacts(runtime, payload, page_size, pretty, &mut stdout).await?;
        stdout
            .flush()
            .into_diagnostic()
            .wrap_err("flushing stdout")?;
    }
    Ok(())
}

pub(crate) async fn export_all_contacts_delimited(
    runtime: &Runtime,
    matches: &ArgMatches,
    payload: Map<String, Value>,
    page_size: usize,
    delimiter: u8,
) -> Result<()> {
    let output_path = matches.get_one::<String>("output").map(Path::new);
    let (spool_path, mut spool_file) = create_export_spool(output_path)?;
    let mut headers = BTreeSet::new();
    let export_result = export_all_contacts_each(runtime, payload, page_size, |row| {
        collect_row_headers(&row, &mut headers)?;
        write_jsonl_row(&mut spool_file, &row)
    })
    .await
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

pub(crate) fn create_export_spool(output_path: Option<&Path>) -> Result<(PathBuf, fs::File)> {
    for attempt in 0..100 {
        let path = export_spool_path(output_path, attempt);
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("creating {}", path.display()));
            }
        }
    }
    err("could not create a unique export spool file")
}

pub(crate) fn export_spool_path(output_path: Option<&Path>, attempt: u32) -> PathBuf {
    let stamp = unix_millis();
    let pid = std::process::id();
    let file_name = output_path
        .and_then(Path::file_name)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "stdout".to_string());
    let spool_name = format!(".{file_name}.meshx-spool-{pid}-{stamp}-{attempt}.jsonl");
    if let Some(parent) = output_path
        .and_then(Path::parent)
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        parent.join(spool_name)
    } else if output_path.is_some() {
        PathBuf::from(spool_name)
    } else {
        std::env::temp_dir().join(spool_name)
    }
}

pub(crate) fn write_delimited_spool(
    matches: &ArgMatches,
    spool_path: &Path,
    headers: &[String],
    delimiter: u8,
) -> Result<()> {
    if let Some(path) = matches.get_one::<String>("output") {
        let output_path = Path::new(path);
        let (temp_path, mut file) = create_export_spool(Some(output_path))?;
        let write_result = write_delimited_from_jsonl(spool_path, headers, delimiter, &mut file)
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
        write_delimited_from_jsonl(spool_path, headers, delimiter, &mut stdout)
    }
}

pub(crate) fn cleanup_export_spool(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error)
            .into_diagnostic()
            .wrap_err_with(|| format!("removing {}", path.display())),
    }
}

pub(crate) fn cleanup_export_spool_best_effort(path: &Path) {
    if let Err(error) = cleanup_export_spool(path) {
        warn!(
            "failed to remove export spool {}: {error:?}",
            path.display()
        );
    }
}

pub(crate) fn prepare_export_resume(
    output_path: &Path,
    state_path: &Path,
    payload: &Map<String, Value>,
    page_size: usize,
) -> Result<()> {
    if state_path.exists() {
        verify_export_state(state_path, payload)?;
        return Ok(());
    }
    let existing_bytes = match fs::metadata(output_path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {}", output_path.display()));
        }
    };
    if existing_bytes != 0 {
        return err(format!(
            "{} is not empty but {} is missing; refusing to resume without filter state",
            output_path.display(),
            state_path.display()
        ));
    }
    write_export_state(state_path, payload, page_size)
}

pub(crate) fn write_export_state(
    path: &Path,
    payload: &Map<String, Value>,
    page_size: usize,
) -> Result<()> {
    let state = export_state_value(payload, page_size)?;
    let text = serde_json::to_string_pretty(&state)
        .into_diagnostic()
        .wrap_err("serializing export state")?;
    fs::write(path, text)
        .into_diagnostic()
        .wrap_err_with(|| format!("writing {}", path.display()))
}

pub(crate) fn verify_export_state(path: &Path, payload: &Map<String, Value>) -> Result<()> {
    let text = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading {}", path.display()))?;
    let state: Value = serde_json::from_str(&text)
        .into_diagnostic()
        .wrap_err_with(|| format!("parsing {}", path.display()))?;
    let expected_hash = export_filters_hash(payload)?;
    let actual_hash = state
        .get("filters_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("{} is missing filters_hash", path.display()))?;
    let schema = state.get("schema").and_then(Value::as_str);
    let command = state.get("command").and_then(Value::as_str);
    let format = state.get("format").and_then(Value::as_str);
    let pagination = state.get("pagination").and_then(Value::as_str);
    if schema != Some("meshx.export-state.v1")
        || command != Some("contacts:export")
        || format != Some("jsonl")
        || pagination != Some("exclude_contact_ids")
    {
        return err(format!(
            "{} is not a contacts:export JSONL state file",
            path.display()
        ));
    }
    if actual_hash != expected_hash {
        return err(format!(
            "{} was created with different export filters; rerun without --resume or use the original filters",
            path.display()
        ));
    }
    Ok(())
}

pub(crate) fn export_state_value(payload: &Map<String, Value>, page_size: usize) -> Result<Value> {
    let filters = export_filter_payload(payload);
    Ok(json!({
        "schema": "meshx.export-state.v1",
        "command": "contacts:export",
        "format": "jsonl",
        "pagination": "exclude_contact_ids",
        "page_size": page_size,
        "filters_hash": export_filters_hash(&filters)?,
        "filters": Value::Object(filters),
    }))
}

pub(crate) fn export_filters_hash(payload: &Map<String, Value>) -> Result<String> {
    record_hash(&canonical_json_value(&Value::Object(
        export_filter_payload(payload),
    )))
}

pub(crate) fn export_filter_payload(payload: &Map<String, Value>) -> Map<String, Value> {
    let mut filters = payload.clone();
    filters.remove("limit");
    filters
}

pub(crate) fn export_state_path(output_path: &Path) -> PathBuf {
    let file_name = output_path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "meshx-export".to_string());
    output_path.with_file_name(format!("{file_name}.meshx-state.json"))
}

pub(crate) fn validate_export_resume_args(
    matches: &ArgMatches,
    format: OutputFormat,
) -> Result<()> {
    if !matches.get_flag("all") {
        return err("--resume requires --all");
    }
    if format != OutputFormat::Jsonl {
        return err("--resume requires --format jsonl");
    }
    if matches.get_one::<String>("output").is_none() {
        return err("--resume requires --output so existing rows can be scanned safely");
    }
    Ok(())
}

pub(crate) fn resume_contact_ids_from_jsonl(path: &Path) -> Result<Vec<u64>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .into_diagnostic()
                .wrap_err_with(|| format!("opening {} for resume", path.display()));
        }
    };
    let reader = StdBufReader::new(file);
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {} line {}", path.display(), index + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(trimmed)
            .into_diagnostic()
            .wrap_err_with(|| format!("{} line {} is not valid JSON", path.display(), index + 1))?;
        let id = contact_id_from_value(&value).ok_or_else(|| {
            miette!(
                "{} line {} does not contain a numeric id",
                path.display(),
                index + 1
            )
        })?;
        if !seen.insert(id) {
            return err(format!(
                "{} line {} repeats contact id {id}; refusing to resume a duplicate output file",
                path.display(),
                index + 1
            ));
        }
        ids.push(id);
    }
    Ok(ids)
}

pub(crate) async fn export_all_contacts_each<F>(
    runtime: &Runtime,
    payload: Map<String, Value>,
    page_size: usize,
    on_contact: F,
) -> Result<usize>
where
    F: FnMut(Value) -> Result<()>,
{
    let (exported, _) =
        export_contacts_each_limited(runtime, payload, page_size, None, on_contact).await?;
    Ok(exported)
}

pub(crate) async fn export_contacts_each_limited<F>(
    runtime: &Runtime,
    mut payload: Map<String, Value>,
    page_size: usize,
    limit: Option<usize>,
    mut on_contact: F,
) -> Result<(usize, usize)>
where
    F: FnMut(Value) -> Result<()>,
{
    if page_size == 0 || page_size > SEARCH_LIMIT_MAX {
        return err(format!(
            "--page-size must be between 1 and {SEARCH_LIMIT_MAX}"
        ));
    }

    payload.remove("limit");
    let total = runtime.search_total(payload.clone()).await?;
    let target = limit.map(|limit| limit.min(total)).unwrap_or(total);
    if target == 0 {
        return Ok((0, total));
    }

    let mut excluded_ids = exclude_contact_ids_from_payload(payload.get("exclude_contact_ids"))?;
    let mut excluded = excluded_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut exported = 0_usize;

    while exported < target {
        let remaining = target.saturating_sub(exported);
        let page_limit = page_size.min(remaining);
        let mut page_payload = payload.clone();
        page_payload.insert(
            "limit".to_string(),
            Value::Number(Number::from(page_limit as u64)),
        );
        if !excluded_ids.is_empty() {
            page_payload.insert(
                "exclude_contact_ids".to_string(),
                Value::Array(
                    excluded_ids
                        .iter()
                        .copied()
                        .map(|id| Value::Number(Number::from(id)))
                        .collect(),
                ),
            );
        }

        let data = runtime
            .call_tool(route::SEARCH, Value::Object(page_payload))
            .await?;
        let rows = rows_from_value(&data)
            .into_iter()
            .map(Value::Object)
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return err(format!(
                "me.sh search reported {total} matches, but returned no rows after exporting {} unique contacts",
                exported
            ));
        }

        let before = exported;
        for row in rows {
            let id = contact_id_from_value(&row)
                .ok_or_else(|| miette!("me.sh search row did not include numeric id"))?;
            if excluded.insert(id) {
                excluded_ids.push(id);
                on_contact(row)?;
                exported += 1;
                if exported == target {
                    break;
                }
            }
        }

        if exported == before {
            return err(format!(
                "me.sh search returned no new unique contact IDs after exporting {} of {total} contacts",
                exported
            ));
        }
    }

    Ok((exported, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(label: &str) -> &'static SnapshotMomentRoute {
        snapshot_moment_route_by_label(label).expect("test route exists")
    }

    fn activity_options(contact_ids: Vec<u64>) -> ContactActivityOptions {
        ContactActivityOptions {
            contact_ids,
            sections: vec![route("notes"), route("events_upcoming")],
            start: Some("2024-02-01".to_string()),
            end: Some("2024-02-29".to_string()),
            limit: 50,
            concurrency: 4,
            flat: false,
        }
    }

    #[test]
    fn contact_activity_base_payload_always_includes_contact_ids() {
        assert_eq!(
            Value::Object(contact_activity_moment_base_payload(&activity_options(
                vec![42, 7]
            ))),
            json!({"contact_ids": [42, 7]})
        );
        assert_eq!(
            Value::Object(contact_activity_moment_base_payload(&activity_options(
                Vec::new()
            ))),
            json!({"contact_ids": []})
        );
    }

    #[test]
    fn contact_activity_moment_payloads_add_window_or_paging_fields() -> Result<()> {
        let options = activity_options(vec![42]);

        assert_eq!(
            Value::Object(contact_activity_moment_date_payload(&options)?),
            json!({
                "contact_ids": [42],
                "start": "2024-02-01",
                "end": "2024-02-29",
            })
        );
        assert_eq!(
            Value::Object(contact_activity_moment_paged_payload(&options, 3)),
            json!({
                "contact_ids": [42],
                "limit": 50,
                "page": 3,
            })
        );
        assert_eq!(
            Value::Object(contact_activity_moment_plan_payload(
                &options,
                route("events_upcoming")
            )),
            json!({
                "contact_ids": [42],
                "limit": 50,
                "page": "1..has_next",
            })
        );
        Ok(())
    }
}
