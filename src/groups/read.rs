use crate::prelude::*;

#[derive(Clone, Debug)]
pub(crate) struct GroupProfileOptions {
    pub(crate) query: Option<String>,
    pub(crate) group_ids: Vec<GroupAuditSelector>,
    pub(crate) all: bool,
    pub(crate) one: bool,
    pub(crate) member_limit: Option<usize>,
    pub(crate) include_fields: Vec<String>,
    pub(crate) page_size: usize,
    pub(crate) concurrency: usize,
}

impl GroupProfileOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let query = matches
            .get_one::<String>("query")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let group_ids = group_audit_selectors_from_matches(matches)?;
        let all = matches.get_flag("all");
        if !all && query.is_none() && group_ids.is_empty() {
            return err("groups:profile requires --query, --group-ids, or --all");
        }
        let all_members = matches.get_flag("all-members");
        let member_limit = optional_nonnegative_usize_from_matches(matches, "member-limit")?;
        if all_members && member_limit.is_some() {
            return err("groups:profile accepts --all-members or --member-limit, not both");
        }
        Ok(Self {
            query,
            group_ids,
            all,
            one: matches.get_flag("one"),
            member_limit: if all_members {
                None
            } else {
                Some(member_limit.unwrap_or(GROUP_PROFILE_MEMBER_LIMIT_DEFAULT))
            },
            include_fields: include_fields_from_matches(matches, "include-fields")?,
            page_size: optional_usize_from_matches(matches, "page-size")?
                .unwrap_or(SEARCH_LIMIT_MAX),
            concurrency: contact_fetch_concurrency(matches, "concurrency")?,
        })
    }
}

pub(crate) fn group_audit_selectors_from_matches(
    matches: &ArgMatches,
) -> Result<Vec<GroupAuditSelector>> {
    group_audit_selectors_from_matches_flag(matches, "group-ids")
}

pub(crate) fn group_audit_selectors_from_matches_flag(
    matches: &ArgMatches,
    flag: &str,
) -> Result<Vec<GroupAuditSelector>> {
    let raw = split_list_values(&collect_values(matches, flag));
    let mut seen = BTreeSet::new();
    let mut selectors = Vec::new();
    for value in raw {
        let selector = GroupAuditSelector::parse_with_flag(&value, &format!("--{flag}"))?;
        let key = match selector {
            GroupAuditSelector::Id(id) => format!("id:{id}"),
            GroupAuditSelector::Starred => "starred".to_string(),
        };
        if seen.insert(key) {
            selectors.push(selector);
        }
    }
    Ok(selectors)
}

pub(crate) fn group_resolve_dry_run_plan(options: &GroupResolveOptions) -> Value {
    let mut plan = vec![
        json!({"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"}),
        json!({"local": "groups:resolve", "purpose": "select group candidates by --query, --group-ids, or explicit --all"}),
    ];
    if options.member_counts {
        plan.push(json!({
            "route": "/tools/v2/search",
            "payload": {"group_ids": "one returned candidate group per request", "limit": 0},
            "concurrency": options.concurrency,
            "purpose": "count live members for returned candidates without writes",
        }));
    }
    json!({
        "source": "live",
        "filters": {
            "query": options.query,
            "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "all": options.all,
        },
        "candidate_limit": options.candidate_limit,
        "one": options.one,
        "member_counts": options.member_counts,
        "plan": plan,
    })
}

pub(crate) async fn groups_resolve(
    runtime: &Runtime,
    options: &GroupResolveOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_count = groups.len();
    let mut selected = groups
        .into_iter()
        .filter(|group| group_resolve_matches(group, options))
        .collect::<Vec<_>>();
    selected.sort_by(compare_groups_by_name_then_id);

    let total = selected.len();
    if options.one && total != 1 {
        return err(format!("groups:resolve --one found {total} matches"));
    }

    let truncated = total > options.candidate_limit;
    selected.truncate(options.candidate_limit);
    let (member_counts, member_count_errors) = if options.member_counts {
        group_audit_live_member_counts(runtime, &selected, options.concurrency).await?
    } else {
        (BTreeMap::new(), Vec::new())
    };
    let candidates = selected
        .into_iter()
        .map(|group| group_resolve_candidate(group, &member_counts))
        .collect::<Vec<_>>();
    let ids = candidates
        .iter()
        .filter_map(|candidate| candidate.get("id").cloned())
        .filter(|value| !value.is_null())
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
        "source": source,
        "filters": {
            "query": options.query,
            "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "all": options.all,
        },
        "summary": {
            "status": status,
            "discovered_count": discovered_count,
            "total_matches": total,
            "candidate_count": candidates.len(),
            "candidate_limit": options.candidate_limit,
            "truncated": truncated,
            "resolved_id": resolved_id,
            "ids": ids,
            "member_counts": {
                "enabled": options.member_counts,
                "counted": member_counts.len(),
                "error_count": member_count_errors.len(),
                "errors": member_count_errors,
            },
        },
        "candidates": candidates,
    }))
}

pub(crate) fn group_resolve_matches(group: &Value, options: &GroupResolveOptions) -> bool {
    if let Some(query) = &options.query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    if options.group_ids.is_empty() {
        return options.all || options.query.is_some();
    }
    options
        .group_ids
        .iter()
        .any(|selector| selector.matches_group(group))
}

pub(crate) fn group_resolve_candidate(
    group: Value,
    member_counts: &BTreeMap<String, u64>,
) -> Value {
    let id_text = record_id(&group).unwrap_or_default();
    let id = parse_contact_id(&id_text)
        .map(|id| Value::Number(Number::from(id)))
        .unwrap_or_else(|_| {
            if id_text.is_empty() {
                Value::Null
            } else {
                Value::String(id_text.clone())
            }
        });
    let name = group_name(&group).unwrap_or_default();
    let selector = group_audit_member_count_request(&group)
        .map(|(_, _, selector)| selector)
        .unwrap_or(Value::Null);
    json!({
        "id": id,
        "name": name,
        "normalized_name": normalize_group_name_key(&name).unwrap_or_default(),
        "search_selector": selector,
        "member_count": member_counts.get(&id_text).copied().map(Value::from).unwrap_or(Value::Null),
        "raw": group,
    })
}

pub(crate) fn group_resolve_rows(report: &Value) -> Value {
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let candidates = report
        .get("candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if candidates.is_empty() {
        return Value::Array(vec![group_resolve_row(summary, None)]);
    }
    Value::Array(
        candidates
            .iter()
            .map(|candidate| group_resolve_row(summary, Some(candidate)))
            .collect(),
    )
}

