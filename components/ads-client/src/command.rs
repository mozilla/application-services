use std::collections::{HashMap, VecDeque};
use crate::{
    MozAdsPlacementRequestGeneric, http_cache::CachePolicy,
};

// Structure for storing and queuing and storing `DispatchCommand`s sent to background worker.
// Commands that can be merged together will be, allowing fewer network requests to be sent. 
// (eg: multiple `RequestAd`s that have the same flags, ohttp values, etc.)
// `push(command)` and `next()` are the primary mechanisms of its use. Commands are merged together on `push`.
// TODO: This CommandQueue may have a hard sqlite backing for certain commands (eg: `ReportAd`) to ensure commands are not dropped.
pub struct CommandQueue {
    // The ordered queue of commands to iterate through.
    command_queue : VecDeque<CommandIdentifier>,

    // For `RequestAd` commands specifically, the arena tying the command identifier to a list of ad placements.
    ads_arena : HashMap<CommandIdentifier, Vec<MozAdsPlacementRequestGeneric>>,
}

impl CommandQueue {
    pub fn new() -> CommandQueue {
        CommandQueue { command_queue: VecDeque::new(), ads_arena: HashMap::new() }
    }
    pub fn push(&mut self, command : DispatchCommand) {
        match command {
            // Primary RequestAds command.
            // We split this into `CommandIdentifier` and `MozAdsPlacementRequestGeneric` and store them in the queue. 
            DispatchCommand::RequestAds { ad_requests, cache_policy, ohttp, flags, blocks } => {
                let identifier = CommandIdentifier::RequestAds { cache_policy, ohttp, flags: flags.into_iter().collect(), blocks };  
                if let Some(entry) = self.ads_arena.get_mut(&identifier) {
                    entry.extend_from_slice(&ad_requests);
                } else {
                    self.ads_arena.insert(identifier.clone(), ad_requests);
                    self.command_queue.push_back(identifier);
                }

            }
        }
    }

    pub fn next(&mut self) -> Option<DispatchCommand> {
        let identifier = self.command_queue.pop_front()?;
        match identifier.clone() {
            // Stitch back together a `CommandIdentifier` and multiple `MozAdsPlacementRequestGeneric` into a `RequestAds`
            CommandIdentifier::RequestAds { cache_policy, ohttp, flags, blocks } => {
                // TODO: Some upper limit on the amount we can send at once?
                if let Some(ad_requests) = self.ads_arena.remove(&identifier) {
                    Some(DispatchCommand::RequestAds { ad_requests, cache_policy, ohttp, flags: flags.into_iter().collect(), blocks })
                } else {
                    // TODO: Telemetry, internal error (arena didn't line up)
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
#[derive(Clone, Hash, PartialEq, Eq)]
enum CommandIdentifier {
    RequestAds {
        cache_policy: CachePolicy,
        ohttp: bool,
        flags: Vec<(String, bool)>,
        blocks: Vec<String>,
    }
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

use crate::{MozAdType, MozAdsPlacementRequestGeneric, ads_store::PlacementId, command::{CommandQueue, DispatchCommand}, http_cache::CachePolicy};

    fn example_request_ads() -> DispatchCommand {
        DispatchCommand::RequestAds { ad_requests: vec![
            MozAdsPlacementRequestGeneric {
                count: Some(4),
                placement_id: PlacementId::new("test_placement"),
                iab_content: None,
                ad_type: MozAdType::Spoc
            }
        ], cache_policy: CachePolicy::CacheFirst { ttl: None }, ohttp: false, flags: HashMap::from([("example_flag".to_string(), true)]), blocks: vec![] }
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

        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds { ref mut ad_requests, .. } = command {
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

        #[allow(irrefutable_let_patterns)]
        if let DispatchCommand::RequestAds { ref mut ohttp, .. } = command_different {
            *ohttp = true;
        } else {
            panic!("Example DispatchCommand should be RequestAds variant");
        };

        queue.push(command.clone());
        queue.push(command_different.clone());

        let retrieved_command = queue.next().expect("First command should exist in queue");
        let retrieved_command_different = queue.next().expect("Second command should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.ads_arena.is_empty());
        assert!(queue.command_queue.is_empty());

        assert_eq!(command, retrieved_command);
        assert_eq!(command_different, retrieved_command_different);
    }
}
