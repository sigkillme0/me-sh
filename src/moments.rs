use crate::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MomentsFeedSort {
    Desc,
    Asc,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MomentsStatsDateBucket {
    Day,
    Month,
    Year,
    Raw,
}

#[derive(Clone, Debug)]
pub(crate) struct MomentsFeedOptions {
    pub(crate) contact_ids: Vec<u64>,
    pub(crate) sections: Vec<&'static SnapshotMomentRoute>,
    pub(crate) start: Option<String>,
    pub(crate) end: Option<String>,
    pub(crate) limit: usize,
    pub(crate) item_limit: Option<usize>,
    pub(crate) sort: MomentsFeedSort,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct MomentsStatsOptions {
    pub(crate) feed: MomentsFeedOptions,
    pub(crate) date_bucket: MomentsStatsDateBucket,
    pub(crate) top_contacts: usize,
    pub(crate) top_dates: usize,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct MomentsTimelineOptions {
    pub(crate) feed: MomentsFeedOptions,
    pub(crate) snapshot_dir: Option<PathBuf>,
    pub(crate) date_bucket: MomentsStatsDateBucket,
    pub(crate) bucket_limit: usize,
    pub(crate) items_per_bucket: usize,
    pub(crate) verify: bool,
    pub(crate) flat: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MomentsStatsSectionBucket {
    pub(crate) section: String,
    pub(crate) route: Value,
    pub(crate) pages: Value,
    pub(crate) count: u64,
    pub(crate) dated_count: u64,
    pub(crate) undated_count: u64,
    pub(crate) contact_keys: BTreeSet<String>,
    pub(crate) first_date: Option<String>,
    pub(crate) last_date: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MomentsStatsContactBucket {
    pub(crate) contact_id: Option<u64>,
    pub(crate) contact_name: Option<String>,
    pub(crate) count: u64,
    pub(crate) sections: BTreeSet<String>,
    pub(crate) latest_date: Option<String>,
    pub(crate) latest_summary: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MomentsStatsDateBucketRow {
    pub(crate) bucket: String,
    pub(crate) count: u64,
    pub(crate) sections: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MomentsTimelineBucket {
    pub(crate) bucket: String,
    pub(crate) count: u64,
    pub(crate) dated_count: u64,
    pub(crate) undated_count: u64,
    pub(crate) sections: BTreeSet<String>,
    pub(crate) contact_keys: BTreeSet<String>,
    pub(crate) first_date: Option<String>,
    pub(crate) last_date: Option<String>,
    pub(crate) items: Vec<Value>,
}

impl MomentsFeedOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let contact_ids = dedupe_ids(optional_ids_from_matches(matches, "contact-ids")?);
        let sections = contact_activity_sections_from_matches(matches)?;
        let start = matches.get_one::<String>("start").cloned();
        let end = matches.get_one::<String>("end").cloned();
        if sections
            .iter()
            .any(|route| matches!(route.kind, SnapshotMomentKind::DateWindow))
            && (start.is_none() || end.is_none())
        {
            return err(
                "moments:feed sections notes, events, and emails require --start and --end",
            );
        }
        let sort = MomentsFeedSort::parse(
            matches
                .get_one::<String>("sort")
                .map(String::as_str)
                .unwrap_or("desc"),
        )?;
        Ok(Self {
            contact_ids,
            sections,
            start,
            end,
            limit: optional_usize_from_matches(matches, "limit")?
                .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT),
            item_limit: optional_usize_from_matches(matches, "item-limit")?,
            sort,
            flat: matches.get_flag("flat"),
        })
    }
}

impl MomentsStatsOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let contact_ids = dedupe_ids(optional_ids_from_matches(matches, "contact-ids")?);
        let sections = contact_activity_sections_from_matches(matches)?;
        let start = matches.get_one::<String>("start").cloned();
        let end = matches.get_one::<String>("end").cloned();
        if sections
            .iter()
            .any(|route| matches!(route.kind, SnapshotMomentKind::DateWindow))
            && (start.is_none() || end.is_none())
        {
            return err(
                "moments:stats sections notes, events, and emails require --start and --end",
            );
        }
        let top_contacts = optional_usize_from_matches(matches, "top-contacts")?.unwrap_or(10);
        let top_dates = optional_usize_from_matches(matches, "top-dates")?.unwrap_or(10);
        let date_bucket = MomentsStatsDateBucket::parse(
            matches
                .get_one::<String>("date-bucket")
                .map(String::as_str)
                .unwrap_or("day"),
        )?;
        let flat = matches.get_flag("flat");
        Ok(Self {
            feed: MomentsFeedOptions {
                contact_ids,
                sections,
                start,
                end,
                limit: optional_usize_from_matches(matches, "limit")?
                    .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT),
                item_limit: None,
                sort: MomentsFeedSort::None,
                flat,
            },
            date_bucket,
            top_contacts,
            top_dates,
            flat,
        })
    }
}

impl MomentsTimelineOptions {
    pub(crate) fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let contact_ids = dedupe_ids(optional_ids_from_matches(matches, "contact-ids")?);
        let sections = contact_activity_sections_from_matches(matches)?;
        let snapshot_dir = matches.get_one::<String>("snapshot-dir").map(PathBuf::from);
        let start = matches.get_one::<String>("start").cloned();
        let end = matches.get_one::<String>("end").cloned();
        if start.is_some() != end.is_some() {
            return err("moments:timeline accepts --start and --end together, not just one");
        }
        if snapshot_dir.is_none()
            && sections
                .iter()
                .any(|route| matches!(route.kind, SnapshotMomentKind::DateWindow))
            && (start.is_none() || end.is_none())
        {
            return err(
                "moments:timeline live sections notes, events, and emails require --start and --end",
            );
        }
        let sort = MomentsFeedSort::parse(
            matches
                .get_one::<String>("sort")
                .map(String::as_str)
                .unwrap_or("desc"),
        )?;
        let date_bucket = MomentsStatsDateBucket::parse(
            matches
                .get_one::<String>("date-bucket")
                .map(String::as_str)
                .unwrap_or("day"),
        )?;
        let bucket_limit = optional_usize_from_matches(matches, "bucket-limit")?
            .unwrap_or(MOMENTS_TIMELINE_BUCKET_LIMIT_DEFAULT);
        if bucket_limit == 0 {
            return err("--bucket-limit must be greater than zero");
        }
        let items_per_bucket = optional_usize_from_matches(matches, "items-per-bucket")?
            .unwrap_or(MOMENTS_TIMELINE_ITEMS_PER_BUCKET_DEFAULT);
        let flat = matches.get_flag("flat");
        Ok(Self {
            feed: MomentsFeedOptions {
                contact_ids,
                sections,
                start,
                end,
                limit: optional_usize_from_matches(matches, "limit")?
                    .unwrap_or(MOMENT_PAGE_SIZE_DEFAULT),
                item_limit: optional_usize_from_matches(matches, "item-limit")?,
                sort,
                flat,
            },
            snapshot_dir,
            date_bucket,
            bucket_limit,
            items_per_bucket,
            verify: !matches.get_flag("skip-verify"),
            flat,
        })
    }
}