pub(crate) fn group_resolve_row(summary: &Value, candidate: Option<&Value>) -> Value {
    json!({
        "status": summary.get("status").cloned().unwrap_or(Value::Null),
        "total_matches": summary.get("total_matches").cloned().unwrap_or(Value::Null),
        "candidate_count": summary.get("candidate_count").cloned().unwrap_or(Value::Null),
        "candidate_limit": summary.get("candidate_limit").cloned().unwrap_or(Value::Null),
        "truncated": summary.get("truncated").cloned().unwrap_or(Value::Null),
        "resolved_id": summary.get("resolved_id").cloned().unwrap_or(Value::Null),
        "id": candidate.and_then(|value| value.get("id")).cloned().unwrap_or(Value::Null),
        "name": candidate.and_then(|value| value.get("name")).cloned().unwrap_or(Value::Null),
        "normalized_name": candidate.and_then(|value| value.get("normalized_name")).cloned().unwrap_or(Value::Null),
        "search_selector": candidate.and_then(|value| value.get("search_selector")).cloned().unwrap_or(Value::Null),
        "member_count": candidate.and_then(|value| value.get("member_count")).cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn group_profile_dry_run_plan(options: &GroupProfileOptions) -> Value {
    let member_page_limit = options
        .member_limit
        .map(|limit| limit.min(options.page_size))
        .unwrap_or(options.page_size);
    json!({
        "source": "live",
        "filters": group_profile_filters(options),
        "page_size": options.page_size,
        "concurrency": options.concurrency,
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"},
            {"local": "groups:profile", "purpose": "select groups by --query, --group-ids, or explicit --all and compute local audit signals"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "one selected group per request", "include_fields": options.include_fields, "limit": 0}, "concurrency": options.concurrency, "purpose": "count members for each selected group without writes"},
            {"route": "/tools/v2/search", "enabled": options.member_limit != Some(0), "payload": {"group_ids": "same selected group", "include_fields": options.include_fields, "limit": member_page_limit, "exclude_contact_ids": "accumulated from prior pages"}, "page_size": options.page_size, "member_limit": options.member_limit.map(Value::from).unwrap_or(Value::String("all".to_string())), "purpose": "fetch bounded member samples, or all members with --all-members"}
        ],
    })
}

pub(crate) async fn groups_profile(
    runtime: &Runtime,
    options: &GroupProfileOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_count = groups.len();
    let duplicate_name_counts = group_duplicate_name_counts(&groups);
    let mut selected = groups
        .into_iter()
        .filter(|group| group_profile_matches(group, options))
        .collect::<Vec<_>>();
    selected.sort_by(compare_groups_by_name_then_id);
    let selected_count = selected.len();
    if options.one && selected_count != 1 {
        return err(format!(
            "groups:profile --one found {selected_count} matches"
        ));
    }

    let member_options = GroupMembersOptions {
        query: None,
        group_ids: Vec::new(),
        all_groups: true,
        include_fields: options.include_fields.clone(),
        limit_per_group: options.member_limit,
        page_size: options.page_size,
        concurrency: options.concurrency,
        flat: false,
    };
    let (member_groups, errors) =
        group_members_fetch_selected(runtime, &selected, &member_options).await?;
    let profiles = member_groups
        .into_iter()
        .map(|group| group_profile_record(group, &duplicate_name_counts))
        .collect::<Vec<_>>();
    let total_members = profiles
        .iter()
        .filter_map(|group| group.get("total_count").and_then(Value::as_u64))
        .sum::<u64>();
    let returned_members = profiles
        .iter()
        .filter_map(|group| group.get("returned_count").and_then(Value::as_u64))
        .sum::<u64>();
    let truncated_group_count = profiles
        .iter()
        .filter(|group| {
            group
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let zero_member_count = profiles
        .iter()
        .filter(|group| {
            group
                .get("total_count")
                .and_then(Value::as_u64)
                .is_some_and(|count| count == 0)
        })
        .count();
    let issue_group_count = profiles
        .iter()
        .filter(|group| {
            group
                .get("issue_count")
                .and_then(Value::as_u64)
                .unwrap_or_default()
                > 0
        })
        .count();

    Ok(json!({
        "source": source,
        "filters": group_profile_filters(options),
        "pagination": "exclude_contact_ids",
        "page_size": options.page_size,
        "discovered_group_count": discovered_count,
        "selected_group_count": selected_count,
        "returned_group_count": profiles.len(),
        "summary": {
            "total_members": total_members,
            "returned_members": returned_members,
            "truncated_group_count": truncated_group_count,
            "zero_member_count": zero_member_count,
            "issue_group_count": issue_group_count,
            "error_count": errors.len(),
        },
        "groups": profiles,
        "errors": errors,
    }))
}

pub(crate) fn group_profile_matches(group: &Value, options: &GroupProfileOptions) -> bool {
    if let Some(query) = &options.query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    if options.group_ids.is_empty() {
        return options.all || options.query.is_some();
    }
    options
        .group_ids
        .iter()
        .any(|selector| selector.matches_group(group))
}

pub(crate) fn group_profile_filters(options: &GroupProfileOptions) -> Value {
    json!({
        "query": options.query,
        "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
        "all": options.all,
        "one": options.one,
        "member_limit": options.member_limit.map(Value::from).unwrap_or(Value::String("all".to_string())),
        "include_fields": options.include_fields,
    })
}

pub(crate) fn group_profile_record(
    member_group: Value,
    duplicate_name_counts: &BTreeMap<String, u64>,
) -> Value {
    let group_data = member_group.get("group").unwrap_or(&Value::Null);
    let id = group_data
        .get("id")
        .cloned()
        .or_else(|| group_data.get("raw").and_then(record_id).map(Value::String))
        .unwrap_or(Value::Null);
    let name = group_data
        .get("name")
        .and_then(value_string)
        .unwrap_or_default();
    let normalized_name = normalize_group_name_key(&name);
    let total_count = member_group
        .get("total_count")
        .cloned()
        .unwrap_or(Value::Null);
    let mut issues = Vec::new();
    if id.as_str().unwrap_or_default().trim().is_empty() && !id.is_number() {
        issues.push("missing_id".to_string());
    }
    if normalized_name.is_none() {
        issues.push("empty_name".to_string());
    }
    if normalized_name
        .as_ref()
        .and_then(|value| duplicate_name_counts.get(value))
        .is_some_and(|count| *count > 1)
    {
        issues.push("duplicate_name".to_string());
    }
    if total_count.as_u64().is_some_and(|count| count == 0) {
        issues.push("zero_members".to_string());
    }
    json!({
        "id": id,
        "name": name,
        "normalized_name": normalized_name.unwrap_or_default(),
        "selector": member_group.get("selector").cloned().unwrap_or(Value::Null),
        "total_count": total_count,
        "returned_count": member_group.get("returned_count").cloned().unwrap_or(Value::Null),
        "truncated": member_group.get("truncated").cloned().unwrap_or(Value::Null),
        "issue_count": issues.len(),
        "issues": issues,
        "raw": group_data.get("raw").cloned().unwrap_or(Value::Null),
        "members": member_group.get("members").cloned().unwrap_or(Value::Array(Vec::new())),
    })
}

pub(crate) fn group_profile_rows(report: &Value) -> Value {
    let groups = report
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        groups
            .iter()
            .map(|group| {
                let members = group
                    .get("members")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let member_ids = members
                    .iter()
                    .filter_map(record_id)
                    .collect::<Vec<_>>()
                    .join(",");
                let member_names = members
                    .iter()
                    .filter_map(contact_name)
                    .collect::<Vec<_>>()
                    .join(", ");
                json!({
                    "id": group.get("id").cloned().unwrap_or(Value::Null),
                    "name": group.get("name").cloned().unwrap_or(Value::Null),
                    "selector": group.get("selector").cloned().unwrap_or(Value::Null),
                    "total_count": group.get("total_count").cloned().unwrap_or(Value::Null),
                    "returned_count": group.get("returned_count").cloned().unwrap_or(Value::Null),
                    "truncated": group.get("truncated").cloned().unwrap_or(Value::Null),
                    "issue_count": group.get("issue_count").cloned().unwrap_or(Value::Null),
                    "issues": group.get("issues").map(cell_string).unwrap_or_default(),
                    "sample_member_ids": member_ids,
                    "sample_member_names": member_names,
                })
            })
            .collect(),
    )
}

pub(crate) fn group_overlap_dry_run_plan(options: &GroupOverlapOptions) -> Value {
    json!({
        "source": "live",
        "filters": group_overlap_filters(options),
        "page_size": options.page_size,
        "concurrency": options.concurrency,
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"},
            {"local": "groups:overlap", "purpose": "select groups by --query, --group-ids, or explicit --all"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "one selected group per request", "limit": 0}, "purpose": "count selected group members without writes"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "one selected group per request", "limit": "page_size", "exclude_contact_ids": "accumulated from prior pages"}, "page_size": options.page_size, "concurrency": options.concurrency, "purpose": "page every selected group member for local pairwise overlap math"}
        ],
    })
}

