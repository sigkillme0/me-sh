use serde::Serialize;
use serde_json::{Number, Value};

use crate::consts::{SEARCH_INCLUDE_FIELDS, route};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ValueKind {
    String,
    Number,
    Boolean,
    ArrayString,
    ArrayNumber,
    ArrayMixed,
    Json,
}

#[derive(Clone, Debug)]
pub(crate) enum DefaultValue {
    Number(f64),
    EmptyArray,
}

impl DefaultValue {
    pub(crate) fn to_json(&self) -> Value {
        match self {
            // Emit whole-number defaults as JSON integers, matching how
            // user-supplied numeric flags are coerced. Otherwise a defaulted
            // `--limit` reaches the API as `100.0` while an explicit `--limit
            // 100` reaches it as `100`.
            Self::Number(value)
                if value.fract() == 0.0 && *value >= 0.0 && *value <= u64::MAX as f64 =>
            {
                Value::Number(Number::from(*value as u64))
            }
            Self::Number(value) if value.fract() == 0.0 && *value >= i64::MIN as f64 => {
                Value::Number(Number::from(*value as i64))
            }
            Self::Number(value) => Number::from_f64(*value)
                .map(Value::Number)
                .unwrap_or(Value::Null),
            Self::EmptyArray => Value::Array(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OptionSpec {
    pub(crate) name: &'static str,
    pub(crate) flag: &'static str,
    pub(crate) kind: ValueKind,
    pub(crate) description: &'static str,
    pub(crate) default: Option<DefaultValue>,
    pub(crate) required: bool,
    pub(crate) allowed: &'static [&'static str],
}

impl OptionSpec {
    pub(crate) fn new(
        name: &'static str,
        flag: &'static str,
        kind: ValueKind,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            flag,
            kind,
            description,
            default: None,
            required: false,
            allowed: &[],
        }
    }

    pub(crate) fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub(crate) fn default(mut self, value: DefaultValue) -> Self {
        self.default = Some(value);
        self
    }

    pub(crate) fn allowed(mut self, values: &'static [&'static str]) -> Self {
        self.allowed = values;
        self
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NestedPrefix {
    pub(crate) prefix: &'static str,
    pub(crate) suffixes: &'static [&'static str],
}

#[derive(Clone, Debug)]
pub(crate) struct CommandSpec {
    pub(crate) name: &'static str,
    pub(crate) tool_name: &'static str,
    pub(crate) route_path: &'static str,
    pub(crate) description: &'static str,
    pub(crate) options: Vec<OptionSpec>,
    pub(crate) nested: &'static [NestedPrefix],
    pub(crate) destructive: bool,
}

pub(crate) fn search_command_spec() -> CommandSpec {
    CommandSpec {
        name: "contacts:search",
        tool_name: "searchContacts",
        route_path: route::SEARCH,
        description: "Search contacts.",
        options: search_options(),
        nested: SEARCH_NESTED,
        destructive: false,
    }
}

pub(crate) fn command_specs() -> Vec<CommandSpec> {
    vec![
        search_command_spec(),
        CommandSpec {
            name: "contact",
            tool_name: "getContact",
            route_path: route::GET_CONTACT,
            description: "Get a contact by ID.",
            options: vec![
                OptionSpec::new("contactId", "contact-id", ValueKind::Number, "Contact ID.")
                    .required(),
            ],
            nested: &[],
            destructive: false,
        },
        CommandSpec {
            name: "contacts:create",
            tool_name: "createContact",
            route_path: route::CREATE_CONTACT,
            description: "Create a contact.",
            options: contact_mutation_options(),
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "contacts:update",
            tool_name: "updateContact",
            route_path: route::UPDATE_CONTACT,
            description: "Update a contact.",
            options: {
                let mut options = contact_mutation_options();
                options.push(
                    OptionSpec::new("contactId", "contact-id", ValueKind::Number, "Contact ID.")
                        .required(),
                );
                options
            },
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "contacts:archive",
            tool_name: "archive_contact",
            route_path: route::ARCHIVE_CONTACT,
            description: "Archive contacts.",
            options: vec![
                OptionSpec::new(
                    "contactIds",
                    "contact-ids",
                    ValueKind::ArrayNumber,
                    "Contact IDs.",
                )
                .required(),
            ],
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "contacts:restore",
            tool_name: "restore_contact",
            route_path: route::RESTORE_CONTACT,
            description: "Restore archived contacts.",
            options: vec![
                OptionSpec::new(
                    "contactIds",
                    "contact-ids",
                    ValueKind::ArrayNumber,
                    "Contact IDs.",
                )
                .required(),
            ],
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "notes:create",
            tool_name: "createNote",
            route_path: route::NOTE,
            description: "Create a note on a contact.",
            options: vec![
                OptionSpec::new("contactId", "contact-id", ValueKind::Number, "Contact ID.")
                    .required(),
                OptionSpec::new("content", "content", ValueKind::String, "Note content.")
                    .required(),
                OptionSpec::new(
                    "reminderDate",
                    "reminder-date",
                    ValueKind::String,
                    "Optional ISO 8601 reminder date.",
                ),
            ],
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "groups",
            tool_name: "getGroups",
            route_path: route::GET_GROUPS,
            description: "List groups.",
            options: vec![OptionSpec::new(
                "limit",
                "limit",
                ValueKind::Number,
                "Maximum groups.",
            )],
            nested: &[],
            destructive: false,
        },
        CommandSpec {
            name: "groups:create",
            tool_name: "createGroup",
            route_path: route::CREATE_GROUP,
            description: "Create a group.",
            options: vec![
                OptionSpec::new("title", "title", ValueKind::String, "Group name.").required(),
            ],
            nested: &[],
            destructive: true,
        },
        CommandSpec {
            name: "groups:update",
            tool_name: "updateGroup",
            route_path: route::UPDATE_GROUP,
            description: "Update a group title or members.",
            options: vec![
                OptionSpec::new("groupId", "group-id", ValueKind::Number, "Group ID.").required(),
                OptionSpec::new("title", "title", ValueKind::String, "New group name."),
                OptionSpec::new(
                    "addContactIds",
                    "add-contact-ids",
                    ValueKind::ArrayNumber,
                    "Contact IDs to add.",
                )
                .default(DefaultValue::EmptyArray),
                OptionSpec::new(
                    "removeContactIds",
                    "remove-contact-ids",
                    ValueKind::ArrayNumber,
                    "Contact IDs to remove.",
                )
                .default(DefaultValue::EmptyArray),
            ],
            nested: &[],
            destructive: true,
        },
        moment_command(
            "notes",
            "getNotes",
            "/moments/notes",
            "Get notes between dates.",
        ),
        moment_command(
            "events",
            "getEvents",
            "/moments/events",
            "Get events between dates.",
        ),
        paged_command(
            "events:upcoming",
            "getUpcomingEvents",
            "/moments/events/upcoming",
            "Get upcoming events.",
        ),
        moment_command(
            "emails",
            "getEmails",
            "/moments/emails",
            "Get emails between dates.",
        ),
        paged_command(
            "emails:recent",
            "getRecentEmails",
            "/moments/emails/recent",
            "Get recent emails.",
        ),
        paged_command(
            "reminders:recent",
            "getRecentReminders",
            "/moments/reminders/recent",
            "Get recent reminders.",
        ),
        paged_command(
            "reminders:upcoming",
            "getUpcomingReminders",
            "/moments/reminders/upcoming",
            "Get upcoming reminders.",
        ),
        CommandSpec {
            name: "contacts:merge",
            tool_name: "merge_contacts",
            route_path: route::MERGE_CONTACTS,
            description: "Merge contacts. Requires --yes because this cannot be undone.",
            options: vec![
                OptionSpec::new(
                    "contactIds",
                    "contact-ids",
                    ValueKind::ArrayNumber,
                    "2 to 10 contact IDs.",
                )
                .required(),
            ],
            nested: &[],
            destructive: true,
        },
    ]
}

pub(crate) fn contact_mutation_options() -> Vec<OptionSpec> {
    vec![
        OptionSpec::new("firstName", "first-name", ValueKind::String, "First name."),
        OptionSpec::new("lastName", "last-name", ValueKind::String, "Last name."),
        OptionSpec::new(
            "phone",
            "phone",
            ValueKind::ArrayString,
            "Phone number. Repeat or comma-separate for multiple.",
        ),
        OptionSpec::new(
            "email",
            "email",
            ValueKind::ArrayString,
            "Email address. Repeat or comma-separate for multiple.",
        ),
        OptionSpec::new(
            "linkedin",
            "linkedin",
            ValueKind::String,
            "LinkedIn handle.",
        ),
        OptionSpec::new(
            "locations",
            "locations",
            ValueKind::Json,
            "JSON array of location objects.",
        ),
        OptionSpec::new("bio", "bio", ValueKind::String, "Biography."),
        OptionSpec::new(
            "website",
            "website",
            ValueKind::ArrayString,
            "Website. Repeat or comma-separate for multiple.",
        ),
        OptionSpec::new("title", "title", ValueKind::String, "Current title."),
        OptionSpec::new(
            "organization",
            "organization",
            ValueKind::String,
            "Current organization.",
        ),
        OptionSpec::new("birthday", "birthday", ValueKind::String, "Birthday date."),
    ]
}

fn moment_command(
    name: &'static str,
    tool: &'static str,
    route: &'static str,
    description: &'static str,
) -> CommandSpec {
    CommandSpec {
        name,
        tool_name: tool,
        route_path: route,
        description,
        options: vec![
            OptionSpec::new(
                "start",
                "start",
                ValueKind::String,
                "Start date, YYYY-MM-DD.",
            )
            .required(),
            OptionSpec::new("end", "end", ValueKind::String, "End date, YYYY-MM-DD.").required(),
            OptionSpec::new(
                "contactIds",
                "contact-ids",
                ValueKind::ArrayNumber,
                "Filter by contact IDs.",
            ),
        ],
        nested: &[],
        destructive: false,
    }
}

fn paged_command(
    name: &'static str,
    tool: &'static str,
    route: &'static str,
    description: &'static str,
) -> CommandSpec {
    CommandSpec {
        name,
        tool_name: tool,
        route_path: route,
        description,
        options: vec![
            OptionSpec::new("limit", "limit", ValueKind::Number, "Maximum results.")
                .default(DefaultValue::Number(10.0)),
            OptionSpec::new("page", "page", ValueKind::Number, "Page number.")
                .default(DefaultValue::Number(1.0)),
            OptionSpec::new(
                "contactIds",
                "contact-ids",
                ValueKind::ArrayNumber,
                "Filter by contact IDs.",
            ),
        ],
        nested: &[],
        destructive: false,
    }
}

pub(crate) fn search_options() -> Vec<OptionSpec> {
    vec![
        OptionSpec::new(
            "keywords",
            "keywords",
            ValueKind::ArrayString,
            "Fallback keyword search.",
        ),
        OptionSpec::new("name", "name", ValueKind::ArrayString, "Contact names."),
        OptionSpec::new(
            "workHistoryCompany",
            "work-history-company",
            ValueKind::ArrayString,
            "Company names.",
        ),
        OptionSpec::new(
            "workHistoryPosition",
            "work-history-position",
            ValueKind::ArrayString,
            "Job titles.",
        ),
        OptionSpec::new(
            "workHistoryActive",
            "work-history-active",
            ValueKind::Boolean,
            "Currently at company.",
        ),
        OptionSpec::new(
            "educationHistorySchool",
            "education-history-school",
            ValueKind::ArrayString,
            "School names.",
        ),
        OptionSpec::new(
            "educationHistoryDegree",
            "education-history-degree",
            ValueKind::ArrayString,
            "Degree names.",
        ),
        OptionSpec::new(
            "educationHistoryActive",
            "education-history-active",
            ValueKind::Boolean,
            "Currently studying.",
        ),
        OptionSpec::new(
            "locationLongitude",
            "location-longitude",
            ValueKind::Number,
            "Longitude.",
        ),
        OptionSpec::new(
            "locationLatitude",
            "location-latitude",
            ValueKind::Number,
            "Latitude.",
        ),
        OptionSpec::new(
            "locationDistance",
            "location-distance",
            ValueKind::Number,
            "Distance radius in km.",
        ),
        OptionSpec::new("ageGte", "age-gte", ValueKind::Number, "Minimum age."),
        OptionSpec::new("ageLte", "age-lte", ValueKind::Number, "Maximum age."),
        OptionSpec::new(
            "previousBirthdayGte",
            "previous-birthday-gte",
            ValueKind::String,
            "Earliest previous birthday.",
        ),
        OptionSpec::new(
            "previousBirthdayLte",
            "previous-birthday-lte",
            ValueKind::String,
            "Latest previous birthday.",
        ),
        OptionSpec::new(
            "informationType",
            "information-type",
            ValueKind::ArrayString,
            "Information types.",
        )
        .allowed(&["facebook", "linkedin", "phone", "email"]),
        OptionSpec::new(
            "upcomingBirthdayGte",
            "upcoming-birthday-gte",
            ValueKind::String,
            "Earliest upcoming birthday.",
        ),
        OptionSpec::new(
            "upcomingBirthdayLte",
            "upcoming-birthday-lte",
            ValueKind::String,
            "Latest upcoming birthday.",
        ),
        OptionSpec::new(
            "firstEmailDateGte",
            "first-email-date-gte",
            ValueKind::String,
            "First email after date.",
        ),
        OptionSpec::new(
            "firstEmailDateLte",
            "first-email-date-lte",
            ValueKind::String,
            "First email before date.",
        ),
        OptionSpec::new(
            "lastEmailDateGte",
            "last-email-date-gte",
            ValueKind::String,
            "Last email after date.",
        ),
        OptionSpec::new(
            "lastEmailDateLte",
            "last-email-date-lte",
            ValueKind::String,
            "Last email before date.",
        ),
        OptionSpec::new(
            "firstInteractionDateGte",
            "first-interaction-date-gte",
            ValueKind::String,
            "First interaction after date.",
        ),
        OptionSpec::new(
            "firstInteractionDateLte",
            "first-interaction-date-lte",
            ValueKind::String,
            "First interaction before date.",
        ),
        OptionSpec::new(
            "lastInteractionDateGte",
            "last-interaction-date-gte",
            ValueKind::String,
            "Last interaction after date.",
        ),
        OptionSpec::new(
            "lastInteractionDateLte",
            "last-interaction-date-lte",
            ValueKind::String,
            "Last interaction before date.",
        ),
        OptionSpec::new(
            "firstEventDateGte",
            "first-event-date-gte",
            ValueKind::String,
            "First event after date.",
        ),
        OptionSpec::new(
            "firstEventDateLte",
            "first-event-date-lte",
            ValueKind::String,
            "First event before date.",
        ),
        OptionSpec::new(
            "lastEventDateGte",
            "last-event-date-gte",
            ValueKind::String,
            "Last event after date.",
        ),
        OptionSpec::new(
            "lastEventDateLte",
            "last-event-date-lte",
            ValueKind::String,
            "Last event before date.",
        ),
        OptionSpec::new(
            "firstTextMessageDateGte",
            "first-text-message-date-gte",
            ValueKind::String,
            "First text after date.",
        ),
        OptionSpec::new(
            "firstTextMessageDateLte",
            "first-text-message-date-lte",
            ValueKind::String,
            "First text before date.",
        ),
        OptionSpec::new(
            "lastTextMessageDateGte",
            "last-text-message-date-gte",
            ValueKind::String,
            "Last text after date.",
        ),
        OptionSpec::new(
            "lastTextMessageDateLte",
            "last-text-message-date-lte",
            ValueKind::String,
            "Last text before date.",
        ),
        OptionSpec::new(
            "noteContent",
            "note-content",
            ValueKind::ArrayString,
            "Note content. Repeat or comma-separate for multiple values.",
        ),
        OptionSpec::new(
            "noteDateGte",
            "note-date-gte",
            ValueKind::String,
            "Note date after.",
        ),
        OptionSpec::new(
            "noteDateLte",
            "note-date-lte",
            ValueKind::String,
            "Note date before.",
        ),
        OptionSpec::new(
            "emailCountGte",
            "email-count-gte",
            ValueKind::Number,
            "Minimum email count.",
        ),
        OptionSpec::new(
            "emailCountLte",
            "email-count-lte",
            ValueKind::Number,
            "Maximum email count.",
        ),
        OptionSpec::new(
            "eventCountGte",
            "event-count-gte",
            ValueKind::Number,
            "Minimum event count.",
        ),
        OptionSpec::new(
            "eventCountLte",
            "event-count-lte",
            ValueKind::Number,
            "Maximum event count.",
        ),
        OptionSpec::new(
            "textMessageCountGte",
            "text-message-count-gte",
            ValueKind::Number,
            "Minimum text count.",
        ),
        OptionSpec::new(
            "textMessageCountLte",
            "text-message-count-lte",
            ValueKind::Number,
            "Maximum text count.",
        ),
        OptionSpec::new(
            "groupIds",
            "group-ids",
            ValueKind::ArrayMixed,
            "Group IDs or starred.",
        ),
        OptionSpec::new(
            "integration",
            "integration",
            ValueKind::ArrayString,
            "Integration names.",
        ),
        OptionSpec::new(
            "limit",
            "limit",
            ValueKind::Number,
            "Maximum results. Use 0 for count only.",
        )
        .default(DefaultValue::Number(100.0)),
        OptionSpec::new("sortField", "sort-field", ValueKind::String, "Sort field."),
        OptionSpec::new(
            "sortDirection",
            "sort-direction",
            ValueKind::String,
            "Sort direction.",
        ),
        OptionSpec::new(
            "excludeContactIds",
            "exclude-contact-ids",
            ValueKind::ArrayNumber,
            "Contact IDs to exclude.",
        ),
        OptionSpec::new(
            "includeFields",
            "include-fields",
            ValueKind::ArrayString,
            "Fields to include. Accepts aliases email, phone, linkedin, work, education, messages, events, notes, and integrations.",
        )
        .allowed(SEARCH_INCLUDE_FIELDS),
    ]
}

static SEARCH_NESTED: &[NestedPrefix] = &[
    NestedPrefix {
        prefix: "work_history",
        suffixes: &["company", "position", "active"],
    },
    NestedPrefix {
        prefix: "education_history",
        suffixes: &["school", "degree", "active"],
    },
    NestedPrefix {
        prefix: "location",
        suffixes: &["longitude", "latitude", "distance"],
    },
    NestedPrefix {
        prefix: "age",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "previous_birthday",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "upcoming_birthday",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "first_email_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "last_email_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "first_interaction_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "last_interaction_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "first_event_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "last_event_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "first_text_message_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "last_text_message_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "note_date",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "email_count",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "event_count",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "text_message_count",
        suffixes: &["gte", "lte"],
    },
    NestedPrefix {
        prefix: "sort",
        suffixes: &["field", "direction"],
    },
];
