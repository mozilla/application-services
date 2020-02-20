#![allow(warnings)]
use cli_support::fxa_creds::*;
use cli_support::prompt::{prompt_char, prompt_string, prompt_usize};
use failure::Fail;
// use prettytable::{cell, row, Cell, Row, Table};
use remerge::RemergeEngine;
use rusqlite::NO_PARAMS;
use serde_json;
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use sync15_traits::*;

use remerge::NativeRecord as Record;
use std::convert::{TryFrom, TryInto};

// I'm completely punting on good error handling here.
type Result<T, E = failure::Error> = std::result::Result<T, E>;

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(AsRef::as_ref)
}

fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn init_logging() {
    if cfg!(debug_assertions) {
        std::env::set_var("RUST_BACKTRACE", "1");
        // Explicitly ignore some rather noisy crates. Turn on trace for everyone else.
        let spec = "trace,rustyline=error,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
        env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", spec));
    }
}

fn target_dir() -> PathBuf {
    let mut path = std::env::current_exe().expect("Failed to get current exe path...");
    // Find `target` which should contain this program.
    while path.file_name().expect("Failed to find target!") != "target" {
        path.pop();
    }
    path
}

fn init_fxa(root: &Path, opts: &mut Opts) -> Result<CliFxa> {
    if let Some(ref mut email) = opts.autocreate_restmail {
        let cfg = FxaConfigUrl::StableDev
            .default_config()
            .unwrap_or_else(get_default_fxa_config);
        if email.len() < 5 {
            email.push_str("-twodle")
        }
        if !email.ends_with("@restmail.net") {
            email.push_str("@restmail.net");
        } else if email.len() - 13 < 5 {
            email.truncate(email.len() - 13);
            email.push_str("-twodle@restmail.net");
        }
        let pass = option_env!("DEMO_FXA_PASS")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                email[..email.len() - 13]
                    .chars()
                    .chain("longer_abc".chars())
                    .rev()
                    .collect::<String>()
            });

        std::fs::create_dir_all(&root)?;
        let creds = root.join(format!("{}-credentials.json", &email[..(email.len() - 13)]));
        Ok(cli_support::fxa_creds::auto_restmail::load_or_create(
            &email, &pass, &creds, &cfg,
        )?)
    } else {
        std::fs::create_dir_all(&root)?;
        let creds = root.join("credentials.json");
        Ok(get_cli_fxa(get_default_fxa_config(), &creds)?)
    }
}

fn show_all_filt(engine: &RemergeEngine, filter: fn(&TodoItem) -> bool) -> Result<Vec<TodoItem>> {
    use colored::*;
    let records = engine.list()?;
    let mut res = vec![];
    for (i, record) in records.into_iter().enumerate() {
        let todo = TodoItem::new(record)?;
        if !filter(&todo) {
            continue;
        }
        let status = if todo.finished {
            "[finished]".blue()
        } else {
            "[in progress]".yellow()
        };
        println!(
            "{counter} \x1b[1m{status}\x1b[0m (\x1b[2m{id}\x1b[0m): {task}",
            counter = format!("{}.", res.len()).green(),
            id = todo.id,
            status = status.bold(),
            task = todo.title,
        );
        if let Some(o) = &todo.owner {
            println!("    Owned By: {}", o);
        }
        println!();
        res.push(todo);
    }
    Ok(res)
}

fn show_all(engine: &RemergeEngine) -> Result<Vec<TodoItem>> {
    show_all_filt(engine, |_| true)
}
// Note: this uses doc comments to generate the help text.
#[derive(Clone, Debug, StructOpt)]
#[structopt(name = "twodle", about = "remerge todo list demo")]
pub struct Opts {
    /// Device name
    #[structopt(long = "device", short = "d", default_value = "local")]
    pub device_name: String,

    #[structopt(long, short)]
    pub quiet: bool,

    #[structopt(long = "account", short = "a")]
    pub autocreate_restmail: Option<String>,

    #[structopt(long, short = "2")]
    pub second: bool,
    // #[structopt(short = "s", default_value = "stable-dev")]
    // pub fxa_stack: FxaConfigUrl,
}

fn main() -> Result<()> {
    let mut opts = Opts::from_args();
    if !opts.quiet {
        init_logging();
    }
    let root = target_dir().join(".todo-demo");
    let dev_root = root.join(&opts.device_name);

    std::fs::create_dir_all(&dev_root)?;

    let fxa = init_fxa(&dev_root, &mut opts)?;
    let mut mcs = sync15::MemoryCachedState::default();

    let db = dev_root.join("todo.db");
    println!("DB located at {:?}", db);
    let mut engine = RemergeEngine::open(db, include_str!("./schema.json")).unwrap();

    println!("Engine has {} records", engine.list()?.len());

    if let Err(e) = show_all(&engine) {
        log::warn!("Failed to show initial data! {}", e);
    }
    let mut rl = rustyline::Editor::<()>::new();
    println!("commands: `add`, `complete`, `modify`, `delete`, `list`, `sync`");
    loop {
        match input_loop(&mut rl, &mut engine, &fxa, &mut mcs) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
    }
    Ok(())
}
type LinePrompt = rustyline::Editor<()>;
fn readline_nonempty(rl: &mut LinePrompt, prompt: &str) -> Result<Option<String>> {
    for _ in 0..2 {
        match rl.readline(prompt) {
            Ok(l) => {
                let l = l.trim();
                if l.is_empty() {
                    continue;
                }
                return Ok(Some(l.into()));
            }
            Err(rustyline::error::ReadlineError::Eof) => std::process::exit(1),
            Err(rustyline::error::ReadlineError::Interrupted) => return Ok(None),
            Err(e) => {
                eprintln!("input error: {}", e);
                failure::bail!("input error: {}", e);
            }
        }
    }
    Ok(None)
}