pub(crate) async fn groups_overlap(
    runtime: &Runtime,
    options: &GroupOverlapOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_count = groups.len();
    let mut selected = groups
        .into_iter()
        .filter(|group| group_overlap_matches(group, options))
        .collect::<Vec<_>>();
    selected.sort_by(compare_groups_by_name_then_id);
    let selected_count = selected.len();
    let member_options = GroupMembersOptions {
        query: None,
        group_ids: Vec::new(),
        all_groups: true,
        include_fields: Vec::new(),
        limit_per_group: None,
        page_size: options.page_size,
        concurrency: options.concurrency,
        flat: false,
    };
    let (member_groups, errors) =
        group_members_fetch_selected(runtime, &selected, &member_options).await?;
    let profiles = member_groups
        .iter()
        .map(group_overlap_profile)
        .collect::<Result<Vec<_>>>()?;
    let pair_count = profiles
        .len()
        .saturating_mul(profiles.len().saturating_sub(1))
        / 2;
    let mut pairs = Vec::new();
    let mut identical_pair_count = 0_usize;
    let mut subset_pair_count = 0_usize;
    let mut disjoint_pair_count = 0_usize;

    for left_index in 0..profiles.len() {
        for right_index in (left_index + 1)..profiles.len() {
            let pair = group_overlap_pair(&profiles[left_index], &profiles[right_index]);
            let overlap = pair
                .get("overlap_count")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            let jaccard = pair
                .get("jaccard")
                .and_then(Value::as_f64)
                .unwrap_or_default();
            let relationship = pair
                .get("relationship")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if relationship == "same_members" {
                identical_pair_count += 1;
            }
            if relationship.contains("subset") || relationship == "same_members" {
                subset_pair_count += 1;
            }
            if relationship == "disjoint" {
                disjoint_pair_count += 1;
            }
            if overlap >= options.min_overlap && jaccard >= options.min_jaccard {
                pairs.push(pair);
            }
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
            .then_with(|| right_jaccard.total_cmp(&left_jaccard))
            .then_with(|| {
                left.get("left_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("left_name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
            .then_with(|| {
                left.get("right_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("right_name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });
    let matching_pair_count = pairs.len();
    if let Some(top) = options.top {
        pairs.truncate(top);
    }
    let zero_member_group_count = profiles
        .iter()
        .filter(|profile| profile.member_ids.is_empty())
        .count();

    Ok(json!({
        "source": source,
        "filters": group_overlap_filters(options),
        "pagination": "exclude_contact_ids",
        "page_size": options.page_size,
        "discovered_group_count": discovered_count,
        "selected_group_count": selected_count,
        "scanned_group_count": profiles.len(),
        "summary": {
            "group_count": profiles.len(),
            "pair_count": pair_count,
            "matching_pair_count": matching_pair_count,
            "returned_pair_count": pairs.len(),
            "zero_member_group_count": zero_member_group_count,
            "identical_pair_count": identical_pair_count,
            "subset_pair_count": subset_pair_count,
            "disjoint_pair_count": disjoint_pair_count,
            "error_count": errors.len(),
        },
        "groups": profiles.iter().map(GroupOverlapProfile::summary_value).collect::<Vec<_>>(),
        "pairs": pairs,
        "errors": errors,
    }))
}

#[derive(Clone, Debug)]
pub(crate) struct GroupOverlapProfile {
    pub(crate) id: Value,
    pub(crate) id_key: String,
    pub(crate) name: String,
    pub(crate) selector: Value,
    pub(crate) total_count: Value,
    pub(crate) member_ids: BTreeSet<u64>,
}

impl GroupOverlapProfile {
    pub(crate) fn summary_value(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "selector": self.selector,
            "total_count": self.total_count,
            "member_count": self.member_ids.len(),
        })
    }
}

pub(crate) fn group_overlap_profile(group: &Value) -> Result<GroupOverlapProfile> {
    let group_data = group.get("group").unwrap_or(&Value::Null);
    let id = group_data.get("id").cloned().unwrap_or(Value::Null);
    let id_key = id
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| id.as_u64().map(|id| id.to_string()))
        .unwrap_or_default();
    let name = group_data
        .get("name")
        .and_then(value_string)
        .unwrap_or_default();
    let mut member_ids = BTreeSet::new();
    for member in group
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = contact_id_from_value(member)
            .ok_or_else(|| miette!("groups:overlap member row did not include numeric id"))?;
        member_ids.insert(id);
    }
    Ok(GroupOverlapProfile {
        id,
        id_key,
        name,
        selector: group.get("selector").cloned().unwrap_or(Value::Null),
        total_count: group.get("total_count").cloned().unwrap_or(Value::Null),
        member_ids,
    })
}

pub(crate) fn group_overlap_pair(left: &GroupOverlapProfile, right: &GroupOverlapProfile) -> Value {
    let left_count = left.member_ids.len();
    let right_count = right.member_ids.len();
    let overlap_count = left.member_ids.intersection(&right.member_ids).count();
    let union_count = left.member_ids.union(&right.member_ids).count();
    let left_only_count = left_count.saturating_sub(overlap_count);
    let right_only_count = right_count.saturating_sub(overlap_count);
    let jaccard = if union_count == 0 {
        1.0
    } else {
        overlap_count as f64 / union_count as f64
    };
    let left_subset = overlap_count == left_count;
    let right_subset = overlap_count == right_count;
    let relationship = if left_subset && right_subset {
        "same_members"
    } else if overlap_count == 0 {
        "disjoint"
    } else if left_subset {
        "left_subset_of_right"
    } else if right_subset {
        "right_subset_of_left"
    } else {
        "overlap"
    };
    json!({
        "left_id": left.id,
        "left_id_key": left.id_key,
        "left_name": left.name,
        "left_count": left_count,
        "right_id": right.id,
        "right_id_key": right.id_key,
        "right_name": right.name,
        "right_count": right_count,
        "overlap_count": overlap_count,
        "left_only_count": left_only_count,
        "right_only_count": right_only_count,
        "union_count": union_count,
        "jaccard": number_value(jaccard).unwrap_or(Value::Null),
        "relationship": relationship,
    })
}

pub(crate) fn group_overlap_matches(group: &Value, options: &GroupOverlapOptions) -> bool {
    if let Some(query) = &options.query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    if options.group_ids.is_empty() {
        return options.all || options.query.is_some();
    }
    options
        .group_ids
        .iter()
        .any(|selector| selector.matches_group(group))
}

pub(crate) fn group_overlap_filters(options: &GroupOverlapOptions) -> Value {
    json!({
        "query": options.query,
        "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
        "all": options.all,
        "min_overlap": options.min_overlap,
        "min_jaccard": number_value(options.min_jaccard).unwrap_or(Value::Null),
        "top": options.top,
    })
}

pub(crate) fn group_compare_dry_run_plan(options: &GroupCompareOptions) -> Value {
    json!({
        "source": "live",
        "filters": group_compare_filters(options),
        "page_size": options.page_size,
        "concurrency": options.concurrency,
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"},
            {"local": "groups:compare", "purpose": "resolve left and right targets to exactly one group each"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "left and right group selectors", "include_fields": options.include_fields, "limit": 0}, "purpose": "count both group member sets without writes"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "left and right group selectors", "include_fields": options.include_fields, "limit": "page_size", "exclude_contact_ids": "accumulated from prior pages"}, "page_size": options.page_size, "concurrency": options.concurrency, "purpose": "page every member in both groups for local overlap and difference math"}
        ],
    })
}

