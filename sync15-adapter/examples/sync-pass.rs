
extern crate sync15_adapter as sync;
extern crate url;
extern crate base64;
extern crate reqwest;
#[macro_use]
extern crate prettytable;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

#[macro_use]
extern crate log;

extern crate env_logger;
extern crate failure;

use std::io::{self, Read, Write};
use std::fs;
use std::process;
use sync::{error, ServerTimestamp, OutgoingChangeset, Payload, Store};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
    access_token: String,
    refresh_token: String,
    keys: HashMap<String, ScopedKeyData>,
    expires_in: u64,
    auth_at: u64,
}

#[derive(Debug, Deserialize)]
struct ScopedKeyData {
    k: String,
    kid: String,
    scope: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRecord {
    pub id: String,
    pub hostname: Option<String>,

    // rename_all = "camelCase" by default will do formSubmitUrl, but we can just
    // override this one field.
    #[serde(rename = "formSubmitURL")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_submit_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_realm: Option<String>,

    #[serde(default = "String::new")]
    pub username: String,

    pub password: String,

    #[serde(default = "String::new")]
    pub username_field: String,

    #[serde(default = "String::new")]
    pub password_field: String,

    pub time_created: i64,
    pub time_password_changed: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_last_used: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub times_used: Option<i64>,
}

fn do_auth(recur: bool) -> Result<OAuthCredentials, failure::Error> {
    match fs::File::open("./credentials.json") {
        Err(_) => {
            if recur {
                panic!("Failed to open credentials 2nd time");
            }
            println!("No credentials found, invoking boxlocker.py...");
            process::Command::new("python")
                .arg("../boxlocker/boxlocker.py").output()
                .expect("Failed to run boxlocker.py");
            return do_auth(true);
        },
        Ok(mut file) => {
            let mut s = String::new();
            file.read_to_string(&mut s)?;
            let creds: OAuthCredentials = serde_json::from_str(&s)?;
            let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if creds.expires_in + creds.auth_at < time {
                println!("Warning, credentials may be stale.");
            }
            Ok(creds)
        }
    }
}

fn read_json_file<T>(path: &str) -> Result<T, failure::Error> where for<'a> T: serde::de::Deserialize<'a> {
    let file = fs::File::open(path)?;
    Ok(serde_json::from_reader(&file)?)
}

fn prompt_string<S: AsRef<str>>(prompt: S) -> Option<String> {
    print!("{}: ", prompt.as_ref());
    let _ = io::stdout().flush(); // Don't care if flush fails really.
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("Failed to read line...");
    if let Some('\n') = s.chars().next_back() { s.pop(); }
    if let Some('\r') = s.chars().next_back() { s.pop(); }
    if s.len() == 0 {
        None
    } else {
        Some(s)
    }
}

fn prompt_usize<S: AsRef<str>>(prompt: S) -> Option<usize> {
    if let Some(s) = prompt_string(prompt) {
        match s.parse::<usize>() {
            Ok(n) => Some(n),
            Err(_) => {
                println!("Couldn't parse!");
                None
            }
        }
    } else {
        None
    }
}

#[inline]
fn duration_ms(dur: Duration) -> u64 {
    dur.as_secs() * 1000 + ((dur.subsec_nanos() / 1_000_000) as u64)
}

#[inline]
fn unix_time_ms() -> u64 {
    duration_ms(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
}

fn read_login() -> PasswordRecord {
    let username = prompt_string("username").unwrap_or(String::new());
    let password = prompt_string("password").unwrap_or(String::new());
    let form_submit_url = prompt_string("form_submit_url");
    let hostname = prompt_string("hostname");
    let http_realm = prompt_string("http_realm");
    let username_field = prompt_string("username_field").unwrap_or(String::new());
    let password_field = prompt_string("password_field").unwrap_or(String::new());
    let ms_i64 = unix_time_ms() as i64;
    PasswordRecord {
        id: sync::util::random_guid().unwrap().into(),
        username,
        password,
        username_field,
        password_field,
        form_submit_url,
        http_realm,
        hostname,
        time_created: ms_i64,
        time_password_changed: ms_i64,
        times_used: None,
        time_last_used: Some(ms_i64),
    }
}

fn update_string(field_name: &str, field: &mut String, extra: &str) -> bool {
    let opt_s = prompt_string(format!("new {} [now {}{}]", field_name, field, extra));
    if let Some(s) = opt_s {
        *field = s;
        true
    } else {
        false
    }
}

fn string_opt(o: &Option<String>) -> Option<&str> {
    o.as_ref().map(|s| s.as_ref())
}
fn string_opt_or<'a>(o: &'a Option<String>, or: &'a str) -> &'a str {
    string_opt(o).unwrap_or(or)
}

fn update_login(record: &mut PasswordRecord) {
    update_string("username", &mut record.username, ", leave blank to keep");
    let changed_password = update_string("password", &mut record.password, ", leave blank to keep");

    if changed_password {
        record.time_password_changed = unix_time_ms() as i64;
    }

    update_string("username_field", &mut record.username_field, ", leave blank to keep");
    update_string("password_field", &mut record.password_field, ", leave blank to keep");

    if prompt_bool(&format!("edit hostname? (now {}) [yN]", string_opt_or(&record.hostname, "(none)"))).unwrap_or(false) {
        record.hostname = prompt_string("hostname");
    }

    if prompt_bool(&format!("edit form_submit_url? (now {}) [yN]", string_opt_or(&record.form_submit_url, "(none)"))).unwrap_or(false) {
        record.form_submit_url = prompt_string("form_submit_url");
    }
}

fn prompt_bool(msg: &str) -> Option<bool> {
    let result = prompt_string(msg);
    result.and_then(|r| match r.chars().next().unwrap() {
        'y' | 'Y' | 't' | 'T' => Some(true),
        'n' | 'N' | 'f' | 'F' => Some(false),
        _ => None
    })
}


fn prompt_chars(msg: &str) -> Option<char> {
    prompt_string(msg).and_then(|r| r.chars().next())
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
struct PasswordEngine {
    pub last_sync: ServerTimestamp,
    pub records: HashMap<String, PasswordRecord>,
    pub changes: HashMap<String, u64>,
    // TODO: meta global stuff
}

impl PasswordEngine {
    pub fn load_or_create() -> PasswordEngine {
        match read_json_file::<PasswordEngine>("./password-engine.json") {
            Ok(engine) => engine,
            Err(e) => {
                println!("Failed to read from password-engine.json. {}", e);
                println!("Blindly assuming that the file doesn't exist...");
                println!("We're likely to clobber it if you don't stop now!");
                PasswordEngine::default()
            }
        }
    }

    pub fn save(&mut self) -> Result<(), failure::Error> {
        // We should really be doing this atomically. I'm just lazy.
        let file = fs::File::create("./password-engine.json")?;
        serde_json::to_writer(file, &self)?;
        Ok(())
    }

    pub fn create(&mut self, r: PasswordRecord) -> Result<(), failure::Error> {
        let id = r.id.clone();
        self.changes.insert(id.clone(), unix_time_ms());
        self.records.insert(id, r);
        self.save()
    }

    pub fn delete(&mut self, id: String) -> Result<(), failure::Error> {
        if self.records.remove(&id).is_none() {
            println!("No such record by that id, but we'll add a tombstone anyway");
        }
        self.changes.insert(id, unix_time_ms());
        self.save()
    }

    pub fn update(&mut self, id: &str, updater: impl FnMut(&mut PasswordRecord)) -> Result<bool, failure::Error> {
        if self.records.get_mut(id).map(updater).is_none() {
            println!("No such record!");
            return Ok(false);
        }
        self.changes.insert(id.into(), unix_time_ms());
        self.save()?;
        Ok(true)
    }

    pub fn sync(&mut self, svc: &sync::Sync15Service) -> Result<(), failure::Error> {
        let ts = self.last_sync;
        sync::synchronize(svc, self, "passwords".into(), ts, true)?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<(), failure::Error> {
        self.last_sync = 0.0.into();
        self.changes.clear();
        self.save()?;
        Ok(())
    }

    pub fn wipe(&mut self) -> Result<(), failure::Error> {
        self.last_sync = 0.0.into();
        self.changes.clear();
        self.records.clear();
        self.save()?;
        Ok(())
    }

    pub fn get_unsynced_changes(&self) -> sync::Result<(Vec<(Payload, SystemTime)>, ServerTimestamp)> {
        let mut result = Vec::with_capacity(self.changes.len());
        for (changed_id, time) in self.changes.iter() {
            let ct = if let Some(record) = self.records.get(changed_id) {
                Payload::from_record(record.clone())?
            } else {
                Payload::new_tombstone(changed_id.clone())
            };
            let mod_time = UNIX_EPOCH + Duration::new(
                time / 1000, ((time % 1000) * 1_000_000) as u32);
            result.push((ct, mod_time));
        }
        Ok((result, self.last_sync))
    }

    pub fn apply_reconciled_changes(
        &mut self,
         record_changes: &[Payload],
         new_last_sync: ServerTimestamp
    ) -> sync::Result<()> {
        for change in record_changes {
            if change.is_tombstone() {
                self.records.remove(change.id());
            } else {
                self.records.insert(change.id().into(),
                                    change.clone().into_record()?);
            }
        }
        self.last_sync = new_last_sync;
        self.save().map_err(sync::error::ErrorKind::StoreError)?;
        Ok(())
    }
}


impl Store for PasswordEngine {
    type Error = error::Error;

    fn apply_incoming(
        &mut self,
        inbound: sync::IncomingChangeset
    ) -> sync::Result<OutgoingChangeset> {
        info!("Remote collection has {} changes", inbound.changes.len());

        let (outbound_changes, last_sync) = self.get_unsynced_changes()?;
        info!("Local collection has {} changes", outbound_changes.len());

        let reconciled = Reconciliation::between(outbound_changes,
                                                 inbound.changes,
                                                 inbound.timestamp)?;

        info!("Finished Reconciling: apply local {}, apply remote {}",
              reconciled.apply_as_incoming.len(),
              reconciled.apply_as_outgoing.len());

        self.apply_reconciled_changes(&reconciled.apply_as_incoming[..], inbound.timestamp)?;

        Ok(OutgoingChangeset {
            changes: reconciled.apply_as_outgoing,
            timestamp: last_sync,
            collection: "passwords".into()
        })
    }

    fn sync_finished(&mut self, new_last_sync: ServerTimestamp, records_synced: &[String]) -> sync::Result<()> {
        for id in records_synced {
            self.changes.remove(id);
        }
        self.last_sync = new_last_sync;
        self.save().map_err(sync::error::ErrorKind::StoreError)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Reconciliation {
    apply_as_incoming: Vec<Payload>,
    apply_as_outgoing: Vec<Payload>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RecordChoice {
    TakeLocal,
    TakeRemote,
    TakeCombined(Payload),
}

impl Reconciliation {
    fn reconcile_single(
        remote: (&Payload, Duration),
        local: (&Payload, Duration)
    ) -> sync::Result<RecordChoice> {
        Ok(match (local.0.is_tombstone(), remote.0.is_tombstone()) {
            (true, true) => {
                trace!("Both records are tombstones, doesn't matter which we take");
                RecordChoice::TakeRemote
            },
            (false, true) => {
                trace!("Modified locally, remote tombstone (keeping local)");
                RecordChoice::TakeLocal
            },
            (true, false) => {
                trace!("Modified on remote, locally tombstone (keeping remote)");
                RecordChoice::TakeRemote
            },
            (false, false) => {
                trace!("Modified on both remote and local, chosing on age (remote = {}s, local = {}s)",
                       remote.1.as_secs(), local.1.as_secs());

                // Take younger.
                if local.1 <= remote.1 {
                    RecordChoice::TakeLocal
                } else {
                    RecordChoice::TakeRemote
                }
            }
        })
    }

    pub fn between(
        local_changes: Vec<(Payload, SystemTime)>,
        remote_changes: Vec<(Payload, ServerTimestamp)>,
        remote_timestamp: ServerTimestamp
    ) -> sync::Result<Reconciliation> {
        let mut result = Reconciliation {
            apply_as_incoming: vec![],
            apply_as_outgoing: vec![],
        };

        let mut local_lookup: HashMap<String, (Payload, Duration)> =
            local_changes.into_iter().map(|(record, time)| {
                (record.id.clone(),
                 (record,
                  time.elapsed().unwrap_or(Duration::new(0, 0))))
            }).collect();

        for (remote, remote_modified) in remote_changes.into_iter() {
            let remote_age = remote_modified.duration_since(remote_timestamp)
                                            .unwrap_or(Duration::new(0, 0));

            let (choice, local) =
                if let Some((local, local_age)) = local_lookup.remove(remote.id()) {
                    (Reconciliation::reconcile_single((&remote, remote_age), (&local, local_age))?, Some(local))
                } else {
                    // No local change with that ID
                    (RecordChoice::TakeRemote, None)
                };

            match choice {
                RecordChoice::TakeRemote => result.apply_as_incoming.push(remote),
                RecordChoice::TakeLocal => result.apply_as_outgoing.push(local.unwrap()),
                RecordChoice::TakeCombined(ct) => {
                    result.apply_as_incoming.push(ct.clone());
                    result.apply_as_outgoing.push(ct);
                }
            }
        }

        for (_, (local_record, _)) in local_lookup.into_iter() {
            result.apply_as_outgoing.push(local_record);
        }

        Ok(result)
    }
}

fn show_all(e: &PasswordEngine) -> Vec<&str> {
    let mut table = prettytable::Table::new();
    table.add_row(row![
            "(idx)", "id",
            "username", "password",
            "usernameField", "passwordField",
            "hostname",
            "formSubmitURL"
            // Skipping metadata so this isn't insanely long
        ]);
    let mut v = Vec::with_capacity(e.records.len());
    let mut records = e.records.iter().collect::<Vec<_>>();
    records.sort_by(|a, b| a.0.cmp(b.0));
    for (id, rec) in records.iter() {
        table.add_row(row![
                v.len(),
                &id,
                &rec.username,
                &rec.password,
                &rec.username_field,
                &rec.password_field,
                string_opt_or(&rec.hostname, ""),
                string_opt_or(&rec.form_submit_url, "")
            ]);
        v.push(&id[..]);
    }
    table.printstd();
    v
}

fn prompt_record_id(e: &PasswordEngine, action: &str) -> Option<String> {
    let index_to_id = show_all(e);
    let input = prompt_usize(&format!("Enter (idx) of record to {}", action))?;
    if input >= index_to_id.len() {
        println!("No such index");
        return None;
    }
    Some(index_to_id[input].into())
}

fn main() -> Result<(), failure::Error> {
    env_logger::init();
    let oauth_data = do_auth(false)?;

    let scope = &oauth_data.keys["https://identity.mozilla.com/apps/oldsync"];

    let mut svc = sync::Sync15Service::new(
        sync::Sync15ServiceInit {
            key_id: scope.kid.clone(),
            sync_key: scope.k.clone(),
            access_token: oauth_data.access_token.clone(),
            tokenserver_base_url: "https://oauth-sync.dev.lcip.org/syncserver/token".into(),
        }
    )?;

    svc.remote_setup()?;

    let mut engine = PasswordEngine::load_or_create();
    println!("Performing startup sync");

    if let Err(e) = engine.sync(&svc) {
        println!("Initial sync failed: {}", e);
        if !prompt_bool("Would you like to continue [yN]").unwrap_or(false) {
            return Err(e);
        }
    }

    println!("Engine has {} passwords", engine.records.len());

    show_all(&engine);

    loop {
        match prompt_chars("[A]dd, [D]elete, [U]pdate, [S]ync, [V]iew, [R]eset, [W]ipe or [Q]uit").unwrap_or('?') {
            'A' | 'a' => {
                println!("Adding new record");
                let record = read_login();
                if let Err(e) = engine.create(record) {
                    println!("Failed to create record! {}", e);
                }
            }
            'D' | 'd' => {
                println!("Deleting record");
                if let Some(id) = prompt_record_id(&engine, "delete") {
                    if let Err(e) = engine.delete(id) {
                        println!("Failed to delete record! {}", e);
                    }
                }
            }
            'U' | 'u' => {
                println!("Updating record fields");
                if let Some(id) = prompt_record_id(&engine, "update") {
                    if let Err(e) = engine.update(&id, update_login) {
                        println!("Failed to update record! {}", e);
                    }
                }
            }
            'R' | 'r' => {
                println!("Resetting client's last sync timestamp (was {}).", engine.last_sync);
                if let Err(e) = engine.reset() {
                    println!("Failed to reset! {}", e);
                }
            }
            'W' | 'w' => {
                println!("Wiping all data from client!");
                if let Err(e) = engine.wipe() {
                    println!("Failed to wipe! {}", e);
                }
            }
            'S' | 's' => {
                println!("Syncing!");
                if let Err(e) = engine.sync(&svc) {
                    println!("Sync failed! {}", e);
                } else {
                    println!("Sync was successful!");
                }
            }
            'V' | 'v' => {
                println!("Engine has {} records, a last sync timestamp of {}, and {} queued changes",
                         engine.records.len(), engine.last_sync, engine.changes.len());
                show_all(&engine);
            }
            'Q' | 'q' => {
                break;
            }
            '?' => {
                continue;
            }
            c => {
                println!("Unknown action '{}', exiting.", c);
                break;
            }
        }
    }

    println!("Exiting (bye!)");
    Ok(())
}
