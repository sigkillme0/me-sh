//! mesh
//!
//! Module layout:
//! - `cli`, `command_spec` — clap command tree and generated route metadata
//! - `error`, `consts` — shared error type and configuration constants
//! - `output`, `payload` — output formats and CLI/JSON payload coercion
//! - `http`, `config`, `auth`, `runtime` — transport, credentials, the runtime
//! - `fetch`, `routes` — shared live-read primitives and route diagnostics
//! - `contacts`, `groups`, `moments`, `snapshot`, `plan_audit` — workflows
//! - `dispatch` — maps a parsed subcommand to its workflow

mod auth;
mod cli;
mod command_spec;
mod config;
mod consts;
mod contacts;
mod dispatch;
mod error;
mod fetch;
mod groups;
mod http;
mod moments;
mod output;
mod payload;
mod plan_audit;
mod prelude;
mod progress;
mod routes;
mod runtime;
mod snapshot;

#[tokio::main]
async fn main() -> miette::Result<()> {
    dispatch::install_diagnostics();
    let matches = cli::build_cli().get_matches();
    let runtime = runtime::Runtime::from_matches(&matches)?;
    dispatch::run(matches, runtime).await
}
