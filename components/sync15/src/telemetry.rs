/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// Manage recording sync telemetry. Assumes some external telemetry
// library/code which manages submitting.

use std::collections::HashMap;
use std::time;

use num::Num;

// We use serde to serialize to json (but we never need to deserialize)
use serde_derive::*;
use serde::ser::{Serialize, Serializer};
#[cfg(test)]
use serde_json::{self, json};

use crate::error::Error;

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

/// What we record for 'when' and 'took' in a telemetry record.
#[derive(Debug, Serialize)]
struct WhenTook {
    when: f64,
    #[serde(skip_serializing_if = "skip_if_zero")]
    took: u64,
}

/// What we track while recording 'when' and 'took. It serializes as a WhenTook,
/// except when .finished() hasn't been called, in which case it panics.
#[derive(Debug)]
enum Stopwatch {
    Started(time::SystemTime, time::Instant),
    Finished(WhenTook),
}

impl Stopwatch {
    fn new() -> Self {
        Stopwatch::Started(time::SystemTime::now(), time::Instant::now())
    }

    fn finished(&self) -> Self {
        match cfg!(test) {
            false => self._finished(),
            _ => Stopwatch::Finished(WhenTook {when: 0.0, took: 0}),
        }
    }

    fn _finished(&self) -> Self {
        match self {
            Stopwatch::Started(st, si) => {
                let std = st.duration_since(time::UNIX_EPOCH).unwrap_or(time::Duration::new(0, 0));
                let when = std.as_secs() as f64; // we don't want sub-sec accuracy. Do we need to write a float?

                let sid = si.elapsed();
                let took = sid.as_secs() * 1000 + (sid.subsec_nanos() as u64) / 1_000_000;
                Stopwatch::Finished(WhenTook {when, took})
            },
            _ => {
                panic!("can't finish twice");
            }
        }
    }

 }

 impl Serialize for Stopwatch {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where S: Serializer {
        match self {
            Stopwatch::Started(_,_) => {
                panic!("must be finished")
            },
            Stopwatch::Finished(c) => {
                c.serialize(serializer)
            }
        }
    }
}

#[cfg(test)]
mod stopwatch_tests {
    use super::*;

    // A wrapper struct because we flatten - this struct should serialize with
    // 'when' and 'took' keys (but with no 'sw'.)
    #[derive(Debug, Serialize)]
    struct WT {
        #[serde(flatten)]
        sw: Stopwatch,
    }

    #[should_panic]
    #[test]
    fn test_not_finished() {
        let wt = WT {sw: Stopwatch::new()};
        serde_json::to_string(&wt).expect("a panic!");
    }

    #[test]
    fn test() {
        assert_json(&WT {sw: Stopwatch::Finished(WhenTook {when: 1.0, took: 1})},
                    json!({"when": 1.0, "took": 1}));
        assert_json(&WT {sw: Stopwatch::Finished(WhenTook {when: 1.0, took: 0})},
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

pub fn sync_failure_from_error(e: &Error) -> SyncFailure {
    SyncFailure::Unexpected {error: e.to_string()}
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
    pub fn new() -> Self {
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

    pub fn applied(&mut self, n: u32) {
        self.applied += n;
    }

    pub fn failed(&mut self, n: u32) {
        self.failed += n;
    }

    pub fn new_failed(&mut self, n: u32) {
        self.new_failed += n;
    }

    pub fn reconciled(&mut self, n: u32) {
        self.reconciled += n;
    }

}

#[derive(Debug, Serialize)]
pub struct EngineOutgoing {
    #[serde(skip_serializing_if = "skip_if_zero")]
    sent: usize,

    #[serde(skip_serializing_if = "skip_if_zero")]
    failed: usize,
}

impl EngineOutgoing {
    pub fn new() -> Self {
        EngineOutgoing {sent: 0, failed: 0}
    }

    pub fn sent(&mut self, n: usize) {
        self.sent += n;
    }

    pub fn failed(&mut self, n: usize) {
        self.failed += n;
    }
}

// One engine's sync.
#[derive(Debug, Serialize)]
pub struct Engine {
    name: String,

    #[serde(flatten)]
    when_took: Stopwatch,

    #[serde(skip_serializing_if = "EngineIncoming::is_empty")]
    incoming: Option<EngineIncoming>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    outgoing: Vec<EngineOutgoing>, // one for each batch posted.

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "failureReason")]
    failure: Option<SyncFailure>,
}

impl Engine {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            when_took: Stopwatch::new(),
            incoming: None,
            outgoing: Vec::new(),
            failure: None,
        }
    }

    pub fn incoming(&mut self, inc: EngineIncoming) {
        assert!(self.incoming.is_none());
        self.incoming = Some(inc);
    }

    pub fn outgoing(&mut self, out: EngineOutgoing) {
        self.outgoing.push(out);
    }