impl MomentsFeedSort {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "desc" => Ok(Self::Desc),
            "asc" => Ok(Self::Asc),
            "none" => Ok(Self::None),
            _ => err(format!("unknown moments:feed sort {value}")),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Desc => "desc",
            Self::Asc => "asc",
            Self::None => "none",
        }
    }
}

impl MomentsStatsDateBucket {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "day" | "daily" => Ok(Self::Day),
            "month" | "monthly" => Ok(Self::Month),
            "year" | "yearly" => Ok(Self::Year),
            "raw" | "date" => Ok(Self::Raw),
            other => err(format!(
                "--date-bucket must be day, month, year, or raw; got {other}"
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
            Self::Raw => "raw",
        }
    }
}

pub(crate) fn moment_response_has_next(data: &Value) -> bool {
    data.get("has_next")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn moment_sections_from_matches(
    matches: &ArgMatches,
    flag: &str,
    default_labels: &[&str],
) -> Result<Vec<&'static SnapshotMomentRoute>> {
    let raw = split_list_values(&collect_values(matches, flag));
    if raw.is_empty() {
        return default_labels
            .iter()
            .map(|label| {
                snapshot_moment_route_by_label(label)
                    .ok_or_else(|| miette!("unknown built-in moment section {label}"))
            })
            .collect();
    }

    let mut seen = BTreeSet::new();
    let mut routes = Vec::new();
    for value in raw {
        if normalize_activity_section(&value) == "all" {
            for label in ALL_MOMENT_SECTIONS {
                let route = snapshot_moment_route_by_label(label)
                    .ok_or_else(|| miette!("unknown built-in moment section {label}"))?;
                if seen.insert(route.label) {
                    routes.push(route);
                }
            }
            continue;
        }
        let label = contact_activity_section_label(&value, flag)?;
        let route = snapshot_moment_route_by_label(label)
            .ok_or_else(|| miette!("unknown moment section {value}"))?;
        if seen.insert(route.label) {
            routes.push(route);
        }
    }
    if routes.is_empty() {
        return err(format!("provide at least one --{flag} value"));
    }
    Ok(routes)
}

pub(crate) fn moments_feed_dry_run_plan(options: &MomentsFeedOptions) -> Value {
    json!({
        "source": "live",
        "filters": moments_feed_filters(options),
        "plan": [
            {
                "routes": options.sections.iter().map(|route| json!({
                    "label": route.label,
                    "route": format!("/tools/v2{}", route.route),
                    "kind": contact_activity_kind(route.kind),
                    "payload": Value::Object(moments_feed_moment_plan_payload(options, route)),
                    "purpose": "fetch live moment rows without writes",
                })).collect::<Vec<_>>()
            },
            {
                "local": "feed",
                "sort": options.sort.as_str(),
                "item_limit": options.item_limit,
                "purpose": "flatten fetched moment rows into a timeline for table, CSV, TSV, JSONL, or --flat output",
            }
        ],
    })
}

pub(crate) fn moments_stats_dry_run_plan(options: &MomentsStatsOptions) -> Value {
    json!({
        "source": "live",
        "filters": moments_stats_filters(options),
        "options": moments_stats_options_value(options),
        "plan": [
            {
                "routes": options.feed.sections.iter().map(|route| json!({
                    "label": route.label,
                    "route": format!("/tools/v2{}", route.route),
                    "kind": contact_activity_kind(route.kind),
                    "payload": Value::Object(moments_feed_moment_plan_payload(&options.feed, route)),
                    "purpose": "fetch live moment rows without writes",
                })).collect::<Vec<_>>()
            },
            {
                "local": "moments:stats",
                "purpose": "aggregate section counts, contact buckets, and date buckets from fetched rows",
            }
        ],
    })
}

pub(crate) async fn moments_feed(runtime: &Runtime, options: MomentsFeedOptions) -> Result<Value> {
    let mut moments = Map::new();
    let mut counts = Map::new();
    let mut total_rows = 0_usize;
    for route in &options.sections {
        let section = moments_feed_fetch_route(runtime, &options, route).await?;
        let count = section
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        total_rows += count as usize;
        counts.insert(route.label.to_string(), Value::Number(Number::from(count)));
        moments.insert(route.label.to_string(), section);
    }
    let feed = moments_feed_flat_rows_from_moments(
        &moments,
        &options.sections,
        options.sort,
        options.item_limit,
    );

    Ok(json!({
        "source": "live",
        "filters": moments_feed_filters(&options),
        "summary": {
            "section_count": options.sections.len(),
            "activity_count": total_rows,
            "feed_count": feed.len(),
            "counts": counts,
        },
        "moments": moments,
        "feed": feed,
    }))
}

