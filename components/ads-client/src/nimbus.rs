use std::collections::HashMap;

// At the current moment, nimbus usage exclusively inside of rust components is not fully realized.
// Therefore, for now we will accept a series of string flags passed in from each surface (that are connected to nimbus).
// The ads-client can then branch behavior based on the passed flags.
#[derive(Clone, Default)]
pub struct NimbusFlags {
    flags: HashMap<NimbusFlag, bool>,
}

impl NimbusFlags {
    // Parse and store flags in constructor.
    // Allow unrecognized flags to be read as `Unknown`, so that extra irrelevant flags can be easily passed.
    pub fn new(flags: HashMap<String, bool>) -> NimbusFlags {
        NimbusFlags {
            flags: flags
                .into_iter()
                .map(|(k, v)| (NimbusFlag::from_string(&k), v))
                .collect(),
        }
    }

    pub fn check_flag(&self, flag: &NimbusFlag) -> bool {
        *self.flags.get(flag).unwrap_or(&false)
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub enum NimbusFlag {
    // `ads-client.async-enabled`
    AsyncEnabled,

    // This allows unrecognized flags to be passed (parsed as `Unknown`)
    Unknown(String),

    #[cfg(test)]
    Test,
}

impl NimbusFlag {
    pub fn from_string(s: &str) -> NimbusFlag {
        match s {
            "ads-client.async-enabled" => NimbusFlag::AsyncEnabled,
            #[cfg(test)]
            "test" => NimbusFlag::Test,
            s => NimbusFlag::Unknown(s.to_string()),
        }
    }
}