    pub fn failure(&mut self, failure: SyncFailure) {
        assert!(self.failure.is_none());
        self.failure = Some(failure);
    }

    fn finished(&mut self) {
        self.when_took = self.when_took.finished();
    }
}

#[cfg(test)]
mod engine_tests {
    use super::*;

    #[test]
    fn test_engine() {
        let mut e = Engine::new("test_engine");
        e.finished();
        assert_json(&e,
                    json!({"name": "test_engine", "when": 0.0}));
    }

    #[should_panic]
    #[test]
    fn test_engine_not_finished() {
        let e = Engine::new("test_engine");
        assert_json(&e, json!({}));
    }

    #[test]
    fn test_incoming() {
        let mut i = EngineIncoming::new();
        i.applied(1);
        i.failed(2);
        let mut e = Engine::new("TestEngine");
        e.incoming(i);
        e.finished();
        assert_json(&e,
                    json!({"name": "TestEngine", "when": 0.0, "incoming": {"applied": 1, "failed": 2}}));
    }

    #[test]
    fn test_outgoing() {
        let mut o = EngineOutgoing::new();
        o.sent(2);
        o.failed(1);
        let mut e = Engine::new("TestEngine");
        e.outgoing(o);
        e.finished();
        assert_json(&e,
                    json!({"name": "TestEngine", "when": 0.0, "outgoing": [{"sent": 2, "failed": 1}]}));
    }

    #[test]
    fn test_failure() {
        let mut e = Engine::new("TestEngine");
        e.failure(SyncFailure::Http {code: 500});
        e.finished();
        assert_json(&e,
                    json!({"name": "TestEngine",
                           "when": 0.0,
                           "failureReason": {"name": "httperror", "code": 500}
                          })
                    );
    }

    #[test]
    fn test_raw() {
        let mut e = Engine::new("TestEngine");
        let mut inc = EngineIncoming::new();
        inc.applied(10);
        e.incoming(inc);
        let mut out = EngineOutgoing::new();
        out.sent(1);
        e.outgoing(out);
        e.failure(SyncFailure::Http {code: 500});
        e.finished();

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
pub struct Sync {
    #[serde(flatten)]
    when_took: Stopwatch,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    engines: Vec<Engine>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "failureReason")]
    failure: Option<SyncFailure>,
}

impl Sync {
    pub fn new() -> Self {
        Self {
            when_took: Stopwatch::new(),
            engines: Vec::new(),
            failure: None,
        }
    }

    pub fn engine(&mut self, mut e: Engine) {
        e.finished();
        self.engines.push(e);
    }

    pub fn failure(&mut self, failure: SyncFailure) {
        assert!(self.failure.is_none());
        self.failure = Some(failure);
    }

    // Note that unlike other 'finished' methods, this isn't private - someone
    // needs to explicitly call this before handling the json payload to
    // whatever ends up submitting it.
    pub fn finished(&mut self) {
        self.when_took = self.when_took.finished();
    }
}

#[cfg(test)]
mod sync_tests {
    use super::*;

    #[test]
    fn test_accum() {
        let mut s = Sync::new();
        let mut inc = EngineIncoming::new();
        inc.applied(10);
        let mut e = Engine::new("test_engine");
        e.incoming(inc);
        e.failure(SyncFailure::Http {code: 500});
        e.finished();
        s.engine(e);
        s.finished();

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
        let mut inc_e1 = EngineIncoming::new();
        inc_e1.applied(1);
        let mut e1 = Engine::new("test_engine");
        e1.incoming(inc_e1);

        let mut inc_e2 = EngineIncoming::new();
        inc_e2.failed(1);
        let mut e2 = Engine::new("test_engine_2");
        e2.incoming(inc_e2);
        let mut out_e2 = EngineOutgoing::new();
        out_e2.sent(1);
        e2.outgoing(out_e2);

        let mut s = Sync::new();
        s.engine(e1);
        s.engine(e2);
        s.failure(SyncFailure::Http {code: 500});
        s.finished();
        assert_json(&s, json!({
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

/**************************************

kill this - external component will manage!

however, need to think through uid/deviceid management

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
        let mut inc = EngineIncoming::new();
        inc.applied(1);
        inc.failed(2);
        let mut out = EngineOutgoing::new();
        out.sent(2);
        out.failed(1);

        let mut e = Engine::new("test-engine");
        e.incoming(inc);
        e.outgoing(out);

        let mut s = Sync::new();
        s.engine(e);
        s.finished();

        let mut p = Payload::new();
        p.sync(s);

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

        let mut inc = EngineIncoming::new();
        inc.applied(2);
        inc.failed(1);
        let mut engine = Engine::new("test_engine");
        engine.incoming(inc);
        let mut sync = Sync::new();
        sync.engine(engine);
        spb.sync(sync);
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

*/