pub(crate) async fn groups_compare(
    runtime: &Runtime,
    options: &GroupCompareOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_count = groups.len();
    let left_group = group_compare_resolve_target(&groups, &options.left, "left")?;
    let right_group = group_compare_resolve_target(&groups, &options.right, "right")?;

    let (left_report, right_report) = if group_compare_same_group(&left_group, &right_group) {
        let report = group_members_fetch_one(
            runtime.clone(),
            left_group.clone(),
            options.include_fields.clone(),
            None,
            options.page_size,
        )
        .await?;
        (report.clone(), report)
    } else if options.concurrency > 1 {
        let left_group = left_group.clone();
        let right_group = right_group.clone();
        let include_fields = options.include_fields.clone();
        let page_size = options.page_size;
        let left_runtime = runtime.clone();
        let right_runtime = runtime.clone();
        tokio::try_join!(
            group_members_fetch_one(
                left_runtime,
                left_group,
                include_fields.clone(),
                None,
                page_size
            ),
            group_members_fetch_one(right_runtime, right_group, include_fields, None, page_size),
        )?
    } else {
        let left = group_members_fetch_one(
            runtime.clone(),
            left_group.clone(),
            options.include_fields.clone(),
            None,
            options.page_size,
        )
        .await?;
        let right = group_members_fetch_one(
            runtime.clone(),
            right_group.clone(),
            options.include_fields.clone(),
            None,
            options.page_size,
        )
        .await?;
        (left, right)
    };

    let left_index = group_compare_member_index(&left_report)?;
    let right_index = group_compare_member_index(&right_report)?;
    let left_ids = left_index.keys().copied().collect::<BTreeSet<_>>();
    let right_ids = right_index.keys().copied().collect::<BTreeSet<_>>();
    let overlap = left_ids
        .intersection(&right_ids)
        .copied()
        .collect::<Vec<_>>();
    let left_only = left_ids.difference(&right_ids).copied().collect::<Vec<_>>();
    let right_only = right_ids.difference(&left_ids).copied().collect::<Vec<_>>();
    let union_count = left_ids.union(&right_ids).count();
    let overlap_count = overlap.len();
    let jaccard = if union_count == 0 {
        1.0
    } else {
        overlap_count as f64 / union_count as f64
    };
    let overlap_refs =
        group_compare_contact_refs(&overlap, &left_index, &right_index, options.id_limit);
    let left_only_refs =
        group_compare_contact_refs(&left_only, &left_index, &right_index, options.id_limit);
    let right_only_refs =
        group_compare_contact_refs(&right_only, &left_index, &right_index, options.id_limit);

    Ok(json!({
        "source": source,
        "filters": group_compare_filters(options),
        "pagination": "exclude_contact_ids",
        "page_size": options.page_size,
        "discovered_group_count": discovered_count,
        "groups": {
            "left": group_compare_group_meta(&left_report, &options.left),
            "right": group_compare_group_meta(&right_report, &options.right),
        },
        "summary": {
            "left_count": left_ids.len(),
            "right_count": right_ids.len(),
            "overlap_count": overlap_count,
            "left_only_count": left_only.len(),
            "right_only_count": right_only.len(),
            "union_count": union_count,
            "same_members": left_ids == right_ids,
            "jaccard": number_value(jaccard).unwrap_or(Value::Null),
            "id_limit": options.id_limit.map(Value::from).unwrap_or(Value::String("all".to_string())),
            "returned_overlap_count": overlap_refs.len(),
            "returned_left_only_count": left_only_refs.len(),
            "returned_right_only_count": right_only_refs.len(),
            "error_count": 0,
        },
        "sets": {
            "overlap": overlap_refs,
            "left_only": left_only_refs,
            "right_only": right_only_refs,
        },
        "errors": [],
    }))
}

pub(crate) fn group_compare_resolve_target(
    groups: &[Value],
    target: &GroupCompareTarget,
    side: &str,
) -> Result<Value> {
    let mut selected = groups
        .iter()
        .filter(|group| target.matches_group(group))
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by(compare_groups_by_name_then_id);
    if selected.len() != 1 {
        return err(format!(
            "groups:compare {side} target found {} matches",
            selected.len()
        ));
    }
    Ok(selected.remove(0))
}

pub(crate) fn group_compare_same_group(left: &Value, right: &Value) -> bool {
    record_id(left)
        .zip(record_id(right))
        .is_some_and(|(left, right)| left == right)
}

pub(crate) fn group_compare_filters(options: &GroupCompareOptions) -> Value {
    json!({
        "left": options.left.as_value(),
        "right": options.right.as_value(),
        "include_fields": options.include_fields,
        "id_limit": options.id_limit.map(Value::from).unwrap_or(Value::String("all".to_string())),
    })
}

