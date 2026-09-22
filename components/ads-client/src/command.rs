use crate::{http_cache::CachePolicy, MozAdsPlacementRequestGeneric};
use std::collections::{HashMap, VecDeque};

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing and storing `DispatchCommand`s sent to background worker.
// Commands that can be merged together will be, allowing fewer network requests to be sent.
// (eg: multiple `RequestAd`s that have the same flags, ohttp values, etc.)
// `push(command)` and `next()` are the primary mechanisms of its use. Commands are merged together on `push`.
// TODO: This CommandQueue may have a hard sqlite backing for certain commands (eg: `ReportAd`) to ensure commands are not dropped.
pub struct CommandQueue {
    // The ordered queue of commands to iterate through.
    command_queue: VecDeque<CommandIdentifier>,

    // For `RequestAd` commands specifically, the arena tying the command identifier to a list of ad placements.
    ads_arena: HashMap<CommandIdentifier, Vec<MozAdsPlacementRequestGeneric>>,
}

impl CommandQueue {
    pub fn new() -> CommandQueue {
        CommandQueue {
            command_queue: VecDeque::new(),
            ads_arena: HashMap::new(),
        }
    }
    pub fn push(&mut self, command: DispatchCommand) {
        self.push_inner(command, false)
    }

    fn push_inner(&mut self, command: DispatchCommand, front: bool) {
        match command {
            // Primary RequestAds command.
            // We split this into `CommandIdentifier` and `MozAdsPlacementRequestGeneric` and store them in the queue.
            DispatchCommand::RequestAds {
                ad_requests,
                cache_policy,
                ohttp,
                flags,
                blocks,
            } => {
                let identifier = CommandIdentifier::RequestAds {
                    cache_policy,
                    ohttp,
                    flags: flags.into_iter().collect(),
                    blocks,
                };
                if let Some(entry) = self.ads_arena.get_mut(&identifier) {
                    entry.extend_from_slice(&ad_requests);
                } else {
                    self.ads_arena.insert(identifier.clone(), ad_requests);
                    if front {
                        self.command_queue.push_front(identifier);
                    } else {
                        self.command_queue.push_back(identifier);
                    }
                }
            }
        }
    }

