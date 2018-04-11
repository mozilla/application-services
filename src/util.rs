// From http://illegalargumentexception.blogspot.ca/2015/05/rust-byte-array-to-hex-string.html.
pub fn to_hex_string(bytes: Vec<u8>) -> String {
  let strs: Vec<String> = bytes.iter()
                               .map(|b| format!("{:02X}", b))
                               .collect();
  strs.join("")
}
