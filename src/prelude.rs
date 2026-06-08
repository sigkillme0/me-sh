//! Crate-internal prelude.
//!
//! Every module pulls the shared namespace in with `use crate::prelude::*`.
//! This re-exports the sibling modules (names are globally unique, so this
//! keeps the flat resolution the code used before it was split into files)
//! together with the handful of external types and the `json!` macro used
//! throughout. Crate dependencies referenced by full path (`csv::`, `tar::`,
//! `zstd::`, `hex::`) need no entry here.

pub(crate) use crate::auth::*;
pub(crate) use crate::config::*;
pub(crate) use crate::consts::*;
pub(crate) use crate::contacts::*;
pub(crate) use crate::error::*;
pub(crate) use crate::fetch::*;
pub(crate) use crate::groups::*;
pub(crate) use crate::http::*;
pub(crate) use crate::moments::*;
pub(crate) use crate::output::*;
pub(crate) use crate::payload::*;
pub(crate) use crate::plan_audit::*;
pub(crate) use crate::progress::Progress;
pub(crate) use crate::routes::*;
pub(crate) use crate::runtime::*;
pub(crate) use crate::snapshot::*;

pub(crate) use crate::cli::build_cli;
pub(crate) use crate::command_spec::{
    CommandSpec, DefaultValue, NestedPrefix, OptionSpec, ValueKind, command_specs,
    contact_mutation_options, search_command_spec,
};

pub(crate) use std::collections::{BTreeMap, BTreeSet};
pub(crate) use std::fs;
pub(crate) use std::io::{self, BufRead, BufReader as StdBufReader, Read, Seek, SeekFrom, Write};
pub(crate) use std::path::{Component, Path, PathBuf};
pub(crate) use std::process::Command as ProcessCommand;
pub(crate) use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) use clap::ArgMatches;
pub(crate) use clap_complete::{Shell, generate};
pub(crate) use comfy_table::{Cell, ContentArrangement, Table, presets::UTF8_FULL};
pub(crate) use directories::BaseDirs;
pub(crate) use miette::{Context, IntoDiagnostic, Result, miette};
pub(crate) use reqwest::{Client as HttpClient, Method, StatusCode};
pub(crate) use serde::{Deserialize, Serialize};
pub(crate) use serde_json::{Map, Number, Value, json};
pub(crate) use sha2::{Digest, Sha256};
pub(crate) use thiserror::Error;
pub(crate) use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
pub(crate) use tokio::net::TcpListener;
pub(crate) use tokio::time::sleep;
pub(crate) use tracing::{debug, warn};
pub(crate) use tracing_subscriber::EnvFilter;
pub(crate) use url::Url;
