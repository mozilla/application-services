/* Process the incoming notification
 * Workhorse function that handles incoming notifications, processing them into PushMessages,
 * and distributing them to the appropriate handlers via the internal event bus.
 *
 * Called from the Connection Manager.
 */
extern crate storage;

pub struct NotifierError;

use storage::ChannelID;

// Incoming WebPush Notification
pub struct Notification {
    pub channel_id: ChannelID, // associated channel ID
    pub body: Vec<u8>,         // Raw body of the incoming notification
    pub con: String,           // encoding from Content-Encoding
    pub salt: Option<Vec<u8>>, // from encryption header
    pub dh: Option<Vec<u8>>,   // from crypto-key
}

// Outbound Push Message
pub struct PushMessage {
    pub channel_id: ChannelID,
    pub body: Vec<u8>,
}

pub trait Notifier {
    // process notification, broadcast, etc.
    fn process_notification(notification: Notification) -> Result<PushMessage, NotifierError>;
    // fetch sub data
    // decrypt the notification (if required)
    // route to proper handler (DOM, system)
    // the PushMessage result is handed off to the DOM or internal service
}

pub struct NotifHandler {}