    pub fn next(&mut self) -> Option<DispatchCommand> {
        let identifier = self.command_queue.pop_front()?;
        match identifier.clone() {
            // Stitch back together a `CommandIdentifier` and multiple `MozAdsPlacementRequestGeneric` into a `RequestAds`
            CommandIdentifier::RequestAds {
                cache_policy,
                ohttp,
                flags,
                blocks,
            } => {
                if let Some(mut ad_requests) = self.ads_arena.remove(&identifier) {
                    // Handling for a great number of ads- we split off the first `MAXIMUM_ADS_BATCH_COUNT` and return those.
                    // We push the remaining ad requests back to the front of the queue.
                    if ad_requests.len() > MAXIMUM_ADS_BATCH_COUNT {
                        let remaining_ads = ad_requests.split_off(MAXIMUM_ADS_BATCH_COUNT);
                        self.push_inner(
                            DispatchCommand::RequestAds {
                                ad_requests: remaining_ads,
                                cache_policy,
                                ohttp,
                                flags: flags.clone().into_iter().collect(),
                                blocks: blocks.clone(),
                            },
                            true,
                        )
                    }

                    Some(DispatchCommand::RequestAds {
                        ad_requests,
                        cache_policy,
                        ohttp,
                        flags: flags.into_iter().collect(),
                        blocks,
                    })
                } else {
                    // TODO: Telemetry should log an internal error (arena didn't line up)
                    self.next()
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.command_queue = VecDeque::new();
        self.ads_arena = HashMap::new();
    }
}

// Identifier of a command to allow separation of command metadata from the raw quantity of the command itself,
// allowing batching along common CommonIdentifier. For internal use in the CommandQueue.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
enum CommandIdentifier {
    RequestAds {
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: Vec<(String, bool)>,
        blocks: Vec<String>,
    },
}

// Command dispatch enum for passing different instructions to the background worker thread.
// `RequestImageAds`, `RequestSpocAds`, `RequestTileAds` are prefetch mechanisms that query and load data into the local cache.
#[derive(PartialEq, Clone, Debug)]
pub enum DispatchCommand {
    RequestAds {
        ad_requests: Vec<MozAdsPlacementRequestGeneric>,
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: HashMap<String, bool>,
        blocks: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        ads_store::PlacementId,
        command::{CommandQueue, DispatchCommand, MAXIMUM_ADS_BATCH_COUNT},
        http_cache::CachePolicy,
        MozAdsPlacementRequestGeneric,
    };

    fn example_request_ads() -> DispatchCommand {
        DispatchCommand::RequestAds {
            ad_requests: vec![MozAdsPlacementRequestGeneric {
                count: Some(4),
                placement_id: PlacementId::new("test_placement"),
                iab_content: None,
            }],
            cache_policy: CachePolicy::CacheFirst { ttl: None },
            ohttp: false,
            flags: HashMap::from([("example_flag".to_string(), true)]),
            blocks: vec![],
        }
    }

    #[test]
    fn identical_command_comes_out() {
        let command = example_request_ads();
        let mut queue = CommandQueue::new();

        queue.push(command.clone());
        let retrieved_command = queue.next().expect("Command should exist in queue");
        assert_eq!(command, retrieved_command);
    }

    #[test]
    fn batch_similar_requests() {
        let mut command = example_request_ads();
        let mut queue = CommandQueue::new();

        queue.push(command.clone());
        queue.push(command.clone());

        let retrieved_command = queue.next().expect("Command should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.ads_arena.is_empty());
        assert!(queue.command_queue.is_empty());

        // Modify `command` so it has more than one request inside.
        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds {
            ref mut ad_requests,
            ..
        } = command
        {
            ad_requests.push(ad_requests[0].clone());
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };
        assert_eq!(command, retrieved_command);
    }

    #[test]
    fn do_not_batch_different_requests() {
        let command = example_request_ads();
        let mut command_different = example_request_ads();
        let mut queue = CommandQueue::new();

        // Modify `command_different` so it doesn't batch.
        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds { ref mut ohttp, .. } = command_different {
            *ohttp = true;
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };

        queue.push(command.clone());
        queue.push(command_different.clone());

        let retrieved_command = queue.next().expect("First command should exist in queue");
        let retrieved_command_different =
            queue.next().expect("Second command should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.ads_arena.is_empty());
        assert!(queue.command_queue.is_empty());

        assert_eq!(command, retrieved_command);
        assert_eq!(command_different, retrieved_command_different);
    }

    #[test]
    fn split_off_too_many_ads() {
        let command = example_request_ads();
        let mut command_different = example_request_ads();

        let mut queue = CommandQueue::new();

        // Queue enough of this command to go one-over the limit.
        for _ in 0..(MAXIMUM_ADS_BATCH_COUNT + 1) {
            queue.push(command.clone());
        }

        // Modify `command_different` so it doesn't batch.
        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds { ref mut ohttp, .. } = command_different {
            *ohttp = true;
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };
        queue.push(command_different.clone());

        let retrieved_command = queue.next().expect("First command should exist in queue");
        let retrieved_command_overflow =
            queue.next().expect("Second command should exist in queue");
        let retrieved_command_different =
            queue.next().expect("Third command should exist in queue");

        assert!(queue.next().is_none());
        assert!(queue.ads_arena.is_empty());
        assert!(queue.command_queue.is_empty());

        assert_eq!(command, retrieved_command_overflow);

        // Retrieved command should have `MAXIMUM_ADS_BATCH_COUNT` requests.
        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds {
            ref ad_requests, ..
        } = retrieved_command
        {
            assert_eq!(ad_requests.len(), MAXIMUM_ADS_BATCH_COUNT)
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };

        // Retrieved overflow should have 1 request, matching the original command.
        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds {
            ref ad_requests, ..
        } = retrieved_command_overflow
        {
            assert_eq!(ad_requests.len(), 1)
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };
        assert_eq!(command_different, retrieved_command_different);
    }
}
