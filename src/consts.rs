use crate::prelude::*;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const API_BASE: &str = "https://api.me.sh";

pub(crate) const MCP_BASE: &str = "https://mcp.me.sh";

pub(crate) const AUTH_URL: &str = "https://app.me.sh/oauth/login";

pub(crate) const CLIENT_ID: &str = "cli";

pub(crate) const REDIRECT_URI: &str = "http://127.0.0.1:6374";

pub(crate) const CALLBACK_ADDR: &str = "127.0.0.1:6374";

pub(crate) const USER_AGENT: &str = "mesh";

/// API tool routes in bare form. `Runtime::call_tool` normalizes these to
/// `/tools/v2/...`. Single source of truth so route paths are not duplicated as
/// string literals across the codebase.
pub(crate) mod route {
    // contacts
    pub(crate) const SEARCH: &str = "/search";
    pub(crate) const GET_CONTACT: &str = "/get-contact";
    pub(crate) const CREATE_CONTACT: &str = "/create-contact";
    pub(crate) const UPDATE_CONTACT: &str = "/update-contact";
    pub(crate) const ARCHIVE_CONTACT: &str = "/archive-contact";
    pub(crate) const RESTORE_CONTACT: &str = "/restore-contact";
    pub(crate) const MERGE_CONTACTS: &str = "/merge-contacts";
    pub(crate) const NOTE: &str = "/note";
    // groups
    pub(crate) const GET_GROUPS: &str = "/get-groups";
    pub(crate) const CREATE_GROUP: &str = "/create-group";
    pub(crate) const UPDATE_GROUP: &str = "/update-group";
    // moments / activity
    pub(crate) const MOMENTS_NOTES: &str = "/moments/notes";
    pub(crate) const MOMENTS_EVENTS: &str = "/moments/events";
    pub(crate) const MOMENTS_EVENTS_UPCOMING: &str = "/moments/events/upcoming";
    pub(crate) const MOMENTS_EMAILS: &str = "/moments/emails";
    pub(crate) const MOMENTS_EMAILS_RECENT: &str = "/moments/emails/recent";
    pub(crate) const MOMENTS_REMINDERS_RECENT: &str = "/moments/reminders/recent";
    pub(crate) const MOMENTS_REMINDERS_UPCOMING: &str = "/moments/reminders/upcoming";
}

pub(crate) const CONFIG_FILE: &str = "mesh.json";

pub(crate) const LEGACY_CONFIG_FILES: &[&str] = &["mesh-cli.json", "clay-cli.json"];

pub(crate) const SEARCH_LIMIT_MAX: usize = 1000;

pub(crate) const MOMENT_PAGE_SIZE_DEFAULT: usize = 100;

pub(crate) const CONTACT_RESOLVE_CANDIDATE_LIMIT_DEFAULT: usize = 20;

pub(crate) const GROUP_PROFILE_MEMBER_LIMIT_DEFAULT: usize = 5;

pub(crate) const GROUP_COMPARE_ID_LIMIT_DEFAULT: usize = 50;

pub(crate) const SNAPSHOT_DIFF_DETAIL_LIMIT_DEFAULT: usize = 20;

pub(crate) const SNAPSHOT_DIFF_CHANGES_PER_RECORD_MAX: usize = 100;

pub(crate) const SNAPSHOT_STATS_TOP_DEFAULT: usize = 10;

pub(crate) const PLAN_AUDIT_ID_SAMPLE_DEFAULT: usize = 20;

pub(crate) const PLAN_AUDIT_DUPLICATE_SAMPLE_DEFAULT: usize = 20;

pub(crate) const CONTACT_MAP_TOP_BUCKETS_DEFAULT: usize = 50;

pub(crate) const CONTACT_MAP_SAMPLE_LIMIT_DEFAULT: usize = 5;

pub(crate) const CONTACT_MAP_EDGE_LIMIT_DEFAULT: usize = 5000;

pub(crate) const CONTACT_RECONNECT_TOP_DEFAULT: usize = 50;

pub(crate) const CONTACT_RECONNECT_LOW_ACTIVITY_DEFAULT: usize = 0;

pub(crate) const CONTACT_RECONNECT_ACTIVITY_CHUNK_SIZE: usize = 200;

pub(crate) const MOMENTS_TIMELINE_BUCKET_LIMIT_DEFAULT: usize = 30;

pub(crate) const MOMENTS_TIMELINE_ITEMS_PER_BUCKET_DEFAULT: usize = 5;

pub(crate) const CONTACT_FETCH_CONCURRENCY_DEFAULT: usize = 4;

pub(crate) const CONTACT_FETCH_CONCURRENCY_MAX: usize = 16;

pub(crate) const PROFILE_GROUP_SCAN_LIMIT_DEFAULT: usize = 1000;

