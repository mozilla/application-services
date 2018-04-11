use hkdf::Hkdf;
use sha2::Sha256;

pub fn derive_hkdf_sha256_key(ikm: &[u8], xts: &[u8], info: &[u8], len: usize) -> Vec<u8> {
  let hk = Hkdf::<Sha256>::extract(&ikm, &xts);
  hk.expand(&info, len)
}
