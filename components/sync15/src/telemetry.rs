/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Manage recording sync telemetry. Assumes some external telemetry
// library/code which manages submitting.

use std::collections::HashMap;
use std::time;
use std::mem;

use num::Num;

// We use serde to serialize to json (but we never need to deserialize)
use serde_derive::*;
use serde::ser::{Serialize, Serializer};
#[cfg(test)]
use serde_json::{self, json};

// For skip_serializing_if
// I'm surprised I can't use Num::is_zero directly, although Option::is_none works.
// (Num's just a trait, Option is an enum, but surely there's *some* way?)
fn skip_if_zero<T: Num + PartialEq>(v: &T) -> bool {
    return v.is_zero();
}

// A test helper, used by the many test modules below. Is there a better way
// to structure tests?
#[cfg(test)]
fn assert_json<T: ?Sized>(v: &T, expected: serde_json::Value )
   where T: serde::Serialize {
    assert_eq!(serde_json::to_value(&v).expect("should get a value"), expected);
}

// ????
#[derive(Debug, Serialize)]
struct Clock {
    when: f64,
    #[serde(skip_serializing_if = "skip_if_zero")]
    took: u64,
}

#[derive(Debug)]
enum WhenTook {
    Started(time::SystemTime, time::Instant),
    Finished(Clock),
}

impl WhenTook {
    fn new() -> Self {
        WhenTook::Started(time::SystemTime::now(), time::Instant::now())
    }

    fn finished(self) -> Self {
        match cfg!(test) {
            false => self._finished(),
            _ => WhenTook::Finished(Clock {when: 0.0, took: 0}),
        }
    }

    fn _finished(self) -> Self {
        match self {
            WhenTook::Started(st, si) => {
                let std = st.duration_since(time::UNIX_EPOCH).unwrap_or(time::Duration::new(0, 0));
                let when = std.as_secs() as f64; // we don't want sub-sec accuracy. Do we need to write a float?

                let sid = si.elapsed();
                let took = sid.as_secs() * 1000 + (sid.subsec_nanos() as u64) / 1_000_000;
                WhenTook::Finished(Clock {when, took})
            },
            _ => {
                panic!("can't finish twice");
            }
        }
    }

 }

 impl Serialize for WhenTook {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where S: Serializer {
        match self {
            WhenTook::Started(_,_) => {
                panic!("must be finished")
            },
            WhenTook::Finished(c) => {
                c.serialize(serializer)
            }
        }
    }
}

#[cfg(test)]
mod whentook_tests {
    use super::*;

    #[derive(Debug, Serialize)]
    struct WT {
        #[serde(flatten)]
        wt: WhenTook,
    }

    #[should_panic]
    #[test]
    fn test_not_finished() {
        let wt = WT {wt: WhenTook::new()};
        serde_json::to_string(&wt).expect("a panic!");
    }

    #[test]
    fn test_finished() {
        let mut wt = WT {wt: WhenTook::new()};
        wt.wt = wt.wt._finished();
        let len = serde_json::to_string(&wt).expect("should get json").len();
        assert!(len > 20); // should have both keys, sane values. regex check?
    }