pub(crate) fn group_compare_group_meta(report: &Value, target: &GroupCompareTarget) -> Value {
    let group = report.get("group").unwrap_or(&Value::Null);
    json!({
        "target": target.as_value(),
        "id": group.get("id").cloned().unwrap_or(Value::Null),
        "name": group.get("name").cloned().unwrap_or(Value::Null),
        "selector": report.get("selector").cloned().unwrap_or(Value::Null),
        "total_count": report.get("total_count").cloned().unwrap_or(Value::Null),
        "returned_count": report.get("returned_count").cloned().unwrap_or(Value::Null),
        "truncated": report.get("truncated").cloned().unwrap_or(Value::Null),
        "raw": group.get("raw").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn group_compare_member_index(report: &Value) -> Result<BTreeMap<u64, Value>> {
    let mut index = BTreeMap::new();
    for member in report
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let id = contact_id_from_value(member)
            .ok_or_else(|| miette!("groups:compare member row did not include numeric id"))?;
        index.entry(id).or_insert_with(|| member.clone());
    }
    Ok(index)
}

pub(crate) fn group_compare_contact_refs(
    ids: &[u64],
    left_index: &BTreeMap<u64, Value>,
    right_index: &BTreeMap<u64, Value>,
    id_limit: Option<usize>,
) -> Vec<Value> {
    let limit = id_limit.unwrap_or(usize::MAX);
    ids.iter()
        .take(limit)
        .map(|id| {
            let contact = left_index
                .get(id)
                .or_else(|| right_index.get(id))
                .unwrap_or(&Value::Null);
            json!({
                "id": id,
                "name": contact_name(contact).unwrap_or_default(),
                "display_name": contact.get("displayName").cloned().unwrap_or(Value::Null),
                "url": contact.get("url").cloned().unwrap_or(Value::Null),
                "score": contact.get("score").cloned().unwrap_or(Value::Null),
                "raw": contact,
            })
        })
        .collect()
}

pub(crate) fn group_compare_rows(report: &Value) -> Value {
    let groups = report.get("groups").unwrap_or(&Value::Null);
    let left = groups.get("left").unwrap_or(&Value::Null);
    let right = groups.get("right").unwrap_or(&Value::Null);
    let summary = report.get("summary").unwrap_or(&Value::Null);
    let sets = report.get("sets").unwrap_or(&Value::Null);
    let mut rows = Vec::new();
    for (set_name, count_key, returned_key) in [
        ("overlap", "overlap_count", "returned_overlap_count"),
        ("left_only", "left_only_count", "returned_left_only_count"),
        (
            "right_only",
            "right_only_count",
            "returned_right_only_count",
        ),
    ] {
        let refs = sets
            .get(set_name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if refs.is_empty() {
            rows.push(group_compare_row(
                set_name,
                summary,
                left,
                right,
                count_key,
                returned_key,
                None,
            ));
        } else {
            for contact in refs {
                rows.push(group_compare_row(
                    set_name,
                    summary,
                    left,
                    right,
                    count_key,
                    returned_key,
                    Some(&contact),
                ));
            }
        }
    }
    Value::Array(rows)
}

pub(crate) fn group_compare_row(
    set_name: &str,
    summary: &Value,
    left: &Value,
    right: &Value,
    count_key: &str,
    returned_key: &str,
    contact: Option<&Value>,
) -> Value {
    json!({
        "set": set_name,
        "set_count": summary.get(count_key).cloned().unwrap_or(Value::Null),
        "set_returned": summary.get(returned_key).cloned().unwrap_or(Value::Null),
        "contact_id": contact.and_then(|value| value.get("id")).cloned().unwrap_or(Value::Null),
        "contact_name": contact.and_then(|value| value.get("name")).cloned().unwrap_or(Value::Null),
        "contact_display_name": contact.and_then(|value| value.get("display_name")).cloned().unwrap_or(Value::Null),
        "contact_url": contact.and_then(|value| value.get("url")).cloned().unwrap_or(Value::Null),
        "left_group_id": left.get("id").cloned().unwrap_or(Value::Null),
        "left_group_name": left.get("name").cloned().unwrap_or(Value::Null),
        "left_count": summary.get("left_count").cloned().unwrap_or(Value::Null),
        "right_group_id": right.get("id").cloned().unwrap_or(Value::Null),
        "right_group_name": right.get("name").cloned().unwrap_or(Value::Null),
        "right_count": summary.get("right_count").cloned().unwrap_or(Value::Null),
        "overlap_count": summary.get("overlap_count").cloned().unwrap_or(Value::Null),
        "left_only_count": summary.get("left_only_count").cloned().unwrap_or(Value::Null),
        "right_only_count": summary.get("right_only_count").cloned().unwrap_or(Value::Null),
        "union_count": summary.get("union_count").cloned().unwrap_or(Value::Null),
        "same_members": summary.get("same_members").cloned().unwrap_or(Value::Null),
        "jaccard": summary.get("jaccard").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn group_audit_dry_run_plan(options: &GroupAuditOptions) -> Value {
    let filters = json!({
        "query": options.query,
        "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
        "issues_only": options.issues_only,
    });
    if let Some(dir) = &options.snapshot_dir {
        json!({
            "source": "snapshot",
            "filters": filters,
            "member_counts": false,
            "plan": [
                {"local_file": dir.join("manifest.json").display().to_string(), "purpose": "verify snapshot hashes"},
                {"local_file": dir.join("groups.json").display().to_string(), "purpose": "read snapshot group catalog"},
                {"local": "groups:audit", "purpose": "audit group names, IDs, duplicate names, and local catalog issues"}
            ],
        })
    } else {
        let mut plan = vec![
            json!({"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"}),
            json!({"local": "groups:audit", "purpose": "audit group names, IDs, duplicate names, and local catalog issues"}),
        ];
        if options.member_counts {
            plan.push(json!({
                "route": "/tools/v2/search",
                "payload": {"group_ids": "one selected group per request", "limit": 0},
                "concurrency": options.concurrency,
                "purpose": "count live members for each selected group without writes",
            }));
        }
        json!({
            "source": "live",
            "filters": filters,
            "member_counts": options.member_counts,
            "top": options.top,
            "plan": plan,
        })
    }
}

pub(crate) async fn groups_audit(runtime: &Runtime, options: GroupAuditOptions) -> Result<Value> {
    let (source, mut groups, source_is_live) = if let Some(dir) = &options.snapshot_dir {
        groups_for_audit_snapshot(dir)?
    } else {
        groups_for_audit_live(runtime).await?
    };
    let discovered_count = groups.len();
    groups.retain(|group| group_audit_matches(group, &options));
    groups.sort_by(compare_groups_by_name_then_id);

    let (member_counts, member_count_errors) = if options.member_counts && source_is_live {
        group_audit_live_member_counts(runtime, &groups, options.concurrency).await?
    } else {
        (BTreeMap::new(), Vec::new())
    };
    Ok(group_audit_report(
        source,
        discovered_count,
        groups,
        member_counts,
        member_count_errors,
        &options,
    ))
}

pub(crate) async fn groups_for_audit_live(runtime: &Runtime) -> Result<(Value, Vec<Value>, bool)> {
    let data = runtime.call_tool(route::GET_GROUPS, json!({})).await?;
    let groups = snapshot_group_rows_from_response(&data)?;
    Ok((
        json!({
            "type": "live",
            "group_count": groups.len(),
        }),
        groups,
        true,
    ))
}

pub(crate) fn groups_for_audit_snapshot(dir: &Path) -> Result<(Value, Vec<Value>, bool)> {
    let verify = verify_snapshot(dir)?;
    if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return err("snapshot failed manifest verification");
    }
    let entry = snapshot_manifest_file_entry(dir, "groups")?
        .ok_or_else(|| miette!("snapshot does not contain file groups"))?;
    let path = entry
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| miette!("snapshot file entry groups is missing path"))?;
    let values = read_snapshot_array_record_values_at_path(&safe_snapshot_file_path(dir, path)?)?;
    let groups = values.into_values().collect::<Vec<_>>();
    Ok((
        json!({
            "type": "snapshot",
            "dir": dir.display().to_string(),
            "file": path,
            "group_count": groups.len(),
        }),
        groups,
        false,
    ))
}

pub(crate) fn group_audit_matches(group: &Value, options: &GroupAuditOptions) -> bool {
    if let Some(query) = &options.query
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

pub(crate) async fn group_audit_live_member_counts(
    runtime: &Runtime,
    groups: &[Value],
    concurrency: usize,
) -> Result<(BTreeMap<String, u64>, Vec<Value>)> {
    if concurrency == 0 {
        return err("group audit concurrency must be greater than zero");
    }
    let mut counts = BTreeMap::new();
    let mut errors = Vec::new();
    for chunk in groups.chunks(concurrency) {
        let mut handles = Vec::new();
        for group in chunk {
            let Some((key, name, selector)) = group_audit_member_count_request(group) else {
                errors.push(json!({
                    "id": record_id(group).unwrap_or_default(),
                    "name": group_name(group).unwrap_or_default(),
                    "error": "group has no usable ID for member count",
                }));
                continue;
            };
            let runtime = runtime.clone();
            handles.push(tokio::spawn(async move {
                let mut payload = Map::new();
                payload.set("group_ids", Value::Array(vec![selector]));
                let result = runtime.search_total(payload).await;
                (key, name, result)
            }));
        }
        for handle in handles {
            let (key, name, result) = handle
                .await
                .into_diagnostic()
                .wrap_err("joining groups:audit member-count task")?;
            match result {
                Ok(total) => {
                    counts.insert(key, total as u64);
                }
                Err(error) => {
                    errors.push(json!({
                        "id": key,
                        "name": name,
                        "error": error.to_string(),
                    }));
                }
            }
        }
    }
    Ok((counts, errors))
}

pub(crate) fn group_audit_member_count_request(group: &Value) -> Option<(String, String, Value)> {
    let key = record_id(group)?;
    let name = group_name(group).unwrap_or_default();
    let selector = if normalize_group_name_key(&name).is_some_and(|value| value == "starred") {
        Value::String("starred".to_string())
    } else {
        Value::Number(Number::from(parse_contact_id(&key).ok()?))
    };
    Some((key, name, selector))
}

pub(crate) fn group_audit_report(
    source: Value,
    discovered_count: usize,
    groups: Vec<Value>,
    member_counts: BTreeMap<String, u64>,
    member_count_errors: Vec<Value>,
    options: &GroupAuditOptions,
) -> Value {
    let mut name_counts: BTreeMap<String, u64> = BTreeMap::new();
    for group in &groups {
        if let Some(name) = group_name(group).and_then(|value| normalize_group_name_key(&value)) {
            *name_counts.entry(name).or_default() += 1;
        }
    }
    let duplicate_name_counts = name_counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, count)| (name.clone(), *count))
        .collect::<BTreeMap<_, _>>();

    let mut issue_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut records = Vec::new();
    let mut zero_member_count = 0_u64;
    let mut total_members_counted = 0_u64;
    let selected_count = groups.len();
    for group in groups {
        let id = record_id(&group).unwrap_or_default();
        let name = group_name(&group).unwrap_or_default();
        let normalized_name = normalize_group_name_key(&name);
        let member_count = member_counts.get(&id).copied();
        let mut issues = Vec::new();
        if id.trim().is_empty() {
            issues.push("missing_id".to_string());
        }
        if normalized_name.is_none() {
            issues.push("empty_name".to_string());
        }
        if normalized_name
            .as_ref()
            .and_then(|value| duplicate_name_counts.get(value))
            .is_some_and(|count| *count > 1)
        {
            issues.push("duplicate_name".to_string());
        }
        if let Some(count) = member_count {
            total_members_counted += count;
            if count == 0 {
                zero_member_count += 1;
                issues.push("zero_members".to_string());
            }
        }
        if options.issues_only && issues.is_empty() {
            continue;
        }
        for issue in &issues {
            *issue_counts.entry(issue.clone()).or_default() += 1;
        }
        records.push(json!({
            "id": id,
            "name": name,
            "normalized_name": normalized_name.unwrap_or_default(),
            "member_count": member_count.map(Value::from).unwrap_or(Value::Null),
            "issue_count": issues.len(),
            "issues": issues,
            "raw": group,
        }));
    }
    records.sort_by(|left, right| {
        let left_issues = left
            .get("issue_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let right_issues = right
            .get("issue_count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        right_issues
            .cmp(&left_issues)
            .then_with(|| {
                right
                    .get("member_count")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
                    .cmp(
                        &left
                            .get("member_count")
                            .and_then(Value::as_u64)
                            .unwrap_or_default(),
                    )
            })
            .then_with(|| {
                left.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });

    json!({
        "source": source,
        "filters": {
            "query": options.query,
            "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "issues_only": options.issues_only,
        },
        "discovered_count": discovered_count,
        "analyzed_count": selected_count,
        "returned_count": records.len(),
        "summary": {
            "selected_count": selected_count,
            "issue_group_count": records.iter().filter(|record| record.get("issue_count").and_then(Value::as_u64).unwrap_or_default() > 0).count(),
            "total_issues": issue_counts.values().sum::<u64>(),
            "top_issues": top_count_entries(&issue_counts, options.top),
            "duplicate_name_count": duplicate_name_counts.len(),
            "top_duplicate_names": top_count_entries(&duplicate_name_counts, options.top),
            "member_counts": {
                "enabled": options.member_counts,
                "counted": member_counts.len(),
                "error_count": member_count_errors.len(),
                "zero_member_count": zero_member_count,
                "total_members_counted": total_members_counted,
                "top_groups": group_audit_top_member_rows(&records, options.top),
                "errors": member_count_errors,
            },
        },
        "groups": records,
    })
}

pub(crate) fn group_audit_top_member_rows(records: &[Value], top: usize) -> Value {
    let mut rows = records
        .iter()
        .filter(|record| record.get("member_count").and_then(Value::as_u64).is_some())
        .map(|record| {
            json!({
                "id": record.get("id").cloned().unwrap_or(Value::Null),
                "name": record.get("name").cloned().unwrap_or(Value::Null),
                "member_count": record.get("member_count").cloned().unwrap_or(Value::Null),
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .get("member_count")
            .and_then(Value::as_u64)
            .unwrap_or_default()
            .cmp(
                &left
                    .get("member_count")
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
            )
            .then_with(|| {
                left.get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(
                        right
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            })
    });
    rows.truncate(top);
    Value::Array(rows)
}

pub(crate) fn group_audit_table_rows(report: &Value) -> Value {
    let Some(rows) = report.get("groups").and_then(Value::as_array) else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.get("id").cloned().unwrap_or(Value::Null),
                    "name": row.get("name").cloned().unwrap_or(Value::Null),
                    "member_count": row.get("member_count").cloned().unwrap_or(Value::Null),
                    "issues": row.get("issues").map(cell_string).unwrap_or_default(),
                })
            })
            .collect(),
    )
}

pub(crate) fn group_members_dry_run_plan(options: &GroupMembersOptions) -> Value {
    json!({
        "source": "live",
        "filters": {
            "query": options.query,
            "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "all_groups": options.all_groups,
            "include_fields": options.include_fields,
            "limit_per_group": options.limit_per_group,
        },
        "page_size": options.page_size,
        "concurrency": options.concurrency,
        "flat": options.flat,
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog without writes"},
            {"local": "group selection", "purpose": "select groups by --group-ids, --query, or --all-groups"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "one selected group per request", "include_fields": options.include_fields, "limit": 0}, "concurrency": options.concurrency, "purpose": "count members for each selected group"},
            {"route": "/tools/v2/search", "payload": {"group_ids": "same selected group", "include_fields": options.include_fields, "limit": options.page_size, "exclude_contact_ids": "accumulated from prior pages"}, "page_size": options.page_size, "purpose": "fetch group members until count or --limit-per-group is reached"}
        ],
    })
}

pub(crate) async fn groups_members(
    runtime: &Runtime,
    options: GroupMembersOptions,
) -> Result<Value> {
    let (source, groups, _) = groups_for_audit_live(runtime).await?;
    let discovered_count = groups.len();
    let selected_groups = groups
        .into_iter()
        .filter(|group| group_members_matches(group, &options))
        .collect::<Vec<_>>();
    let selected_count = selected_groups.len();
    let (groups, errors) =
        group_members_fetch_selected(runtime, &selected_groups, &options).await?;
    let total_members = groups
        .iter()
        .filter_map(|group| group.get("total_count").and_then(Value::as_u64))
        .sum::<u64>();
    let returned_members = groups
        .iter()
        .filter_map(|group| group.get("returned_count").and_then(Value::as_u64))
        .sum::<u64>();
    let truncated_group_count = groups
        .iter()
        .filter(|group| {
            group
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    Ok(json!({
        "source": source,
        "filters": {
            "query": options.query,
            "group_ids": options.group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "all_groups": options.all_groups,
            "include_fields": options.include_fields,
            "limit_per_group": options.limit_per_group,
        },
        "pagination": "exclude_contact_ids",
        "page_size": options.page_size,
        "discovered_group_count": discovered_count,
        "selected_group_count": selected_count,
        "returned_group_count": groups.len(),
        "summary": {
            "total_members": total_members,
            "returned_members": returned_members,
            "truncated_group_count": truncated_group_count,
            "error_count": errors.len(),
        },
        "groups": groups,
        "errors": errors,
    }))
}

pub(crate) fn group_members_matches(group: &Value, options: &GroupMembersOptions) -> bool {
    if let Some(query) = &options.query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    if options.group_ids.is_empty() {
        return options.all_groups || options.query.is_some();
    }
    options
        .group_ids
        .iter()
        .any(|selector| selector.matches_group(group))
}

pub(crate) async fn group_members_fetch_selected(
    runtime: &Runtime,
    groups: &[Value],
    options: &GroupMembersOptions,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for chunk in groups.chunks(options.concurrency) {
        let mut handles = Vec::new();
        for group in chunk {
            let group = group.clone();
            let id = record_id(&group).unwrap_or_default();
            let name = group_name(&group).unwrap_or_default();
            let runtime = runtime.clone();
            let include_fields = options.include_fields.clone();
            let limit_per_group = options.limit_per_group;
            let page_size = options.page_size;
            handles.push(tokio::spawn(async move {
                let result = group_members_fetch_one(
                    runtime,
                    group,
                    include_fields,
                    limit_per_group,
                    page_size,
                )
                .await;
                (id, name, result)
            }));
        }
        for handle in handles {
            let (id, name, result) = handle
                .await
                .into_diagnostic()
                .wrap_err("joining groups:members fetch task")?;
            match result {
                Ok(value) => results.push(value),
                Err(error) => errors.push(json!({
                    "id": id,
                    "name": name,
                    "error": error.to_string(),
                })),
            }
        }
    }
    results.sort_by(|left, right| {
        left.get("group")
            .and_then(|group| group.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .get("group")
                    .and_then(|group| group.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });
    Ok((results, errors))
}

pub(crate) async fn group_members_fetch_one(
    runtime: Runtime,
    group: Value,
    include_fields: Vec<String>,
    limit_per_group: Option<usize>,
    page_size: usize,
) -> Result<Value> {
    let (group_id, group_name, selector) = group_audit_member_count_request(&group)
        .ok_or_else(|| miette!("group has no usable ID for member search"))?;
    let mut payload = Map::new();
    payload.insert(
        "group_ids".to_string(),
        Value::Array(vec![selector.clone()]),
    );
    if !include_fields.is_empty() {
        payload.insert(
            "include_fields".to_string(),
            Value::Array(include_fields.into_iter().map(Value::String).collect()),
        );
    }
    let mut members = Vec::new();
    let (returned_count, total_count) =
        export_contacts_each_limited(&runtime, payload, page_size, limit_per_group, |row| {
            members.push(row);
            Ok(())
        })
        .await?;
    Ok(json!({
        "group": {
            "id": group_id,
            "name": group_name,
            "raw": group,
        },
        "selector": selector,
        "total_count": total_count,
        "returned_count": returned_count,
        "truncated": returned_count < total_count,
        "members": members,
    }))
}

pub(crate) fn group_members_flat_rows(report: &Value) -> Value {
    let Some(groups) = report.get("groups").and_then(Value::as_array) else {
        return Value::Array(Vec::new());
    };
    let mut rows = Vec::new();
    for group in groups {
        let group_data = group.get("group").unwrap_or(&Value::Null);
        let group_id = group_data.get("id").cloned().unwrap_or(Value::Null);
        let group_name = group_data.get("name").cloned().unwrap_or(Value::Null);
        let selector = group.get("selector").cloned().unwrap_or(Value::Null);
        let total_count = group.get("total_count").cloned().unwrap_or(Value::Null);
        let returned_count = group.get("returned_count").cloned().unwrap_or(Value::Null);
        let truncated = group
            .get("truncated")
            .cloned()
            .unwrap_or(Value::Bool(false));
        let members = group
            .get("members")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if members.is_empty() {
            rows.push(json!({
                "group_id": group_id,
                "group_name": group_name,
                "group_selector": selector,
                "group_total": total_count,
                "group_returned": returned_count,
                "group_truncated": truncated,
                "contact_id": Value::Null,
                "contact_name": Value::Null,
                "contact_display_name": Value::Null,
                "contact_url": Value::Null,
                "contact_score": Value::Null,
            }));
            continue;
        }
        for member in members {
            rows.push(json!({
                "group_id": group_id,
                "group_name": group_name,
                "group_selector": selector,
                "group_total": total_count,
                "group_returned": returned_count,
                "group_truncated": truncated,
                "contact_id": record_id(&member).map(Value::String).unwrap_or(Value::Null),
                "contact_name": contact_name(&member).map(Value::String).unwrap_or(Value::Null),
                "contact_display_name": member.get("displayName").cloned().unwrap_or(Value::Null),
                "contact_url": member.get("url").cloned().unwrap_or(Value::Null),
                "contact_score": member.get("score").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    Value::Array(rows)
}

pub(crate) async fn group_bulk_membership_plan(
    runtime: &Runtime,
    options: &GroupBulkMembershipOptions,
) -> Result<GroupBulkMembershipPlan> {
    let (_, groups, _) = groups_for_audit_live(runtime).await?;
    let mut selected_groups = groups
        .into_iter()
        .filter(|group| group_bulk_membership_matches(group, options))
        .collect::<Vec<_>>();
    selected_groups.sort_by(compare_groups_by_name_then_id);

    let mut writable_groups = Vec::new();
    let mut skipped_special_groups = Vec::new();
    for group in selected_groups {
        if group_is_special_starred(&group) {
            skipped_special_groups.push(group);
        } else {
            writable_groups.push(group);
        }
    }

    if options.one && writable_groups.len() != 1 {
        return err(format!(
            "{} --one found {} writable groups",
            options.command,
            writable_groups.len()
        ));
    }
    if let Some(limit) = options.group_limit {
        writable_groups.truncate(limit);
    }
    if writable_groups.is_empty() {
        return err(format!("{} resolved zero writable groups", options.command));
    }

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

    let mut actions = Vec::new();
    let mut row = 1_usize;
    for group in &writable_groups {
        let (group_id, group_name) = group_bulk_membership_group_ref(group)?;
        for chunk in target_ids.chunks(options.chunk_size) {
            actions.push(group_bulk_membership_action(
                row,
                options.kind,
                group_id,
                group_name.clone(),
                chunk,
            ));
            row = row.saturating_add(1);
        }
    }

    let group_source = match (
        options.target_group_ids.is_empty(),
        options.query.is_some(),
        options.all_groups,
    ) {
        (_, _, true) => "all-groups",
        (false, true, false) => "explicit+query",
        (false, false, false) => "explicit",
        (true, true, false) => "query",
        (true, false, false) => "none",
    }
    .to_string();
    let target_source = match (explicit_ids.is_empty(), options.search_payload.is_some()) {
        (false, true) => "explicit+search",
        (false, false) => "explicit",
        (true, true) => "search",
        (true, false) => "none",
    }
    .to_string();

    Ok(GroupBulkMembershipPlan {
        kind: options.kind,
        command: options.command,
        group_source,
        target_source,
        query: options.query.clone(),
        target_group_ids: options.target_group_ids.clone(),
        selected_groups: writable_groups,
        skipped_special_groups,
        explicit_ids,
        search_payload: options.search_payload.clone(),
        search_exported_count,
        search_match_count,
        target_ids,
        actions,
        page_size: options.page_size,
        target_limit: options.target_limit,
        group_limit: options.group_limit,
        chunk_size: options.chunk_size,
        concurrency: options.concurrency,
    })
}

pub(crate) fn group_bulk_membership_matches(
    group: &Value,
    options: &GroupBulkMembershipOptions,
) -> bool {
    if let Some(query) = &options.query
        && !group_name_matches_query(group, query)
    {
        return false;
    }
    if options.target_group_ids.is_empty() {
        return options.all_groups || options.query.is_some();
    }
    options
        .target_group_ids
        .iter()
        .any(|selector| selector.matches_group(group))
}

pub(crate) fn group_bulk_membership_group_ref(group: &Value) -> Result<(u64, String)> {
    let id = record_id(group)
        .ok_or_else(|| miette!("group did not include an ID"))?
        .parse::<u64>()
        .into_diagnostic()
        .wrap_err("group ID was not numeric")?;
    Ok((id, group_name(group).unwrap_or_default()))
}

pub(crate) fn group_bulk_membership_action(
    row: usize,
    kind: GroupApplyKind,
    group_id: u64,
    group_name: String,
    contact_ids: &[u64],
) -> GroupBulkMembershipAction {
    let mut payload = Map::new();
    payload.insert(
        "group_id".to_string(),
        Value::Number(Number::from(group_id)),
    );
    payload.insert(
        if kind == GroupApplyKind::Add {
            "add_contact_ids".to_string()
        } else {
            "remove_contact_ids".to_string()
        },
        json!(contact_ids),
    );
    GroupBulkMembershipAction {
        row,
        kind,
        group_id,
        group_name,
        route: route::UPDATE_GROUP,
        payload,
    }
}

pub(crate) fn group_bulk_membership_plan_value(plan: &GroupBulkMembershipPlan) -> Value {
    json!({
        "source": "live",
        "action": plan.kind.as_str(),
        "command": plan.command,
        "group_source": plan.group_source,
        "target_source": plan.target_source,
        "filters": {
            "query": plan.query,
            "target_group_ids": plan.target_group_ids.iter().map(GroupAuditSelector::as_value).collect::<Vec<_>>(),
            "contact_ids": plan.explicit_ids,
            "from_search": plan.search_payload.is_some(),
            "search_payload": plan.search_payload,
            "group_limit": plan.group_limit,
            "target_limit": plan.target_limit,
        },
        "summary": {
            "group_count": plan.selected_groups.len(),
            "skipped_special_group_count": plan.skipped_special_groups.len(),
            "target_count": plan.target_ids.len(),
            "pair_count": plan.selected_groups.len().saturating_mul(plan.target_ids.len()),
            "explicit_count": plan.explicit_ids.len(),
            "search_exported_count": plan.search_exported_count,
            "search_match_count": plan.search_match_count,
            "write_chunk_count": plan.actions.len(),
            "write_required": !plan.actions.is_empty(),
        },
        "page_size": plan.page_size,
        "chunk_size": plan.chunk_size,
        "concurrency": plan.concurrency,
        "groups": plan.selected_groups.iter().map(group_bulk_membership_group_value).collect::<Vec<_>>(),
        "skipped_special_groups": plan.skipped_special_groups.iter().map(group_bulk_membership_group_value).collect::<Vec<_>>(),
        "target_contact_ids": plan.target_ids,
        "plan": [
            {"route": "/tools/v2/get-groups", "payload": {}, "purpose": "read live group catalog and select writable target groups"},
            {"route": "/tools/v2/search", "enabled": plan.search_payload.is_some(), "payload": "same contact search filters with limit set to page_size and exclude_contact_ids accumulated from explicit/prior IDs", "page_size": plan.page_size, "purpose": "resolve target contact IDs without writes"},
            {"route": "/tools/v2/update-group", "payload": {"group_id": "one selected group", "contact_ids": "up to chunk_size target IDs per request"}, "chunk_size": plan.chunk_size, "concurrency": plan.concurrency, "purpose": format!("{} selected contacts for each selected writable group; requires --yes outside dry-run", plan.kind.as_str())}
        ],
        "actions": plan.actions.iter().map(group_bulk_membership_action_value).collect::<Vec<_>>(),
    })
}

pub(crate) fn group_bulk_membership_group_value(group: &Value) -> Value {
    let (id, name) = group_bulk_membership_group_ref(group).unwrap_or((0, String::new()));
    json!({
        "id": if id == 0 { Value::Null } else { Value::Number(Number::from(id)) },
        "name": name,
        "raw": group,
    })
}

pub(crate) async fn apply_group_bulk_membership(
    runtime: &Runtime,
    plan: &GroupBulkMembershipPlan,
) -> Result<Value> {
    let mut results = Vec::with_capacity(plan.actions.len());
    let mut failures = 0_u64;
    let mut changed_pair_count = 0_usize;
    let mut failed_pair_count = 0_usize;
    let outcomes = run_bulk_tool_calls(
        runtime,
        plan.actions.clone(),
        plan.concurrency,
        &format!("joining {} write task", plan.command),
        |action| (action.route, Value::Object(action.payload.clone())),
    )
    .await?;
    for (action, result) in outcomes {
        let contact_ids = group_bulk_membership_action_contact_ids(&action);
        let target_count = contact_ids.len();
        match result {
            Ok(data) => {
                changed_pair_count = changed_pair_count.saturating_add(target_count);
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "group_id": action.group_id,
                    "group_name": action.group_name,
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
                failed_pair_count = failed_pair_count.saturating_add(target_count);
                results.push(json!({
                    "row": action.row,
                    "action": action.kind.as_str(),
                    "group_id": action.group_id,
                    "group_name": action.group_name,
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
        "command": plan.command,
        "group_source": plan.group_source,
        "target_source": plan.target_source,
        "summary": {
            "group_count": plan.selected_groups.len(),
            "target_count": plan.target_ids.len(),
            "pair_count": plan.selected_groups.len().saturating_mul(plan.target_ids.len()),
            "write_chunk_count": plan.actions.len(),
            "changed_pair_count": changed_pair_count,
            "failed_pair_count": failed_pair_count,
            "failure_count": failures,
            "ok": failures == 0,
        },
        "results": results,
    }))
}

pub(crate) fn group_bulk_membership_action_value(action: &GroupBulkMembershipAction) -> Value {
    let contact_ids = group_bulk_membership_action_contact_ids(action);
    json!({
        "row": action.row,
        "action": action.kind.as_str(),
        "group_id": action.group_id,
        "group_name": action.group_name,
        "contact_ids": contact_ids,
        "target_count": contact_ids.len(),
        "route": format!("/tools/v2{}", action.route),
        "payload": action.payload,
    })
}

pub(crate) fn group_bulk_membership_action_contact_ids(
    action: &GroupBulkMembershipAction,
) -> Vec<u64> {
    let key = if action.kind == GroupApplyKind::Add {
        "add_contact_ids"
    } else {
        "remove_contact_ids"
    };
    action
        .payload
        .get(key)
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|value| value.as_u64())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn group_bulk_membership_rows(report: &Value) -> Value {
    let rows = report
        .get("results")
        .or_else(|| report.get("actions"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(group_bulk_membership_row)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Value::Array(rows)
}

pub(crate) fn group_bulk_membership_row(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let payload = object.get("payload").unwrap_or(&Value::Null);
    let contact_ids = object
        .get("contact_ids")
        .cloned()
        .or_else(|| payload.get("add_contact_ids").cloned())
        .or_else(|| payload.get("remove_contact_ids").cloned())
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let target_count = object.get("target_count").cloned().unwrap_or_else(|| {
        Value::Number(Number::from(
            contact_ids.as_array().map(Vec::len).unwrap_or_default(),
        ))
    });
    Some(json!({
        "row": object.get("row").cloned().unwrap_or(Value::Null),
        "action": object.get("action").cloned().unwrap_or(Value::Null),
        "group_id": object
            .get("group_id")
            .cloned()
            .or_else(|| payload.get("group_id").cloned())
            .unwrap_or(Value::Null),
        "group_name": object.get("group_name").cloned().unwrap_or(Value::Null),
        "route": object.get("route").cloned().unwrap_or(Value::Null),
        "ok": object.get("ok").cloned().unwrap_or(Value::Null),
        "target_count": target_count,
        "contact_ids": contact_ids,
        "result_id": object.get("result_id").cloned().unwrap_or(Value::Null),
        "error": object.get("error").cloned().unwrap_or(Value::Null),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-group-audit-{label}-{}-{}",
            std::process::id(),
            now_millis()
        ))
    }

    fn write_groups_snapshot(dir: &Path, path: &str, content: &str) -> Result<()> {
        let file_path = safe_snapshot_file_path(dir, path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(file_path, content).into_diagnostic()?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&json!({
                "files": {
                    "groups": {
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
    fn groups_for_audit_snapshot_reads_groups_from_manifest_path() -> Result<()> {
        let dir = temp_snapshot_dir("manifest-path");
        write_groups_snapshot(&dir, "data/groups.json", r#"[{"id":7,"name":"Nested"}]"#)?;

        let (source, groups, live_member_counts) = groups_for_audit_snapshot(&dir)?;

        fs::remove_dir_all(&dir).ok();
        assert_eq!(source.get("file"), Some(&json!("data/groups.json")));
        assert_eq!(groups, vec![json!({"id":7,"name":"Nested"})]);
        assert!(!live_member_counts);
        Ok(())
    }

    #[test]
    fn group_resolve_matches_collapsed_whitespace_query() {
        let options = GroupResolveOptions {
            query: Some("sales team".to_string()),
            group_ids: Vec::new(),
            all: false,
            one: false,
            candidate_limit: 10,
            member_counts: false,
            concurrency: 1,
        };

        assert!(group_resolve_matches(
            &json!({"id": 1, "name": "Sales   Team"}),
            &options
        ));
    }

    #[test]
    fn group_bulk_membership_action_builds_add_payload() {
        let action =
            group_bulk_membership_action(3, GroupApplyKind::Add, 5, "Friends".to_string(), &[1, 2]);

        assert_eq!(action.row, 3);
        assert_eq!(action.kind, GroupApplyKind::Add);
        assert_eq!(action.group_id, 5);
        assert_eq!(action.group_name, "Friends");
        assert_eq!(action.route, "/update-group");
        assert_eq!(
            group_bulk_membership_action_contact_ids(&action),
            vec![1, 2]
        );
        assert_eq!(
            Value::Object(action.payload.clone()),
            json!({"group_id": 5, "add_contact_ids": [1, 2]})
        );
        assert_eq!(
            group_bulk_membership_action_value(&action).get("contact_ids"),
            Some(&json!([1, 2]))
        );
    }

    #[test]
    fn group_bulk_membership_action_builds_remove_payload() {
        let action = group_bulk_membership_action(
            4,
            GroupApplyKind::Remove,
            5,
            "Friends".to_string(),
            &[8, 9],
        );

        assert_eq!(action.kind, GroupApplyKind::Remove);
        assert_eq!(
            group_bulk_membership_action_contact_ids(&action),
            vec![8, 9]
        );
        assert_eq!(
            Value::Object(action.payload),
            json!({"group_id": 5, "remove_contact_ids": [8, 9]})
        );
    }
}