pub(crate) async fn moments_stats(
    runtime: &Runtime,
    options: MomentsStatsOptions,
) -> Result<Value> {
    let mut moments = Map::new();
    let mut total_rows = 0_u64;
    for route in &options.feed.sections {
        let section = moments_feed_fetch_route(runtime, &options.feed, route).await?;
        total_rows += section
            .get("count")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        moments.insert(route.label.to_string(), section);
    }

    let feed = moments_feed_flat_rows_from_moments(
        &moments,
        &options.feed.sections,
        MomentsFeedSort::None,
        None,
    );
    let sections = moments_stats_section_rows(&moments, &options.feed.sections);
    let top_contacts = moments_stats_contact_rows(&feed, options.top_contacts);
    let date_buckets = moments_stats_date_rows(&feed, options.date_bucket, options.top_dates);
    let dated_count = feed
        .iter()
        .filter(|row| moments_feed_row_string(row, "date").is_some())
        .count() as u64;
    let undated_count = feed.len() as u64 - dated_count;
    let contact_bucket_count = moments_stats_contact_bucket_count(&feed);
    let date_bucket_count = moments_stats_date_bucket_count(&feed, options.date_bucket);

    Ok(json!({
        "source": "live",
        "filters": moments_stats_filters(&options),
        "options": moments_stats_options_value(&options),
        "summary": {
            "section_count": options.feed.sections.len(),
            "activity_count": total_rows,
            "feed_count": feed.len(),
            "dated_count": dated_count,
            "undated_count": undated_count,
            "contact_bucket_count": contact_bucket_count,
            "date_bucket_count": date_bucket_count,
            "returned_contact_count": top_contacts.len(),
            "returned_date_bucket_count": date_buckets.len(),
        },
        "sections": sections,
        "top_contacts": top_contacts,
        "date_buckets": date_buckets,
    }))
}

