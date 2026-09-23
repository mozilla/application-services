use url::Url;

use crate::mars::{ad_request::AdPlacementRequest, ReportReason};
use std::collections::VecDeque;

pub const MAXIMUM_ADS_BATCH_COUNT: usize = 100;

// Structure for storing and queuing and storing `DispatchCommand`s sent to background worker.
// AdPlacementRequest will be merged together, allowing fewer network requests to be sent.
pub struct CommandQueue {
    queued_ads: Vec<AdPlacementRequest>,

    // The ordered queue of non-AdPlacementRequest commands to iterate through.
    command_queue: VecDeque<QueuedCommand>,
}

impl CommandQueue {
    pub fn new() -> CommandQueue {
        CommandQueue {
            queued_ads: Vec::new(),
            command_queue: VecDeque::new(),
        }
    }
    pub fn push_ad_request(&mut self, ad_request: AdPlacementRequest) {
        self.queued_ads.push(ad_request);
    }

    pub fn push_queued_command(&mut self, queued_command: QueuedCommand) {
        self.command_queue.push_back(queued_command);
    }

    pub fn next(&mut self) -> Option<DispatchCommand> {
        // First, if any ad requests are queued, batch the first `MAXIMUM_ADS_BATCH_COUNT` and resolve those.
        let num_ads = self.queued_ads.len().min(MAXIMUM_ADS_BATCH_COUNT);
        let ad_requests: Vec<_> = self.queued_ads.drain(..num_ads).collect();
        if ad_requests.len() > 0 {
            return Some(DispatchCommand::RequestAds { ad_requests });
        }

        // Otherwise, pop the next queue-able command.
        self.command_queue.pop_front().map(|c| c.into())
    }

    pub fn clear(&mut self) {
        self.command_queue = VecDeque::new();
        self.queued_ads = Vec::new();
    }
}

// Queue-able command (ReportAd, etc.)
#[derive(Debug, Clone, PartialEq)]
pub enum QueuedCommand {
    RecordClick { url: Url },
    RecordImpression { url: Url },
    ReportAd { url: Url, reason: ReportReason },
}

// Command dispatch enum for passing different instructions to the background worker thread.
// `RequestAds` are prefetch mechanisms that query and load data into the local cache.
#[derive(Clone, Debug, PartialEq)]
pub enum DispatchCommand {
    RequestAds {
        ad_requests: Vec<AdPlacementRequest>,
    },
    RecordClick {
        url: Url,
    },
    RecordImpression {
        url: Url,
    },
    ReportAd {
        url: Url,
        reason: ReportReason,
    },
}

impl From<QueuedCommand> for DispatchCommand {
    fn from(value: QueuedCommand) -> Self {
        match value {
            QueuedCommand::ReportAd { url, reason } => DispatchCommand::ReportAd { url, reason },
            QueuedCommand::RecordClick { url } => DispatchCommand::RecordClick { url },
            QueuedCommand::RecordImpression { url } => DispatchCommand::RecordImpression { url },
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::command::{
        AdPlacementRequest, CommandQueue, DispatchCommand, MAXIMUM_ADS_BATCH_COUNT,
    };

    fn example_request_ads() -> AdPlacementRequest {
        AdPlacementRequest {
            count: 4,
            placement: "test_placement".to_string(),
            content: None,
        }
    }

    fn example_request_ads_command() -> DispatchCommand {
        DispatchCommand::RequestAds {
            ad_requests: vec![AdPlacementRequest {
                count: 4,
                placement: "test_placement".to_string(),
                content: None,
            }],
        }
    }

    #[test]
    fn identical_command_comes_out() {
        let request = example_request_ads();
        let command = example_request_ads_command();
        let mut queue = CommandQueue::new();

        queue.push_ad_request(request.clone());
        let retrieved_command = queue.next().expect("Command should exist in queue");
        assert_eq!(command, retrieved_command);
    }

    #[test]
    fn batch_similar_requests() {
        let request = example_request_ads();
        let mut command = example_request_ads_command();
        let mut queue = CommandQueue::new();

        queue.push_ad_request(request.clone());
        queue.push_ad_request(request.clone());

        let retrieved_command = queue.next().expect("Command should exist in queue");
        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
        assert!(queue.command_queue.is_empty());

        // Modify `request` so it has more than one request inside.
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
    fn split_off_too_many_ads() {
        let request = example_request_ads();
        let command = example_request_ads_command();
        let mut queue = CommandQueue::new();

        // Queue enough of this command to go one-over the limit.
        for _ in 0..(MAXIMUM_ADS_BATCH_COUNT + 1) {
            queue.push_ad_request(request.clone());
        }

        let retrieved_command = queue.next().expect("First command should exist in queue");
        let retrieved_command_overflow =
            queue.next().expect("Second command should exist in queue");

        assert!(queue.next().is_none());
        assert!(queue.queued_ads.is_empty());
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
    }
}
