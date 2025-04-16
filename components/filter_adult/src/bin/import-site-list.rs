/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use base64::prelude::*;
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version, about = "A tool for generating the adult_set.rs from the pre-existing FilterAdult.sys.mjs file. The result is sent to stdout.", long_about = None)]
struct Cli {
    /// The path to the FilterAdult.sys.mjs file that contains MD5 hashes of
    /// sites to filter.
    #[arg(required = true)]
    input_file: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let path = cli.input_file.as_deref().unwrap();
    let display = path.display();

    let file_contents: String =
        fs::read_to_string(path).unwrap_or_else(|_| panic!("Unable to read {}", display));
    let hashmap = ingest_filteradult_mjs(&file_contents);
    let adult_set = generate_adult_set(hashmap);
    println!("{}", adult_set);
}

fn ingest_filteradult_mjs(filteradult_mjs_contents: &str) -> HashMap<String, Vec<u8>> {
    let mut result = HashMap::new();
    let re = Regex::new(r#"\s\s"(.+==)","#).unwrap();

    for (_, [string_hash]) in re
        .captures_iter(filteradult_mjs_contents)
        .map(|c| c.extract())
    {
        let byte_hash = BASE64_STANDARD.decode(string_hash).unwrap();
        result.insert(string_hash.to_string(), byte_hash);
    }

    result
}

fn generate_adult_set(hashmap: HashMap<String, Vec<u8>>) -> String {
    let mut output = Vec::<String>::new();
    let license_preamble = r#"/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

"#;
    output.push(license_preamble.to_string());
    output.push(format!(
        "pub static ADULT_SET: [[u8;16]; {}] = [\n",
        hashmap.len()
    ));
    for (string_hash, bytes) in &hashmap {
        output.push(format!("    {:?}, // {}\n", bytes, string_hash))
    }
    output.push(String::from("];\n"));
    output.join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_filteradult_mjs_simple() {
        let filteradult_contents = r#"
   * For tests, adds a domain to the adult list.
   */
  addDomainToList(url) {
    gAdultSet.add(
      md5Hash(Services.eTLD.getBaseDomain(Services.io.newURI(url)))
    );
  },

  /**
   * For tests, removes a domain to the adult list.
   */
  removeDomainFromList(url) {
    gAdultSet.delete(
      md5Hash(Services.eTLD.getBaseDomain(Services.io.newURI(url)))
    );
  },
};

// These are md5 hashes of base domains to be filtered out. Originally from:
// https://hg.mozilla.org/mozilla-central/log/default/browser/base/content/newtab/newTab.inadjacent.json
gAdultSet = new Set([
  "+P5q4YD1Rr5SX26Xr+tzlw==",
  "+PUVXkoTqHxJHO18z4KMfw==",
  "+Pl0bSMBAdXpRIA+zE02JA==",
  "+QosBAnSM2h4lsKuBlqEZw==",
  "+S+WXgVDSU1oGmCzGwuT3g==",
  "+SclwwY8R2RPrnX54Z+A6w==",
]);
"#;
        let expected_hashes = vec![
            "+P5q4YD1Rr5SX26Xr+tzlw==",
            "+PUVXkoTqHxJHO18z4KMfw==",
            "+Pl0bSMBAdXpRIA+zE02JA==",
            "+QosBAnSM2h4lsKuBlqEZw==",
            "+S+WXgVDSU1oGmCzGwuT3g==",
            "+SclwwY8R2RPrnX54Z+A6w==",
        ];
        let hashmap = ingest_filteradult_mjs(filteradult_contents);

        // Ensure we got the expected number of hashes out.
        assert_eq!(hashmap.len(), 6);

        for hash in expected_hashes {
            // Compute the byte representation of the hash, and ensure it
            // is what the string hash has been mapped to.
            let hash_bytes = BASE64_STANDARD.decode(hash).unwrap();
            // Apparently string comparison is how Rust byte vectors can be
            // compared in tests?
            assert_eq!(hashmap.get(hash).unwrap(), &*hash_bytes.to_vec());
        }
    }

    #[test]
    fn test_ingest_filteradult_mjs_empty() {
        let filteradult_contents = r#"
   * For tests, adds a domain to the adult list.
   */
  addDomainToList(url) {
    gAdultSet.add(
      md5Hash(Services.eTLD.getBaseDomain(Services.io.newURI(url)))
    );
  }
]);
"#;
        let hashmap = ingest_filteradult_mjs(filteradult_contents);

        // We should not have found any hashes.
        assert_eq!(hashmap.len(), 0);
    }
}