fn is_default<T: PartialEq + Default>(v: &T) -> bool {
    v == &T::default()
}

#[derive(Clone, Default, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TodoItem {
    #[serde(skip_serializing_if = "is_default")]
    id: Guid,
    finished: bool,
    #[serde(rename = "task")]
    title: String,
    #[serde(skip_serializing_if = "is_default")]
    owner: Option<String>,
}

impl TodoItem {
    pub fn new(e: Record) -> Result<Self, remerge::Error> {
        Ok(serde_json::from_value(e.into_val())?)
    }
}
impl TryFrom<TodoItem> for Record {
    type Error = remerge::Error;
    fn try_from(e: TodoItem) -> Result<Self, Self::Error> {
        let raw: serde_json::Value = serde_json::to_value(e)?;
        Ok(Record::from_value_unchecked(raw)?)
    }
}
impl TryFrom<Record> for TodoItem {
    type Error = remerge::Error;
    fn try_from(e: Record) -> std::result::Result<Self, Self::Error> {
        Ok(serde_json::from_value(e.into_val())?)
    }
}

fn choose_record(
    rl: &mut LinePrompt,
    engine: &mut RemergeEngine,
    filter: fn(&TodoItem) -> bool,
) -> Result<Option<TodoItem>> {
    let l = show_all_filt(&engine, filter)?;
    if l.is_empty() {
        println!("No available records");
        return Ok(None);
    }
    let n = match rl.readline("enter record index: ")?.parse::<usize>() {
        Ok(n) if n < l.len() => n,
        _ => {
            println!("nevermind then");
            return Ok(None);
        }
    };
    Ok(Some(l[n].clone()))
}

fn input_loop(
    rl: &mut LinePrompt,
    engine: &mut RemergeEngine,
    cli_fxa: &CliFxa,
    mcs: &mut sync15::MemoryCachedState,
) -> Result<bool> {
    let l = if let Some(e) = readline_nonempty(rl, "> ")? {
        e
    } else {
        return Ok(true);
    };

    let (s, e) = l.split_at(l.find(' ').map(|i| i + 1).unwrap_or_else(|| l.len()));
    match &*s.trim().to_ascii_lowercase() {
        "help" => {
            println!("commands: `add`, `complete`, `modify`, `delete`, `list`, `sync`");
        }
        "exit" => {
            return Ok(false);
        }
        "add" => {
            let mut task = e.trim().to_string();
            if task.is_empty() {
                if let Some(t) = readline_nonempty(rl, "Description: ")? {
                    task = t.to_string();
                } else {
                    println!("Discarded.");
                    return Ok(true);
                }
            }
            let owner = rl.readline("Owner (optional): ")?.trim().to_owned();
            let owner = if owner.is_empty() {
                None
            } else {
                Some(owner.to_owned())
            };
            let id = engine.insert(TodoItem {
                title: task.to_owned(),
                owner,
                ..TodoItem::default()
            })?;
            if let Some(i) = engine.get(id.clone())? {
                let r: TodoItem = i.try_into()?;
                println!("\nAdded record: {:#?}", r);
            } else {
                println!("\nAdd failed of {:?}?", id);
            }
        }
        "sync" => match sync(engine, cli_fxa, mcs) {
            Err(e) => {
                log::warn!("Sync failed! {}", e);
                log::warn!("BT: {:?}", e.backtrace());
            }
            Ok(sync_ping) => {
                log::info!("Sync was successful!");
                log::info!(
                    "Sync telemetry: {}",
                    serde_json::to_string_pretty(&sync_ping).unwrap()
                );
            }
        },
        "complete" => {
            if let Some(mut r) = choose_record(rl, engine, |r| !r.finished)? {
                r.finished = true;
                engine.update(r)?;
            }
        }
        "delete" => {
            if let Some(r) = choose_record(rl, engine, |_| true)? {
                engine.delete(r.id.clone())?;
                println!("Deleted: {:?}", r);
            }
        }
        "modify" => {
            if let Some(mut r) = choose_record(rl, engine, |_| true)? {
                println!("Record is {:#?}", r);
                println!("Modify task (was {:?})", r.title);
                if let Ok(v) = rl.readline("> ") {
                    if !v.is_empty() {
                        r.title = v.into()
                    }
                }
                println!("set owner? ({:?})", r.owner);
                if let Ok(v) = rl.readline("> ") {
                    if !v.is_empty() {
                        r.owner = Some(v.into());
                    }
                }
                engine.update(r)?;
            }
        }
        "list" => {
            show_all(&engine)?;
        }
        _ => {}
    }
    Ok(true)
}
pub fn sync(
    r: &mut RemergeEngine,
    cli_fxa: &CliFxa,
    mcs: &mut sync15::MemoryCachedState,
) -> Result<sync15::telemetry::SyncTelemetryPing> {
    let store = r.sync_store();
    let mut state = store.get_disc_cached_state()?;
    let mut result = sync15::sync_multiple(
        &[&store],
        &mut state,
        mcs,
        &cli_fxa.client_init,
        &cli_fxa.root_sync_key,
        &(),
        None,
    );
    // We always update the state - sync_multiple does the right thing
    // if it needs to be dropped (ie, they will be None or contain Nones etc)
    store.set_disc_cached_state(state)?;

    // for b/w compat reasons, we do some dances with the result.
    // XXX - note that this means telemetry isn't going to be reported back
    // to the app - we need to check with lockwise about whether they really
    // need these failures to be reported or whether we can loosen this.
    if let Err(e) = result.result {
        return Err(e.into());
    }
    match result.engine_results.remove("passwords") {
        None | Some(Ok(())) => Ok(result.telemetry),
        Some(Err(e)) => Err(e.into()),
    }
}