pub(crate) async fn moments_feed_fetch_route(
    runtime: &Runtime,
    options: &MomentsFeedOptions,
    route: &SnapshotMomentRoute,
) -> Result<Value> {
    let mut rows = Vec::new();
    let mut pages = 0_usize;

    match route.kind {
        SnapshotMomentKind::DateWindow => {
            let payload = moments_feed_moment_date_payload(options)?;
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
                let payload = moments_feed_moment_paged_payload(options, page);
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

pub(crate) fn moments_feed_moment_date_payload(
    options: &MomentsFeedOptions,
) -> Result<Map<String, Value>> {
    let start = options
        .start
        .as_ref()
        .ok_or_else(|| miette!("missing --start"))?;
    let end = options
        .end
        .as_ref()
        .ok_or_else(|| miette!("missing --end"))?;
    let mut payload = moments_feed_moment_base_payload(options);
    payload.set("start", start.clone());
    payload.set("end", end.clone());
    Ok(payload)
}

pub(crate) fn moments_feed_moment_paged_payload(
    options: &MomentsFeedOptions,
    page: usize,
) -> Map<String, Value> {
    let mut payload = moments_feed_moment_base_payload(options);
    payload.insert(
        "limit".to_string(),
        Value::Number(Number::from(options.limit as u64)),
    );
    payload.set("page", page as u64);
    payload
}

pub(crate) fn moments_feed_moment_plan_payload(
    options: &MomentsFeedOptions,
    route: &SnapshotMomentRoute,
) -> Map<String, Value> {
    let mut payload = moments_feed_moment_base_payload(options);
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

pub(crate) fn moments_feed_moment_base_payload(options: &MomentsFeedOptions) -> Map<String, Value> {
    let mut payload = Map::new();
    if !options.contact_ids.is_empty() {
        payload.set("contact_ids", json!(options.contact_ids));
    }
    payload
}

pub(crate) fn moments_feed_filters(options: &MomentsFeedOptions) -> Value {
    json!({
        "contact_ids": options.contact_ids.clone(),
        "sections": contact_activity_section_labels(&options.sections),
        "start": options.start.clone(),
        "end": options.end.clone(),
        "limit": options.limit,
        "item_limit": options.item_limit,
        "sort": options.sort.as_str(),
    })
}

pub(crate) fn moments_stats_filters(options: &MomentsStatsOptions) -> Value {
    json!({
        "contact_ids": options.feed.contact_ids.clone(),
        "sections": contact_activity_section_labels(&options.feed.sections),
        "start": options.feed.start.clone(),
        "end": options.feed.end.clone(),
        "limit": options.feed.limit,
    })
}

pub(crate) fn moments_feed_flat_rows_from_moments(
    moments: &Map<String, Value>,
    sections: &[&'static SnapshotMomentRoute],
    sort: MomentsFeedSort,
    item_limit: Option<usize>,
) -> Vec<Value> {
    let mut rows = Vec::new();
    for route in sections {
        let Some(data) = moments.get(route.label) else {
            continue;
        };
        let route_value = data.get("route").cloned().unwrap_or(Value::Null);
        let section_count = data.get("count").cloned().unwrap_or(Value::Null);
        let pages = data.get("pages").cloned().unwrap_or(Value::Null);
        let items = data
            .get("rows")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in items {
            let contact_id = contact_activity_row_contact_id(&item);
            rows.push(json!({
                "section": route.label,
                "route": route_value.clone(),
                "section_count": section_count.clone(),
                "pages": pages.clone(),
                "activity_id": record_id(&item).map(Value::String).unwrap_or(Value::Null),
                "contact_id": contact_id.map(|id| Value::Number(Number::from(id))).unwrap_or(Value::Null),
                "contact_name": contact_activity_row_contact_name(&item).map(Value::String).unwrap_or(Value::Null),
                "date": contact_activity_row_date(&item).map(Value::String).unwrap_or(Value::Null),
                "title": contact_activity_row_title(&item).map(Value::String).unwrap_or(Value::Null),
                "summary": contact_activity_row_summary(&item).map(Value::String).unwrap_or(Value::Null),
            }));
        }
    }
    moments_feed_sort_rows(&mut rows, sort);
    if let Some(limit) = item_limit {
        rows.truncate(limit);
    }
    rows
}

pub(crate) fn moments_feed_sort_rows(rows: &mut [Value], sort: MomentsFeedSort) {
    if sort == MomentsFeedSort::None {
        return;
    }
    rows.sort_by(|left, right| {
        let left_date = moments_feed_row_string(left, "date");
        let right_date = moments_feed_row_string(right, "date");
        let date_order = match (left_date.as_deref(), right_date.as_deref()) {
            (Some(left_date), Some(right_date)) => match sort {
                MomentsFeedSort::Asc => left_date.cmp(right_date),
                MomentsFeedSort::Desc => right_date.cmp(left_date),
                MomentsFeedSort::None => std::cmp::Ordering::Equal,
            },
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        date_order.then_with(|| moments_feed_tiebreaker(left).cmp(&moments_feed_tiebreaker(right)))
    });
}

pub(crate) fn moments_feed_row_string(row: &Value, key: &str) -> Option<String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn moments_feed_tiebreaker(row: &Value) -> String {
    [
        moments_feed_row_string(row, "section"),
        moments_feed_row_string(row, "contact_name"),
        moments_feed_row_string(row, "title"),
        moments_feed_row_string(row, "summary"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\u{1f}")
}

pub(crate) fn moments_feed_output_rows(report: &Value) -> Value {
    report
        .get("feed")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()))
}

pub(crate) fn moments_stats_options_value(options: &MomentsStatsOptions) -> Value {
    json!({
        "date_bucket": options.date_bucket.as_str(),
        "top_contacts": options.top_contacts,
        "top_dates": options.top_dates,
    })
}

pub(crate) fn moments_stats_section_rows(
    moments: &Map<String, Value>,
    sections: &[&'static SnapshotMomentRoute],
) -> Vec<Value> {
    let mut rows = Vec::new();
    for route in sections {
        let Some(data) = moments.get(route.label) else {
            continue;
        };
        let mut bucket = MomentsStatsSectionBucket {
            section: route.label.to_string(),
            route: data.get("route").cloned().unwrap_or(Value::Null),
            pages: data.get("pages").cloned().unwrap_or(Value::Null),
            count: data
                .get("count")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            ..Default::default()
        };
        for item in data
            .get("rows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(date) = contact_activity_row_date(item) {
                bucket.dated_count += 1;
                moments_stats_update_date_bounds(
                    &mut bucket.first_date,
                    &mut bucket.last_date,
                    &date,
                );
            } else {
                bucket.undated_count += 1;
            }
            bucket
                .contact_keys
                .insert(moments_stats_contact_key_from_item(item));
        }
        rows.push(json!({
            "section": bucket.section,
            "route": bucket.route,
            "pages": bucket.pages,
            "count": bucket.count,
            "dated_count": bucket.dated_count,
            "undated_count": bucket.undated_count,
            "contact_bucket_count": bucket.contact_keys.len(),
            "first_date": bucket.first_date.map(Value::String).unwrap_or(Value::Null),
            "last_date": bucket.last_date.map(Value::String).unwrap_or(Value::Null),
        }));
    }
    rows
}

pub(crate) fn moments_stats_contact_rows(feed: &[Value], limit: usize) -> Vec<Value> {
    let mut buckets: BTreeMap<String, MomentsStatsContactBucket> = BTreeMap::new();
    for row in feed {
        let contact_id = row.get("contact_id").and_then(Value::as_u64);
        let contact_name = moments_feed_row_string(row, "contact_name");
        let key = moments_stats_contact_key(contact_id, contact_name.as_deref());
        let bucket = buckets.entry(key).or_default();
        bucket.count += 1;
        if bucket.contact_id.is_none() {
            bucket.contact_id = contact_id;
        }
        if bucket.contact_name.is_none() {
            bucket.contact_name = contact_name;
        }
        if let Some(section) = moments_feed_row_string(row, "section") {
            bucket.sections.insert(section);
        }
        if let Some(date) = moments_feed_row_string(row, "date")
            && bucket.latest_date.as_deref().unwrap_or_default() < date.as_str()
        {
            bucket.latest_date = Some(date);
            bucket.latest_summary = moments_feed_row_string(row, "summary");
        }
    }

    let mut rows = buckets
        .into_iter()
        .map(|(key, bucket)| {
            (
                bucket.count,
                moments_stats_contact_sort_key(&bucket, &key),
                key,
                bucket,
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    rows.truncate(limit);
    rows.into_iter()
        .map(|(count, _, key, bucket)| {
            json!({
                "contact_key": key,
                "contact_id": bucket.contact_id.map(|id| Value::Number(Number::from(id))).unwrap_or(Value::Null),
                "contact_name": bucket.contact_name.map(Value::String).unwrap_or(Value::Null),
                "count": count,
                "sections": bucket.sections.into_iter().collect::<Vec<_>>(),
                "latest_date": bucket.latest_date.map(Value::String).unwrap_or(Value::Null),
                "latest_summary": bucket.latest_summary.map(Value::String).unwrap_or(Value::Null),
            })
        })
        .collect()
}

pub(crate) fn moments_stats_date_rows(
    feed: &[Value],
    date_bucket: MomentsStatsDateBucket,
    limit: usize,
) -> Vec<Value> {
    let mut buckets: BTreeMap<String, MomentsStatsDateBucketRow> = BTreeMap::new();
    for row in feed {
        let Some(date) = moments_feed_row_string(row, "date") else {
            continue;
        };
        let key = moments_stats_date_bucket(&date, date_bucket);
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| MomentsStatsDateBucketRow {
                bucket: key,
                ..Default::default()
            });
        bucket.count += 1;
        if let Some(section) = moments_feed_row_string(row, "section") {
            bucket.sections.insert(section);
        }
    }

    let mut rows = buckets
        .into_iter()
        .map(|(key, bucket)| (bucket.count, key, bucket))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    rows.truncate(limit);
    rows.into_iter()
        .map(|(_, _, bucket)| {
            json!({
                "bucket": bucket.bucket,
                "count": bucket.count,
                "sections": bucket.sections.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub(crate) fn moments_stats_contact_bucket_count(feed: &[Value]) -> usize {
    feed.iter()
        .map(|row| {
            moments_stats_contact_key(
                row.get("contact_id").and_then(Value::as_u64),
                moments_feed_row_string(row, "contact_name").as_deref(),
            )
        })
        .collect::<BTreeSet<_>>()
        .len()
}

pub(crate) fn moments_stats_date_bucket_count(
    feed: &[Value],
    date_bucket: MomentsStatsDateBucket,
) -> usize {
    feed.iter()
        .filter_map(|row| moments_feed_row_string(row, "date"))
        .map(|date| moments_stats_date_bucket(&date, date_bucket))
        .collect::<BTreeSet<_>>()
        .len()
}

pub(crate) fn moments_stats_update_date_bounds(
    first_date: &mut Option<String>,
    last_date: &mut Option<String>,
    date: &str,
) {
    if first_date.as_deref().is_none_or(|existing| date < existing) {
        *first_date = Some(date.to_string());
    }
    if last_date.as_deref().is_none_or(|existing| date > existing) {
        *last_date = Some(date.to_string());
    }
}

pub(crate) fn moments_stats_contact_key_from_item(item: &Value) -> String {
    moments_stats_contact_key(
        contact_activity_row_contact_id(item),
        contact_activity_row_contact_name(item).as_deref(),
    )
}

pub(crate) fn moments_stats_contact_key(
    contact_id: Option<u64>,
    contact_name: Option<&str>,
) -> String {
    if let Some(id) = contact_id {
        return format!("id:{id}");
    }
    let name = single_line(contact_name.unwrap_or_default());
    if name.is_empty() {
        "(unknown)".to_string()
    } else {
        format!("name:{}", name.to_lowercase())
    }
}

pub(crate) fn moments_stats_contact_sort_key(
    bucket: &MomentsStatsContactBucket,
    key: &str,
) -> String {
    [
        bucket.latest_date.clone().unwrap_or_default(),
        bucket.contact_name.clone().unwrap_or_default(),
        key.to_string(),
    ]
    .join("\u{1f}")
}

pub(crate) fn moments_stats_date_bucket(date: &str, bucket: MomentsStatsDateBucket) -> String {
    let trimmed = date.trim();
    match bucket {
        MomentsStatsDateBucket::Raw => trimmed.to_string(),
        MomentsStatsDateBucket::Day => trimmed.chars().take(10).collect(),
        MomentsStatsDateBucket::Month => trimmed.chars().take(7).collect(),
        MomentsStatsDateBucket::Year => trimmed.chars().take(4).collect(),
    }
}

pub(crate) fn moments_stats_flat_rows(report: &Value) -> Value {
    let mut rows = Vec::new();
    let source = report.get("source").cloned().unwrap_or(Value::Null);
    if let Some(sections) = report.get("sections").and_then(Value::as_array) {
        for section in sections {
            rows.push(json!({
                "row_type": "section",
                "source": source.clone(),
                "section": section.get("section").cloned().unwrap_or(Value::Null),
                "route": section.get("route").cloned().unwrap_or(Value::Null),
                "count": section.get("count").cloned().unwrap_or(Value::Null),
                "dated_count": section.get("dated_count").cloned().unwrap_or(Value::Null),
                "undated_count": section.get("undated_count").cloned().unwrap_or(Value::Null),
                "contact_bucket_count": section.get("contact_bucket_count").cloned().unwrap_or(Value::Null),
                "first_date": section.get("first_date").cloned().unwrap_or(Value::Null),
                "last_date": section.get("last_date").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    if let Some(contacts) = report.get("top_contacts").and_then(Value::as_array) {
        for contact in contacts {
            rows.push(json!({
                "row_type": "top_contact",
                "source": source.clone(),
                "contact_key": contact.get("contact_key").cloned().unwrap_or(Value::Null),
                "contact_id": contact.get("contact_id").cloned().unwrap_or(Value::Null),
                "contact_name": contact.get("contact_name").cloned().unwrap_or(Value::Null),
                "count": contact.get("count").cloned().unwrap_or(Value::Null),
                "sections": moments_stats_sections_cell(contact.get("sections")),
                "latest_date": contact.get("latest_date").cloned().unwrap_or(Value::Null),
                "latest_summary": contact.get("latest_summary").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    if let Some(date_buckets) = report.get("date_buckets").and_then(Value::as_array) {
        for bucket in date_buckets {
            rows.push(json!({
                "row_type": "date_bucket",
                "source": source.clone(),
                "date_bucket": bucket.get("bucket").cloned().unwrap_or(Value::Null),
                "count": bucket.get("count").cloned().unwrap_or(Value::Null),
                "sections": moments_stats_sections_cell(bucket.get("sections")),
            }));
        }
    }
    Value::Array(rows)
}

pub(crate) fn moments_stats_sections_cell(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn moments_timeline_dry_run_plan(options: &MomentsTimelineOptions) -> Value {
    let filters = moments_timeline_filters(options);
    let options_value = moments_timeline_options_value(options);
    if let Some(dir) = &options.snapshot_dir {
        json!({
            "source": "snapshot",
            "filters": filters,
            "options": options_value,
            "plan": [
                {"local_file": dir.join("manifest.json").display().to_string(), "enabled": options.verify, "purpose": "verify snapshot hashes before reading moment sections"},
                {"local_files": options.feed.sections.iter().map(|route| dir.join(route.file_name).display().to_string()).collect::<Vec<_>>(), "purpose": "read selected snapshot moment JSONL sections when present"},
                {"local": "moments:timeline", "purpose": "filter, sort, bucket, and sample activity rows without me.sh writes"}
            ],
        })
    } else {
        json!({
            "source": "live",
            "filters": filters,
            "options": options_value,
            "plan": [
                {
                    "routes": options.feed.sections.iter().map(|route| json!({
                        "label": route.label,
                        "route": format!("/tools/v2{}", route.route),
                        "kind": contact_activity_kind(route.kind),
                        "payload": Value::Object(moments_feed_moment_plan_payload(&options.feed, route)),
                        "purpose": "fetch live moment rows without writes",
                    })).collect::<Vec<_>>()
                },
                {"local": "moments:timeline", "purpose": "filter, sort, bucket, and sample activity rows without me.sh writes"}
            ],
        })
    }
}

pub(crate) async fn moments_timeline(
    runtime: &Runtime,
    options: MomentsTimelineOptions,
) -> Result<Value> {
    let (source, moments, missing_sections) = if let Some(dir) = &options.snapshot_dir {
        moments_timeline_snapshot_moments(dir, &options)?
    } else {
        let mut feed_options = options.feed.clone();
        feed_options.item_limit = None;
        let report = moments_feed(runtime, feed_options).await?;
        (
            json!({"type": "live"}),
            report
                .get("moments")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            Vec::new(),
        )
    };

    let mut feed = moments_feed_flat_rows_from_moments(
        &moments,
        &options.feed.sections,
        MomentsFeedSort::None,
        None,
    );
    moments_timeline_filter_sort_limit_feed(&mut feed, &options);
    moments_timeline_annotate_feed(&mut feed, options.date_bucket);
    let total_bucket_count = moments_timeline_bucket_count(&feed);
    let buckets = moments_timeline_buckets(&feed, &options);
    let dated_count = feed
        .iter()
        .filter(|row| moments_feed_row_string(row, "date").is_some())
        .count() as u64;
    let undated_count = feed.len() as u64 - dated_count;

    Ok(json!({
        "source": source,
        "filters": moments_timeline_filters(&options),
        "options": moments_timeline_options_value(&options),
        "summary": {
            "section_count": options.feed.sections.len(),
            "feed_count": feed.len(),
            "bucket_count": total_bucket_count,
            "returned_bucket_count": buckets.len(),
            "dated_count": dated_count,
            "undated_count": undated_count,
            "contact_bucket_count": moments_stats_contact_bucket_count(&feed),
            "missing_section_count": missing_sections.len(),
            "counts": moments_timeline_section_counts(&feed),
        },
        "missing_sections": missing_sections,
        "buckets": buckets,
        "feed": feed,
    }))
}

pub(crate) fn moments_timeline_snapshot_moments(
    dir: &Path,
    options: &MomentsTimelineOptions,
) -> Result<(Value, Map<String, Value>, Vec<Value>)> {
    if options.verify {
        let verify = verify_snapshot(dir)?;
        if !verify.get("ok").and_then(Value::as_bool).unwrap_or(false) {
            return err("snapshot failed manifest verification");
        }
    }

    let mut moments = Map::new();
    let mut missing_sections = Vec::new();
    for route in &options.feed.sections {
        if !snapshot_manifest_has_file(dir, route.label)? {
            missing_sections.push(json!({
                "section": route.label,
                "file": route.file_name,
                "error": "snapshot does not contain this moment section",
            }));
            moments.insert(
                route.label.to_string(),
                json!({
                    "label": route.label,
                    "route": route.route,
                    "kind": contact_activity_kind(route.kind),
                    "pages": 0,
                    "count": 0,
                    "rows": [],
                }),
            );
            continue;
        }
        let path = snapshot_manifest_file_path(dir, route.label)?;
        let rows = read_snapshot_jsonl_values_at_path(&path)?;
        moments.insert(
            route.label.to_string(),
            json!({
                "label": route.label,
                "route": route.route,
                "kind": contact_activity_kind(route.kind),
                "pages": 1,
                "count": rows.len(),
                "rows": rows,
            }),
        );
    }
    Ok((
        json!({
            "type": "snapshot",
            "dir": dir.display().to_string(),
            "verified": options.verify,
        }),
        moments,
        missing_sections,
    ))
}

pub(crate) fn moments_timeline_filter_sort_limit_feed(
    feed: &mut Vec<Value>,
    options: &MomentsTimelineOptions,
) {
    let contact_ids = options
        .feed
        .contact_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let start = options.feed.start.as_deref();
    let end = options.feed.end.as_deref();
    feed.retain(|row| {
        if !contact_ids.is_empty()
            && !row
                .get("contact_id")
                .and_then(Value::as_u64)
                .is_some_and(|id| contact_ids.contains(&id))
        {
            return false;
        }
        if let (Some(start), Some(end)) = (start, end) {
            let Some(date) = moments_feed_row_string(row, "date") else {
                return false;
            };
            if !moments_timeline_date_in_range(&date, start, end) {
                return false;
            }
        }
        true
    });
    moments_feed_sort_rows(feed, options.feed.sort);
    if let Some(limit) = options.feed.item_limit {
        feed.truncate(limit);
    }
}

pub(crate) fn moments_timeline_date_in_range(date: &str, start: &str, end: &str) -> bool {
    if start.len() == 10 && end.len() == 10 {
        let day = date.chars().take(10).collect::<String>();
        return day.as_str() >= start && day.as_str() <= end;
    }
    date >= start && date <= end
}

pub(crate) fn moments_timeline_annotate_feed(
    feed: &mut [Value],
    date_bucket: MomentsStatsDateBucket,
) {
    for row in feed {
        let bucket = moments_feed_row_string(row, "date")
            .map(|date| moments_stats_date_bucket(&date, date_bucket))
            .unwrap_or_else(|| "undated".to_string());
        if let Some(object) = row.as_object_mut() {
            object.set("date_bucket", bucket);
        }
    }
}

pub(crate) fn moments_timeline_bucket_count(feed: &[Value]) -> usize {
    feed.iter()
        .filter_map(|row| moments_feed_row_string(row, "date_bucket"))
        .collect::<BTreeSet<_>>()
        .len()
}

pub(crate) fn moments_timeline_buckets(
    feed: &[Value],
    options: &MomentsTimelineOptions,
) -> Vec<Value> {
    let mut buckets: BTreeMap<String, MomentsTimelineBucket> = BTreeMap::new();
    for row in feed {
        let bucket_key =
            moments_feed_row_string(row, "date_bucket").unwrap_or_else(|| "undated".to_string());
        let bucket = buckets
            .entry(bucket_key.clone())
            .or_insert_with(|| MomentsTimelineBucket {
                bucket: bucket_key,
                ..Default::default()
            });
        bucket.count += 1;
        if let Some(section) = moments_feed_row_string(row, "section") {
            bucket.sections.insert(section);
        }
        bucket.contact_keys.insert(moments_stats_contact_key(
            row.get("contact_id").and_then(Value::as_u64),
            moments_feed_row_string(row, "contact_name").as_deref(),
        ));
        if let Some(date) = moments_feed_row_string(row, "date") {
            bucket.dated_count += 1;
            moments_stats_update_date_bounds(&mut bucket.first_date, &mut bucket.last_date, &date);
        } else {
            bucket.undated_count += 1;
        }
        if bucket.items.len() < options.items_per_bucket {
            bucket.items.push(row.clone());
        }
    }

    let mut buckets = buckets.into_values().collect::<Vec<_>>();
    moments_timeline_sort_buckets(&mut buckets, options.feed.sort);
    buckets.truncate(options.bucket_limit);
    buckets
        .into_iter()
        .map(|bucket| {
            json!({
                "bucket": bucket.bucket,
                "count": bucket.count,
                "dated_count": bucket.dated_count,
                "undated_count": bucket.undated_count,
                "sections": bucket.sections.into_iter().collect::<Vec<_>>(),
                "contact_bucket_count": bucket.contact_keys.len(),
                "first_date": bucket.first_date.map(Value::String).unwrap_or(Value::Null),
                "last_date": bucket.last_date.map(Value::String).unwrap_or(Value::Null),
                "items": bucket.items,
            })
        })
        .collect()
}

pub(crate) fn moments_timeline_sort_buckets(
    buckets: &mut [MomentsTimelineBucket],
    sort: MomentsFeedSort,
) {
    buckets.sort_by(|left, right| {
        let left_undated = left.bucket == "undated";
        let right_undated = right.bucket == "undated";
        match (left_undated, right_undated) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }
        let bucket_order = match sort {
            MomentsFeedSort::Asc | MomentsFeedSort::None => left.bucket.cmp(&right.bucket),
            MomentsFeedSort::Desc => right.bucket.cmp(&left.bucket),
        };
        bucket_order
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left.bucket.cmp(&right.bucket))
    });
}

pub(crate) fn moments_timeline_section_counts(feed: &[Value]) -> Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for row in feed {
        if let Some(section) = moments_feed_row_string(row, "section") {
            *counts.entry(section).or_default() += 1;
        }
    }
    Value::Object(
        counts
            .into_iter()
            .map(|(section, count)| (section, Value::Number(Number::from(count))))
            .collect(),
    )
}

pub(crate) fn moments_timeline_filters(options: &MomentsTimelineOptions) -> Value {
    json!({
        "source": if options.snapshot_dir.is_some() { "snapshot" } else { "live" },
        "snapshot_dir": options.snapshot_dir.as_ref().map(|dir| dir.display().to_string()),
        "contact_ids": options.feed.contact_ids.clone(),
        "sections": contact_activity_section_labels(&options.feed.sections),
        "start": options.feed.start.clone(),
        "end": options.feed.end.clone(),
        "limit": options.feed.limit,
        "item_limit": options.feed.item_limit,
        "sort": options.feed.sort.as_str(),
    })
}

pub(crate) fn moments_timeline_options_value(options: &MomentsTimelineOptions) -> Value {
    json!({
        "date_bucket": options.date_bucket.as_str(),
        "bucket_limit": options.bucket_limit,
        "items_per_bucket": options.items_per_bucket,
        "verify": options.verify,
    })
}

pub(crate) fn moments_timeline_flat_rows(report: &Value) -> Value {
    let source = report
        .get("source")
        .and_then(|source| source.get("type"))
        .cloned()
        .or_else(|| report.get("source").cloned())
        .unwrap_or(Value::Null);
    let mut bucket_counts = BTreeMap::new();
    if let Some(buckets) = report.get("buckets").and_then(Value::as_array) {
        for bucket in buckets {
            if let Some(name) = moments_feed_row_string(bucket, "bucket") {
                bucket_counts.insert(name, bucket.get("count").cloned().unwrap_or(Value::Null));
            }
        }
    }
    let Some(feed) = report.get("feed").and_then(Value::as_array) else {
        return Value::Array(Vec::new());
    };
    Value::Array(
        feed.iter()
            .map(|row| {
                let bucket = row.get("date_bucket").cloned().unwrap_or(Value::Null);
                let bucket_count = bucket
                    .as_str()
                    .and_then(|key| bucket_counts.get(key))
                    .cloned()
                    .unwrap_or(Value::Null);
                json!({
                    "source": source.clone(),
                    "date_bucket": bucket,
                    "bucket_count": bucket_count,
                    "section": row.get("section").cloned().unwrap_or(Value::Null),
                    "route": row.get("route").cloned().unwrap_or(Value::Null),
                    "activity_id": row.get("activity_id").cloned().unwrap_or(Value::Null),
                    "contact_id": row.get("contact_id").cloned().unwrap_or(Value::Null),
                    "contact_name": row.get("contact_name").cloned().unwrap_or(Value::Null),
                    "date": row.get("date").cloned().unwrap_or(Value::Null),
                    "title": row.get("title").cloned().unwrap_or(Value::Null),
                    "summary": row.get("summary").cloned().unwrap_or(Value::Null),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(label: &str) -> &'static SnapshotMomentRoute {
        snapshot_moment_route_by_label(label).expect("test route exists")
    }

    fn feed_options(contact_ids: Vec<u64>) -> MomentsFeedOptions {
        MomentsFeedOptions {
            contact_ids,
            sections: vec![route("notes"), route("events_upcoming")],
            start: Some("2024-01-01".to_string()),
            end: Some("2024-01-31".to_string()),
            limit: 25,
            item_limit: None,
            sort: MomentsFeedSort::Desc,
            flat: false,
        }
    }

    fn timeline_options(feed: MomentsFeedOptions) -> MomentsTimelineOptions {
        MomentsTimelineOptions {
            feed,
            snapshot_dir: None,
            date_bucket: MomentsStatsDateBucket::Day,
            bucket_limit: 10,
            items_per_bucket: 2,
            verify: true,
            flat: false,
        }
    }

    fn temp_snapshot_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meshx-moments-{label}-{}-{}",
            std::process::id(),
            now_millis()
        ))
    }

    fn write_moment_snapshot_section(
        dir: &Path,
        label: &str,
        path: &str,
        content: &str,
    ) -> Result<()> {
        let file_path = safe_snapshot_file_path(dir, path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).into_diagnostic()?;
        }
        fs::write(file_path, content).into_diagnostic()?;
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

    fn sample_moments() -> Map<String, Value> {
        let mut moments = Map::new();
        moments.insert(
            "notes".to_string(),
            json!({
                "route": "/moments/notes",
                "pages": 1,
                "count": 2,
                "rows": [
                    {
                        "id": "old",
                        "contact_id": 1,
                        "contact_name": "Ada",
                        "date": "2024-01-01",
                        "title": "Old note",
                        "summary": "older"
                    },
                    {
                        "id": "new",
                        "contact_id": 2,
                        "contact_name": "Grace",
                        "start": "2024-01-03T09:00:00Z",
                        "subject": "New event",
                        "body": "newer"
                    }
                ]
            }),
        );
        moments.insert(
            "events_upcoming".to_string(),
            json!({
                "route": "/moments/events/upcoming",
                "pages": 1,
                "count": 1,
                "rows": [
                    {
                        "id": "undated",
                        "contact_id": 3,
                        "contact_name": "Lin",
                        "title": "No date",
                        "summary": "undated"
                    }
                ]
            }),
        );
        moments
    }

    #[test]
    fn moments_timeline_snapshot_moments_reads_sections_from_manifest_paths() -> Result<()> {
        let dir = temp_snapshot_dir("manifest-path");
        let content = "{\"id\":\"n1\",\"contact_id\":7,\"title\":\"nested\"}\n";
        write_moment_snapshot_section(&dir, "notes", "data/notes.jsonl", content)?;
        let mut options = timeline_options(feed_options(Vec::new()));
        options.feed.sections = vec![route("notes")];

        let (_, moments, missing_sections) = moments_timeline_snapshot_moments(&dir, &options)?;

        fs::remove_dir_all(&dir).ok();
        assert!(missing_sections.is_empty());
        assert_eq!(moments["notes"].get("count"), Some(&json!(1)));
        assert_eq!(
            moments["notes"].pointer("/rows/0/title"),
            Some(&json!("nested"))
        );
        Ok(())
    }

    #[test]
    fn moments_feed_base_payload_includes_contact_ids_only_when_present() {
        assert_eq!(
            Value::Object(moments_feed_moment_base_payload(&feed_options(vec![42, 7]))),
            json!({"contact_ids": [42, 7]})
        );
        assert_eq!(
            Value::Object(moments_feed_moment_base_payload(&feed_options(Vec::new()))),
            json!({})
        );
    }

    #[test]
    fn moments_feed_plan_payload_adds_window_or_paging_fields() {
        let options = feed_options(vec![42]);

        assert_eq!(
            Value::Object(moments_feed_moment_plan_payload(&options, route("notes"))),
            json!({
                "contact_ids": [42],
                "start": "2024-01-01",
                "end": "2024-01-31",
            })
        );
        assert_eq!(
            Value::Object(moments_feed_moment_plan_payload(
                &options,
                route("events_upcoming")
            )),
            json!({
                "contact_ids": [42],
                "limit": 25,
                "page": "1..has_next",
            })
        );
    }

    #[test]
    fn moments_feed_flat_rows_sort_and_limit_dated_rows_before_undated() {
        let rows = moments_feed_flat_rows_from_moments(
            &sample_moments(),
            &[route("notes"), route("events_upcoming")],
            MomentsFeedSort::Desc,
            Some(2),
        );

        assert_eq!(rows.len(), 2);
        assert_eq!(
            moments_feed_row_string(&rows[0], "activity_id").as_deref(),
            Some("new")
        );
        assert_eq!(
            moments_feed_row_string(&rows[0], "date").as_deref(),
            Some("2024-01-03T09:00:00Z")
        );
        assert_eq!(
            moments_feed_row_string(&rows[1], "activity_id").as_deref(),
            Some("old")
        );
    }

    #[test]
    fn moments_stats_rows_bucket_contacts_and_dates() {
        let feed = moments_feed_flat_rows_from_moments(
            &sample_moments(),
            &[route("notes"), route("events_upcoming")],
            MomentsFeedSort::None,
            None,
        );

        let contacts = moments_stats_contact_rows(&feed, 10);
        let dates = moments_stats_date_rows(&feed, MomentsStatsDateBucket::Day, 10);

        assert_eq!(moments_stats_contact_bucket_count(&feed), 3);
        assert_eq!(
            moments_stats_date_bucket_count(&feed, MomentsStatsDateBucket::Day),
            2
        );
        assert_eq!(
            contacts
                .iter()
                .filter_map(|row| moments_feed_row_string(row, "contact_key"))
                .collect::<Vec<_>>(),
            vec!["id:2", "id:1", "id:3"]
        );
        assert_eq!(
            dates
                .iter()
                .filter_map(|row| moments_feed_row_string(row, "bucket"))
                .collect::<Vec<_>>(),
            vec!["2024-01-03", "2024-01-01"]
        );
    }

    #[test]
    fn moments_stats_contact_key_folds_unicode_contact_names() {
        assert_eq!(
            moments_stats_contact_key(None, Some("MÜNCHEN")),
            moments_stats_contact_key(None, Some("münchen"))
        );
    }

    #[test]
    fn moments_timeline_filter_sort_limit_feed_filters_contact_and_date_range() {
        let mut feed = moments_feed_flat_rows_from_moments(
            &sample_moments(),
            &[route("notes"), route("events_upcoming")],
            MomentsFeedSort::None,
            None,
        );
        let mut options = timeline_options(feed_options(vec![2]));
        options.feed.item_limit = Some(5);

        moments_timeline_filter_sort_limit_feed(&mut feed, &options);
        moments_timeline_annotate_feed(&mut feed, MomentsStatsDateBucket::Day);

        assert_eq!(feed.len(), 1);
        assert_eq!(feed[0].get("contact_id").and_then(Value::as_u64), Some(2));
        assert_eq!(
            moments_feed_row_string(&feed[0], "date_bucket").as_deref(),
            Some("2024-01-03")
        );
        assert!(moments_timeline_date_in_range(
            "2024-01-03T09:00:00Z",
            "2024-01-03",
            "2024-01-03"
        ));
    }

    #[test]
    fn moments_timeline_buckets_count_and_sample_items() {
        let mut feed = moments_feed_flat_rows_from_moments(
            &sample_moments(),
            &[route("notes"), route("events_upcoming")],
            MomentsFeedSort::Asc,
            None,
        );
        let mut options = timeline_options(feed_options(Vec::new()));
        options.feed.sort = MomentsFeedSort::Asc;

        moments_timeline_annotate_feed(&mut feed, options.date_bucket);
        let buckets = moments_timeline_buckets(&feed, &options);

        assert_eq!(moments_timeline_bucket_count(&feed), 3);
        assert_eq!(
            buckets
                .iter()
                .filter_map(|row| moments_feed_row_string(row, "bucket"))
                .collect::<Vec<_>>(),
            vec!["2024-01-01", "2024-01-03", "undated"]
        );
        assert_eq!(buckets[0].get("count").and_then(Value::as_u64), Some(1));
        assert_eq!(
            buckets[0]
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
    }
}
