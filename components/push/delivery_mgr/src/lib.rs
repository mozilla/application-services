/* Handle delivery concerns around incoming notifications.
 * A notification may be discarded because of quota restrictions or may not require
 * encryption because it is priviledged.
 *
 * While this leans havily on data in Storage, the functions are separated out so that
 * Storage is only focused on actual data storage and retrieval.
 */

extern crate storage;

use storage::Storage;

pub trait DeliveryManager {
    fn new<D: DeliveryManager, S: Storage>(storage: S) -> D;

    // checks and increments quota (if needed)
    fn check_quota(chid: &str) -> bool;

    // resets quota back to zero.
    fn reset_quota(chid: &str) -> bool;

    // sets the quota for the chid.
    fn set_quota(chid: &str, quota: u64, system: bool);

    // is this a private, high priviledge "system" call?
    fn is_system(chid: &str) -> bool;

    // send the notification to the recipient.
    fn dispatch(chid: &str, content: Vec<u8>);
}

/*
struct Dispatch {
    storage: Storage
}
*/