    #[test]
    fn test() {
        assert_json(&WT {wt: WhenTook::Finished(Clock {when: 1.0, took: 1})},
                    json!({"when": 1.0, "took": 1}));
        assert_json(&WT {wt: WhenTook::Finished(Clock {when: 1.0, took: 0})},
                    json!({"when": 1.0}));
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// A generic "Event" - suitable for all kinds of pings!
// (although currently we only support the sync ping...)
//
#[derive(Debug, Serialize)]
pub struct Event {
    // We use static str references as we expect values to be literals.
    object: &'static str,

    method: &'static str,

    // Maybe "value" should be a string?
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'static str>,

    // we expect the keys to be literals but values are real strings.
    #[serde(skip_serializing_if = "Option::is_none")]
    extra: Option<HashMap<&'static str, String>>
}

impl Event {
    pub fn new(object: &'static str,
               method: &'static str) -> Self {
        assert!(object.len() <= 20);
        assert!(method.len() <= 20);
        Self {object, method, value: None, extra: None}
    }

    pub fn value(mut self, v: &'static str) -> Self {
        assert!(v.len() <= 80);
        self.value = Some(v);
        self
    }

    pub fn extra(mut self, key: &'static str, val: String) -> Self {
        assert!(key.len() <= 15);
        assert!(val.len() <= 85);
        match self.extra {
            None => self.extra = Some(HashMap::new()),
            Some(ref e) => assert!(e.len() < 10),
        }
        self.extra.as_mut().unwrap().insert(key, val);
        self
    }
}

#[cfg(test)]
mod test_events {
    use super::*;

    #[test]
    #[should_panic]
    fn test_invalid_length_ctor() {
        Event::new("A very long object value", "Method");
    }

    #[test]
    #[should_panic]
    fn test_invalid_length_extra_key() {
        Event::new("O", "M").extra("A very long key value", "v".to_string());
    }

    #[test]
    #[should_panic]
    fn test_invalid_length_extra_val() {
        let l = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ
                abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        Event::new("O", "M").extra("k", l.to_string());
    }


    #[test]
    #[should_panic]
    fn test_too_many_extras() {
        let l = "abcdefghijk";
        let mut e = Event::new("Object", "Method");
        for i in 0..l.len() {
            e = e.extra(&l[i..i+1], "v".to_string());
        }
    }

    #[test]
    fn test_json() {
        assert_json(&Event::new("Object", "Method").value("Value"),
                    json!({"object": "Object", "method": "Method", "value": "Value"}));

        assert_json(&Event::new("Object", "Method").extra("one", "one".to_string()),
                    json!({"object": "Object",
                           "method": "Method",
                           "extra": {"one": "one"}
                          })
                    )
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// A Sync failure.
//
#[derive(Debug, Serialize)]
#[serde(tag = "name")]
pub enum SyncFailure {
    #[serde(rename = "shutdownerror")]
    Shutdown,

    #[serde(rename = "othererror")]
    Other {error: String},

    #[serde(rename = "unexpectederror")]
    Unexpected {error: String},

    #[serde(rename = "autherror")]
    Auth {from: String},

    #[serde(rename = "httperror")]
    Http {code: u32},

    #[serde(rename = "nserror")]
    Nserror {code: i32}, // probably doesn't really make sense in rust, but here we are...
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn reprs() {
        assert_json(&SyncFailure::Shutdown,
                    json!({"name": "shutdownerror"}));

        assert_json(&SyncFailure::Other {error: "dunno".to_string()},
                    json!({"name": "othererror", "error": "dunno"}));

        assert_json(&SyncFailure::Unexpected {error: "dunno".to_string()},
                    json!({"name": "unexpectederror", "error": "dunno"}));

        assert_json(&SyncFailure::Auth {from: "FxA".to_string()},
                    json!({"name": "autherror", "from": "FxA"}));

        assert_json(&SyncFailure::Http {code: 500},
                    json!({"name": "httperror", "code": 500}));

        assert_json(&SyncFailure::Nserror {code: -1},
                    json!({"name": "nserror", "code": -1}));
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// Incoming and Outgoing records for an engine's sync
//
#[derive(Debug, Serialize)]
pub struct EngineIncoming {
    #[serde(skip_serializing_if = "skip_if_zero")]
    applied: u32,

    #[serde(skip_serializing_if = "skip_if_zero")]
    failed: u32,

    #[serde(rename = "newFailed")]
    #[serde(skip_serializing_if = "skip_if_zero")]
    new_failed: u32,

    #[serde(skip_serializing_if = "skip_if_zero")]
    reconciled: u32,
}

impl EngineIncoming {
    fn new() -> Self {
        Self {applied: 0, failed: 0, new_failed: 0, reconciled: 0}
    }

    fn is_empty(inc: &Option<Self>) -> bool {
        match inc {
            Some(a) => {
                a.applied == 0 && a.failed == 0 && a.new_failed == 0 && a.reconciled == 0
            },
            None => true
        }
    }

    // builder
    pub fn applied(mut self, n: u32) -> Self {
        self.applied = n;
        self
    }

    pub fn failed(mut self, n: u32) -> Self {
        self.failed = n;
        self
    }

    pub fn new_failed(mut self, n: u32) -> Self {
        self.new_failed = n;
        self
    }

    pub fn reconciled(mut self, n: u32) -> Self {
        self.reconciled = n;
        self
    }

}

#[derive(Debug, Serialize)]
pub struct EngineOutgoing {
    #[serde(skip_serializing_if = "skip_if_zero")]
    sent: u32,

    #[serde(skip_serializing_if = "skip_if_zero")]
    failed: u32,
}

impl EngineOutgoing {
    pub fn new() -> Self {
        EngineOutgoing {sent: 0, failed: 0}
    }

    pub fn sent(mut self, n: u32) -> Self {
        self.sent = n;
        self
    }

    pub fn failed(mut self, n: u32) -> Self {
        self.failed = n;
        self
    }
}

// One engine's sync.
#[derive(Debug, Serialize)]
struct Engine {
    name: String,

    #[serde(flatten)]
    when_took: WhenTook,

    #[serde(skip_serializing_if = "EngineIncoming::is_empty")]
    incoming: Option<EngineIncoming>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    outgoing: Vec<EngineOutgoing>, // one for each batch posted.

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "failureReason")]
    failure: Option<SyncFailure>,
}

impl Engine {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            when_took: WhenTook::new(),
            incoming: None,
            outgoing: Vec::new(),
            failure: None,
        }
    }

    pub fn incoming(mut self, inc: EngineIncoming) -> Self {
        assert!(self.incoming.is_none());
        self.incoming = Some(inc);
        self
    }

    pub fn outgoing(mut self, out: EngineOutgoing) -> Self {
        self.outgoing.push(out);
        self
    }

    pub fn failure(mut self, failure: SyncFailure) -> Self {
        assert!(self.failure.is_none());
        self.failure = Some(failure);
        self
    }

    pub fn finished(mut self) -> Self {
        self.when_took = self.when_took.finished();
        self
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn test_engine() {
        let e = Engine::new("test_engine").finished();
        assert_json(&e,
                    json!({"name": "test_engine", "when": 0.0}));
    }

    #[test]
    fn test_incoming() {
        let e = Engine::new("TestEngine");
        let i = EngineIncoming::new().applied(1).failed(2);
        let e = e.incoming(i).finished();
        assert_json(&e,
                    json!({"name": "TestEngine", "when": 0.0, "incoming": {"applied": 1, "failed": 2}}));
    }

    #[test]
    fn test_outgoing() {
        let e = Engine::new("TestEngine");
        let o = EngineOutgoing::new();
        let o = o.sent(2).failed(1);
        let e = e.outgoing(o);
        assert_json(&e.finished(),
                    json!({"name": "TestEngine", "when": 0.0, "outgoing": [{"sent": 2, "failed": 1}]}));
    }

    #[test]
    fn test_failure() {
        let e = Engine::new("TestEngine");
        let e = e.failure(SyncFailure::Http {code: 500});
        assert_json(&e.finished(),
                    json!({"name": "TestEngine",
                           "when": 0.0,
                           "failureReason": {"name": "httperror", "code": 500}
                          })
                    );
    }

    #[test]
    fn test_raw() {
        let e = Engine::new("TestEngine");
        let e = e.incoming(EngineIncoming::new().applied(10));
        let e = e.outgoing(EngineOutgoing::new().sent(1));
        let e = e.failure(SyncFailure::Http {code: 500});
        let e = e.finished();

        assert_eq!(e.outgoing.len(), 1);
        assert_eq!(e.incoming.as_ref().unwrap().applied, 10);
        assert_eq!(e.outgoing[0].sent, 1);
        assert!(e.failure.is_some());
        serde_json::to_string(&e).expect("should get json");
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// A single sync. May have many engines, may have its own failure.
//
#[derive(Debug, Serialize)]
struct Sync {
    #[serde(flatten)]
    when_took: WhenTook,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    engines: Vec<Engine>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "failureReason")]
    failure: Option<SyncFailure>,
}

impl Sync {
    fn new() -> Self {
        Self {
            when_took: WhenTook::new(),
            engines: Vec::new(),
            failure: None,
        }
    }

    pub fn engine(mut self, mut e: Engine) -> Self {
        e.when_took = e.when_took.finished();
        self.engines.push(e);
        self
    }

    pub fn failure(mut self, failure: SyncFailure) -> Self {
        assert!(self.failure.is_none());
        self.failure = Some(failure);
        self
    }

    pub fn finished(mut self) -> Self {
        self.when_took = self.when_took.finished();
        self
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn test_accum() {
        let s = Sync::new();
        let e = Engine::new("test_engine").
                    failure(SyncFailure::Http {code: 500}).
                    incoming(EngineIncoming::new().applied(10)).
                    finished();
        let s = s.engine(e).finished();
        assert_json(&s, json!({
            "when": 0.0,
            "engines": [{
                "name":"test_engine",
                "when":0.0,
                "incoming": {
                    "applied": 10
                },
                "failureReason": {
                    "name": "httperror",
                    "code": 500
                }
            }]
        }));
    }

    #[test]
    fn test_multi_engine() {
        let s = Sync::new();
        let s = s.engine(Engine::new("test_engine").incoming(EngineIncoming::new().applied(1)));
        let e = Engine::new("test_engine_2").incoming(EngineIncoming::new().failed(1));
        let e = e.outgoing(EngineOutgoing::new().sent(1)).finished();
        let s = s.engine(e);
        let s = s.failure(SyncFailure::Http {code: 500});
        assert_json(&s.finished(), json!({
            "when": 0.0,
            "engines": [{
                "name": "test_engine",
                "when": 0.0,
                "incoming": {
                    "applied": 1
                }
            },{
                "name": "test_engine_2",
                "when": 0.0,
                "incoming": {
                    "failed": 1
                },
                "outgoing": [{
                    "sent": 1
                }]
            }],
            "failureReason": {
                "name": "httperror",
                "code": 500
            }
        }));
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// The Sync ping payload
// May have many syncs, may have many events.
// Private - see the ???
//
#[derive(Serialize, Debug)]
struct Payload {
    version: u32,
    // eg, os, why, ...

    uid: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    events: Vec<Event>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    syncs: Vec<Sync>,
}

impl Payload {
    pub fn new() -> Self {
        Self {
            version: 1,
            uid: None,
            events: Vec::new(),
            syncs:Vec::new(),
        }
    }

    fn sync(&mut self, mut sync: Sync) {
        sync.when_took = sync.when_took.finished();
        self.syncs.push(sync);
    }

    fn event(&mut self, event: Event) {
        self.events.push(event);
    }

    // uid must already be hashed or otherwise anonymised - it is submitted
    // as specified.
    pub fn uid(&mut self, uid: String) {
    // TODO: ditto device-id
        self.uid = Some(uid);
    }
}

#[cfg(test)]
mod test_payload {
    use super::*;
    #[test]
    fn test_payload() {
        let mut p = Payload::new();
        p.sync(Sync::new().
                    engine(Engine::new("test-engine").
                        incoming(EngineIncoming::new().applied(1).failed(2)).
                        outgoing(EngineOutgoing::new().sent(2).failed(1)).
                        finished()
                    ).finished()
                );
        assert_json(&p, json!({
            "version": 1,
            "uid": null,
            "syncs": [{
                "when": 0.0,
                "engines": [{
                    "name": "test-engine",
                    "when": 0.0,
                    "incoming": {
                        "applied": 1,
                        "failed": 2
                    },
                    "outgoing": [{
                        "sent": 2,
                        "failed": 1
                    }]
                }]
            }]
        }));
    }
}

//////////////////////////////////////////////////////////////////////////////
//
// The top-level submission management
//
trait Submitter {
    // Does the submitter want to submit the ping at this point?
    // (eg, might return false until a number of pings or a time period is met)
    fn want(&mut self, payload: &Payload) -> bool;
    // Take it for submission - even if you don't want it!
    // (eg, something in the ping changed, like the UID.
    fn take(&mut self, payload: Payload);
}


#[derive(Debug)]
struct Something<S> {
    payload: Payload,
    submitter: S,
}

impl<S: Submitter> Something<S> {
    pub fn new(submitter: S) -> Self {
        Self {
            payload: Payload::new(),
            submitter,
        }
    }

    // delegate methods for the payload.
    fn sync(&mut self, sync: Sync) {
        self.payload.sync(sync);
        self.maybe_submit();
    }

    fn event(&mut self, event: Event) {
        self.payload.event(event);
        self.maybe_submit();
    }

    fn uid(&mut self, uid: String) {
        // uid change forces a submission of whatever we have now before we
        // record the new uid.
        // XXX - WTF is up with this!? Just to check a string value? :(
        if self.payload.uid.is_some() && self.payload.uid.as_ref().unwrap() != &uid {
            self.force_submit();
        }
        self.payload.uid(uid);
    }

    fn maybe_submit(&mut self) {
        if self.submitter.want(&self.payload) {
            self.force_submit();
        }
    }

    fn force_submit(&mut self) {
        let to_submit = mem::replace(&mut self.payload, Payload::new());
        self.submitter.take(to_submit);
    }
}

// XXX - should be test only, but until we get a concrete submitter...
#[derive(Default, Debug)]
pub struct TestSubmitter {
    num_want: u32,
    num_take: u32,
}

impl TestSubmitter {
    pub fn new() -> TestSubmitter {
        TestSubmitter {..Default::default()}
    }
}

impl Submitter for TestSubmitter {
    fn want(&mut self, payload: &Payload) -> bool {
        let j = serde_json::to_string_pretty(&payload).expect("should be able to stringify");
        println!("asked if I want {}", j);
        self.num_want += 1;
        false
    }
    fn take(&mut self, payload: Payload) {
        let j = serde_json::to_string_pretty(&payload).expect("should be able to stringify");
        println!("submitting {}", j);
        self.num_take += 1;
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_something() {
        let mut spb = Something::new(TestSubmitter::new());

        let engine = Engine::new("test_engine").incoming(
            EngineIncoming::new().applied(2).failed(1)
        );
        spb.sync(Sync::new().engine(engine));
        assert_eq!(&spb.submitter.num_want, &1, "should have been asked");
        assert_eq!(&spb.submitter.num_take, &0, "should have declined");
    }

    #[test]
    fn test_uid_change_submits() {
        let mut spb = Something::new(TestSubmitter::new());
        spb.event(Event::new("Object", "Method"));

        assert_eq!(&spb.submitter.num_want, &1, "should have been asked");
        assert_eq!(&spb.submitter.num_take, &0, "should have declined");

        // None => Some
        spb.uid("foo".to_string());
        assert_eq!(&spb.submitter.num_want, &1, "should have not been asked again");
        assert_eq!(&spb.submitter.num_take, &0, "should still have declined");

        // Some(x) => Some(x)
        spb.uid("foo".to_string());
        assert_eq!(&spb.submitter.num_want, &1, "should have not been asked again");
        assert_eq!(&spb.submitter.num_take, &0, "should still have declined");

        // Some(x) => Some(y)
        spb.uid("bar".to_string());
        assert_eq!(&spb.submitter.num_want, &1, "still not asked");
        assert_eq!(&spb.submitter.num_take, &1, "should have been force submitted");
    }
}