pub(crate) const PROFILE_ACTIVITY_DEFAULT_SECTIONS: &[&str] = &[
    "events_upcoming",
    "emails_recent",
    "reminders_recent",
    "reminders_upcoming",
];

pub(crate) const ROUTE_DOCTOR_PROBE_TEMPLATES: &[RouteProbeTemplate] = &[
    RouteProbeTemplate {
        label: "search_count",
        route: route::SEARCH,
        kind: RouteProbeKind::SearchCount,
    },
    RouteProbeTemplate {
        label: "groups",
        route: route::GET_GROUPS,
        kind: RouteProbeKind::ArrayRows,
    },
    RouteProbeTemplate {
        label: "notes",
        route: route::MOMENTS_NOTES,
        kind: RouteProbeKind::MomentDateWindow,
    },
    RouteProbeTemplate {
        label: "events",
        route: route::MOMENTS_EVENTS,
        kind: RouteProbeKind::MomentDateWindow,
    },
    RouteProbeTemplate {
        label: "emails",
        route: route::MOMENTS_EMAILS,
        kind: RouteProbeKind::MomentDateWindow,
    },
    RouteProbeTemplate {
        label: "events_upcoming",
        route: route::MOMENTS_EVENTS_UPCOMING,
        kind: RouteProbeKind::MomentPaged,
    },
    RouteProbeTemplate {
        label: "emails_recent",
        route: route::MOMENTS_EMAILS_RECENT,
        kind: RouteProbeKind::MomentPaged,
    },
    RouteProbeTemplate {
        label: "reminders_recent",
        route: route::MOMENTS_REMINDERS_RECENT,
        kind: RouteProbeKind::MomentPaged,
    },
    RouteProbeTemplate {
        label: "reminders_upcoming",
        route: route::MOMENTS_REMINDERS_UPCOMING,
        kind: RouteProbeKind::MomentPaged,
    },
];

pub(crate) const ALL_MOMENT_SECTIONS: &[&str] = &[
    "notes",
    "events",
    "emails",
    "events_upcoming",
    "emails_recent",
    "reminders_recent",
    "reminders_upcoming",
];

pub(crate) const SEARCH_INCLUDE_FIELDS: &[&str] = &[
    "work_history",
    "education_history",
    "location",
    "birthday",
    "created",
    "interaction_history",
    "message_history",
    "email_history",
    "event_history",
    "notes",
    "integrations",
    "emails",
    "phone_numbers",
    "social_links",
];

pub(crate) const SNAPSHOT_MOMENT_ROUTES: &[SnapshotMomentRoute] = &[
    SnapshotMomentRoute {
        label: "notes",
        file_name: "notes.jsonl",
        route: route::MOMENTS_NOTES,
        kind: SnapshotMomentKind::DateWindow,
    },
    SnapshotMomentRoute {
        label: "events",
        file_name: "events.jsonl",
        route: route::MOMENTS_EVENTS,
        kind: SnapshotMomentKind::DateWindow,
    },
    SnapshotMomentRoute {
        label: "emails",
        file_name: "emails.jsonl",
        route: route::MOMENTS_EMAILS,
        kind: SnapshotMomentKind::DateWindow,
    },
    SnapshotMomentRoute {
        label: "events_upcoming",
        file_name: "events-upcoming.jsonl",
        route: route::MOMENTS_EVENTS_UPCOMING,
        kind: SnapshotMomentKind::Paged,
    },
    SnapshotMomentRoute {
        label: "emails_recent",
        file_name: "emails-recent.jsonl",
        route: route::MOMENTS_EMAILS_RECENT,
        kind: SnapshotMomentKind::Paged,
    },
    SnapshotMomentRoute {
        label: "reminders_recent",
        file_name: "reminders-recent.jsonl",
        route: route::MOMENTS_REMINDERS_RECENT,
        kind: SnapshotMomentKind::Paged,
    },
    SnapshotMomentRoute {
        label: "reminders_upcoming",
        file_name: "reminders-upcoming.jsonl",
        route: route::MOMENTS_REMINDERS_UPCOMING,
        kind: SnapshotMomentKind::Paged,
    },
];

pub(crate) const SNAPSHOT_STATS_FIELDS: &[(&str, &[&str])] = &[
    ("name", &["name", "displayName", "display_name"]),
    ("email", &["email", "emails", "email_history"]),
    ("phone", &["phone", "phone_numbers", "phones"]),
    ("linkedin", &["linkedin", "social_links"]),
    ("work", &["work_history", "title", "organization"]),
    ("location", &["location", "locations"]),
    ("notes", &["notes", "note"]),
    ("events", &["events", "event_history"]),
    ("messages", &["messages", "message_history"]),
    ("interactions", &["interactions", "interaction_history"]),
];
