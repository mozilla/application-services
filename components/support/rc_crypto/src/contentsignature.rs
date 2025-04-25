/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::str;

use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE},
    Engine,
};

use crate::error::*;
use crate::signature;

/// Verify content signatures, with the ECDSA P384 curve and SHA-384 hashing (NIST384p / secp384r1).
///
/// These signatures are typically used to guarantee integrity of data between our servers and clients.
/// This is a critical part of systems like Remote Settings or the experiment platform.
///
/// The equivalent implementation for Gecko is ``security/manager/ssl/nsIContentSignatureVerifier.idl``.
///
/// Decode a string with colon separated hexadecimal pairs into an array of bytes
/// (eg. "3C:01:44" -> [60, 1, 68]).
fn decode_root_hash(input: &str) -> Result<Vec<u8>> {
    let bytes_hex = input.split(':');

    let mut result: Vec<u8> = vec![];
    for byte_hex in bytes_hex {
        let byte = match hex::decode(byte_hex) {
            Ok(v) => v,
            Err(_) => return Err(ErrorKind::RootHashFormatError(input.to_string()).into()),
        };
        result.extend(byte);
    }

    Ok(result)
}

/// Split a certificate chain in PEM format into a list of certificates bytes,
/// decoded from base64.
fn split_pem(pem_content: &[u8]) -> Result<Vec<Vec<u8>>> {
    let pem_str = match str::from_utf8(pem_content) {
        Ok(v) => v,
        Err(e) => {
            return Err(ErrorKind::PEMFormatError(e.to_string()).into());
        }
    };

    let pem_lines = pem_str.split('\n');

    let mut blocks: Vec<Vec<u8>> = vec![];
    let mut block: Vec<u8> = vec![];
    let mut read = false;
    for line in pem_lines {
        if line.contains("-----BEGIN CERTIFICATE") {
            read = true;
        } else if line.contains("-----END CERTIFICATE") {
            read = false;
            let decoded = match STANDARD.decode(&block) {
                Ok(v) => v,
                Err(e) => return Err(ErrorKind::PEMFormatError(e.to_string()).into()),
            };
            blocks.push(decoded);
            block.clear();
        } else if read {
            block.extend_from_slice(line.as_bytes());
        }
    }
    if read {
        return Err(ErrorKind::PEMFormatError("Missing end header".into()).into());
    }
    if blocks.is_empty() {
        return Err(ErrorKind::PEMFormatError("Missing PEM data".into()).into());
    }

    Ok(blocks)
}

/// Verify that the signature matches the input data.
///
/// The data must be prefixed with ``Content-Signature:\u{0}``.
/// The signature must be provided as base 64 url-safe encoded.
/// The certificate chain, provided as PEM, must be valid at the provided current time.
/// The root certificate content must match the provided root hash, and the leaf
/// subject name must match the provided hostname.
pub fn verify(
    input: &[u8],
    signature: &[u8],
    pem_bytes: &[u8],
    seconds_since_epoch: u64,
    root_sha256_hash: &str,
    hostname: &str,
) -> Result<()> {
    let certificates = split_pem(pem_bytes)?;

    let mut certificates_slices: Vec<&[u8]> = vec![];
    for certificate in &certificates {
        certificates_slices.push(certificate);
    }

    let root_hash_bytes = decode_root_hash(root_sha256_hash)?;

    nss::pkixc::verify_code_signing_certificate_chain(
        certificates_slices,
        seconds_since_epoch,
        &root_hash_bytes,
        hostname,
    )
    .map_err(|err| match err.kind() {
        nss::ErrorKind::CertificateIssuerError => ErrorKind::CertificateIssuerError,
        nss::ErrorKind::CertificateValidityError => ErrorKind::CertificateValidityError,
        nss::ErrorKind::CertificateSubjectError => ErrorKind::CertificateSubjectError,
        _ => ErrorKind::CertificateChainError(err.to_string()),
    })?;

    let leaf_cert = certificates.first().unwrap(); // PEM parse fails if len == 0.

    let public_key_bytes = match nss::cert::extract_ec_public_key(leaf_cert) {
        Ok(bytes) => bytes,
        Err(err) => return Err(ErrorKind::CertificateContentError(err.to_string()).into()),
    };

    let signature_bytes = match URL_SAFE.decode(signature) {
        Ok(b) => b,
        Err(err) => return Err(ErrorKind::SignatureContentError(err.to_string()).into()),
    };

    // Since signature is NIST384p / secp384r1, we can perform a few safety checks.
    if signature_bytes.len() != 96 {
        return Err(ErrorKind::SignatureContentError(format!(
            "signature contains {} bytes instead of {}",
            signature_bytes.len(),
            96
        ))
        .into());
    }
    if public_key_bytes.len() != 96 + 1 {
        // coordinates with x04 prefix.
        return Err(ErrorKind::CertificateContentError(format!(
            "public key contains {} bytes instead of {}",
            public_key_bytes.len(),
            97
        ))
        .into());
    }

    let signature_alg = &signature::ECDSA_P384_SHA384;
    let public_key = signature::UnparsedPublicKey::new(signature_alg, &public_key_bytes);
    // Note that if the provided key type or curve is incorrect here, the signature will
    // be considered as invalid.
    match public_key.verify(input, &signature_bytes) {
        Ok(_) => Ok(()),
        Err(err) => Err(ErrorKind::SignatureMismatchError(err.to_string()).into()),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const ROOT_HASH: &str = "3C:01:44:6A:BE:90:36:CE:A9:A0:9A:CA:A3:A5:20:AC:62:8F:20:A7:AE:32:CE:86:1C:B2:EF:B7:0F:A0:C7:45";
    const VALID_CERT_CHAIN: &[u8] = b"\
-----BEGIN CERTIFICATE-----
MIIDBjCCAougAwIBAgIIFml6g0ldRGowCgYIKoZIzj0EAwMwgaMxCzAJBgNVBAYT
AlVTMRwwGgYDVQQKExNNb3ppbGxhIENvcnBvcmF0aW9uMS8wLQYDVQQLEyZNb3pp
bGxhIEFNTyBQcm9kdWN0aW9uIFNpZ25pbmcgU2VydmljZTFFMEMGA1UEAww8Q29u
dGVudCBTaWduaW5nIEludGVybWVkaWF0ZS9lbWFpbEFkZHJlc3M9Zm94c2VjQG1v
emlsbGEuY29tMB4XDTIxMDIwMzE1MDQwNVoXDTIxMDQyNDE1MDQwNVowgakxCzAJ
BgNVBAYTAlVTMRMwEQYDVQQIEwpDYWxpZm9ybmlhMRYwFAYDVQQHEw1Nb3VudGFp
biBWaWV3MRwwGgYDVQQKExNNb3ppbGxhIENvcnBvcmF0aW9uMRcwFQYDVQQLEw5D
bG91ZCBTZXJ2aWNlczE2MDQGA1UEAxMtcmVtb3RlLXNldHRpbmdzLmNvbnRlbnQt
c2lnbmF0dXJlLm1vemlsbGEub3JnMHYwEAYHKoZIzj0CAQYFK4EEACIDYgAE8pKb
HX4IiD0SCy+NO7gwKqRRZ8IhGd8PTaIHIBgM6RDLRyDeswXgV+2kGUoHyzkbNKZt
zlrS3AhqeUCtl1g6ECqSmZBbRTjCpn/UCpCnMLL0T0goxtAB8Rmi3CdM0cBUo4GD
MIGAMA4GA1UdDwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEFBQcDAzAfBgNVHSME
GDAWgBQlZawrqt0eUz/t6OdN45oKfmzy6DA4BgNVHREEMTAvgi1yZW1vdGUtc2V0
dGluZ3MuY29udGVudC1zaWduYXR1cmUubW96aWxsYS5vcmcwCgYIKoZIzj0EAwMD
aQAwZgIxAPh43Bxl4MxPT6Ra1XvboN5O2OvIn2r8rHvZPWR/jJ9vcTwH9X3F0aLJ
9FiresnsLAIxAOoAcREYB24gFBeWxbiiXaG7TR/yM1/MXw4qxbN965FFUaoB+5Bc
fS8//SQGTlCqKQ==
-----END CERTIFICATE-----
-----BEGIN CERTIFICATE-----
MIIF2jCCA8KgAwIBAgIEAQAAADANBgkqhkiG9w0BAQsFADCBqTELMAkGA1UEBhMC
VVMxCzAJBgNVBAgTAkNBMRYwFAYDVQQHEw1Nb3VudGFpbiBWaWV3MRwwGgYDVQQK
ExNBZGRvbnMgVGVzdCBTaWduaW5nMSQwIgYDVQQDExt0ZXN0LmFkZG9ucy5zaWdu
aW5nLnJvb3QuY2ExMTAvBgkqhkiG9w0BCQEWInNlY29wcytzdGFnZXJvb3RhZGRv
bnNAbW96aWxsYS5jb20wHhcNMjEwMTExMDAwMDAwWhcNMjQxMTE0MjA0ODU5WjCB
ozELMAkGA1UEBhMCVVMxHDAaBgNVBAoTE01vemlsbGEgQ29ycG9yYXRpb24xLzAt
BgNVBAsTJk1vemlsbGEgQU1PIFByb2R1Y3Rpb24gU2lnbmluZyBTZXJ2aWNlMUUw
QwYDVQQDDDxDb250ZW50IFNpZ25pbmcgSW50ZXJtZWRpYXRlL2VtYWlsQWRkcmVz
cz1mb3hzZWNAbW96aWxsYS5jb20wdjAQBgcqhkjOPQIBBgUrgQQAIgNiAARw1dyE
xV5aNiHJPa/fVHO6kxJn3oZLVotJ0DzFZA9r1sQf8i0+v78Pg0/c3nTAyZWfkULz
vOpKYK/GEGBtisxCkDJ+F3NuLPpSIg3fX25pH0LE15fvASBVcr8tKLVHeOmjggG6
MIIBtjAMBgNVHRMEBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAWBgNVHSUBAf8EDDAK
BggrBgEFBQcDAzAdBgNVHQ4EFgQUJWWsK6rdHlM/7ejnTeOaCn5s8ugwgdkGA1Ud
IwSB0TCBzoAUhtg0HE5Y0RNcmV/YQpjtFA8Z8l2hga+kgawwgakxCzAJBgNVBAYT
AlVTMQswCQYDVQQIEwJDQTEWMBQGA1UEBxMNTW91bnRhaW4gVmlldzEcMBoGA1UE
ChMTQWRkb25zIFRlc3QgU2lnbmluZzEkMCIGA1UEAxMbdGVzdC5hZGRvbnMuc2ln
bmluZy5yb290LmNhMTEwLwYJKoZIhvcNAQkBFiJzZWNvcHMrc3RhZ2Vyb290YWRk
b25zQG1vemlsbGEuY29tggRgJZg7MDMGCWCGSAGG+EIBBAQmFiRodHRwOi8vYWRk
b25zLmFsbGl6b20ub3JnL2NhL2NybC5wZW0wTgYDVR0eBEcwRaBDMCCCHi5jb250
ZW50LXNpZ25hdHVyZS5tb3ppbGxhLm9yZzAfgh1jb250ZW50LXNpZ25hdHVyZS5t
b3ppbGxhLm9yZzANBgkqhkiG9w0BAQsFAAOCAgEAtGTTzcPzpcdf07kIeRs9vPMx
qiF8ylW5L/IQ2NzT3sFFAvPW1vW1wZC0xAHMsuVyo+BTGrv+4mlD0AUR9acRfiTZ
9qyZ3sJbyhQwJAXLKU4YpnzuFOf58T/yOnOdwpH2ky/0FuHskMyfXaAz2Az4JXJH
TCgggqfdZNvsZ5eOnQlKoC5NadMa8oTI5sd4SyR5ANUPAtYok931MvVSz3IMbwTr
v4PPWXdl9SGXuOknSqdY6/bS1LGvC2KprsT+PBlvVtS6YgZOH0uCgTTLpnrco87O
ErzC2PJBA1Ftn3Mbaou6xy7O+YX+reJ6soNUV+0JHOuKj0aTXv0c+lXEAh4Y8nea
UGhW6+MRGYMOP2NuKv8s2+CtNH7asPq3KuTQpM5RerjdouHMIedX7wpNlNk0CYbg
VMJLxZfAdwcingLWda/H3j7PxMoAm0N+eA24TGDQPC652ZakYk4MQL/45lm0A5f0
xLGKEe6JMZcTBQyO7ANWcrpVjKMiwot6bY6S2xU17mf/h7J32JXZJ23OPOKpMS8d
mljj4nkdoYDT35zFuS1z+5q6R5flLca35vRHzC3XA0H/XJvgOKUNLEW/IiJIqLNi
ab3Ao0RubuX+CAdFML5HaJmkyuJvL3YtwIOwe93RGcGRZSKZsnMS+uY5QN8+qKQz
LC4GzWQGSCGDyD+JCVw=
-----END CERTIFICATE-----
-----BEGIN CERTIFICATE-----
MIIHbDCCBVSgAwIBAgIEYCWYOzANBgkqhkiG9w0BAQwFADCBqTELMAkGA1UEBhMC
VVMxCzAJBgNVBAgTAkNBMRYwFAYDVQQHEw1Nb3VudGFpbiBWaWV3MRwwGgYDVQQK
ExNBZGRvbnMgVGVzdCBTaWduaW5nMSQwIgYDVQQDExt0ZXN0LmFkZG9ucy5zaWdu
aW5nLnJvb3QuY2ExMTAvBgkqhkiG9w0BCQEWInNlY29wcytzdGFnZXJvb3RhZGRv
bnNAbW96aWxsYS5jb20wHhcNMjEwMjExMjA0ODU5WhcNMjQxMTE0MjA0ODU5WjCB
qTELMAkGA1UEBhMCVVMxCzAJBgNVBAgTAkNBMRYwFAYDVQQHEw1Nb3VudGFpbiBW
aWV3MRwwGgYDVQQKExNBZGRvbnMgVGVzdCBTaWduaW5nMSQwIgYDVQQDExt0ZXN0
LmFkZG9ucy5zaWduaW5nLnJvb3QuY2ExMTAvBgkqhkiG9w0BCQEWInNlY29wcytz
dGFnZXJvb3RhZGRvbnNAbW96aWxsYS5jb20wggIiMA0GCSqGSIb3DQEBAQUAA4IC
DwAwggIKAoICAQDKRVty/FRsO4Ech6EYleyaKgAueaLYfMSsAIyPC/N8n/P8QcH8
rjoiMJrKHRlqiJmMBSmjUZVzZAP0XJku0orLKWPKq7cATt+xhGY/RJtOzenMMsr5
eN02V3GzUd1jOShUpERjzXdaO3pnfZqhdqNYqP9ocqQpyno7bZ3FZQ2vei+bF52k
51uPioTZo+1zduoR/rT01twGtZm3QpcwU4mO74ysyxxgqEy3kpojq8Nt6haDwzrj
khV9M6DGPLHZD71QaUiz5lOhD9CS8x0uqXhBhwMUBBkHsUDSxbN4ZhjDDWpCmwaD
OtbJMUJxDGPCr9qj49QESccb367OeXLrfZ2Ntu/US2Bw9EDfhyNsXr9dg9NHj5yf
4sDUqBHG0W8zaUvJx5T2Ivwtno1YZLyJwQW5pWeWn8bEmpQKD2KS/3y2UjlDg+YM
NdNASjFe0fh6I5NCFYmFWA73DpDGlUx0BtQQU/eZQJ+oLOTLzp8d3dvenTBVnKF+
uwEmoNfZwc4TTWJOhLgwxA4uK+Paaqo4Ap2RGS2ZmVkPxmroB3gL5n3k3QEXvULh
7v8Psk4+MuNWnxudrPkN38MGJo7ju7gDOO8h1jLD4tdfuAqbtQLduLXzT4DJPA4y
JBTFIRMIpMqP9CovaS8VPtMFLTrYlFh9UnEGpCeLPanJr+VEj7ae5sc8YwIDAQAB
o4IBmDCCAZQwDAYDVR0TBAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwFgYDVR0lAQH/
BAwwCgYIKwYBBQUHAwMwLAYJYIZIAYb4QgENBB8WHU9wZW5TU0wgR2VuZXJhdGVk
IENlcnRpZmljYXRlMDMGCWCGSAGG+EIBBAQmFiRodHRwOi8vYWRkb25zLm1vemls
bGEub3JnL2NhL2NybC5wZW0wHQYDVR0OBBYEFIbYNBxOWNETXJlf2EKY7RQPGfJd
MIHZBgNVHSMEgdEwgc6AFIbYNBxOWNETXJlf2EKY7RQPGfJdoYGvpIGsMIGpMQsw
CQYDVQQGEwJVUzELMAkGA1UECBMCQ0ExFjAUBgNVBAcTDU1vdW50YWluIFZpZXcx
HDAaBgNVBAoTE0FkZG9ucyBUZXN0IFNpZ25pbmcxJDAiBgNVBAMTG3Rlc3QuYWRk
b25zLnNpZ25pbmcucm9vdC5jYTExMC8GCSqGSIb3DQEJARYic2Vjb3BzK3N0YWdl
cm9vdGFkZG9uc0Btb3ppbGxhLmNvbYIEYCWYOzANBgkqhkiG9w0BAQwFAAOCAgEA
nowyJv8UaIV7NA0B3wkWratq6FgA1s/PzetG/ZKZDIW5YtfUvvyy72HDAwgKbtap
Eog6zGI4L86K0UGUAC32fBjE5lWYEgsxNM5VWlQjbgTG0dc3dYiufxfDFeMbAPmD
DzpIgN3jHW2uRqa/MJ+egHhv7kGFL68uVLboqk/qHr+SOCc1LNeSMCuQqvHwwM0+
AU1GxhzBWDkealTS34FpVxF4sT5sKLODdIS5HXJr2COHHfYkw2SW/Sfpt6fsOwaF
2iiDaK4LPWHWhhIYa6yaynJ+6O6KPlpvKYCChaTOVdc+ikyeiSO6AakJykr5Gy7d
PkkK7MDCxuY6psHj7iJQ59YK7ujQB8QYdzuXBuLLo5hc5gBcq3PJs0fLT2YFcQHA
dj+olGaDn38T0WI8ycWaFhQfKwATeLWfiQepr8JfoNlC2vvSDzGUGfdAfZfsJJZ8
5xZxahHoTFGS0mDRfXqzKH5uD578GgjOZp0fULmzkcjWsgzdpDhadGjExRZFKlAy
iKv8cXTONrGY0fyBDKennuX0uAca3V0Qm6v2VRp+7wG/pywWwc5n+04qgxTQPxgO
6pPB9UUsNbaLMDR5QPYAWrNhqJ7B07XqIYJZSwGP5xB9NqUZLF4z+AOMYgWtDpmg
IKdcFKAt3fFrpyMhlfIKkLfmm0iDjmfmIXbDGBJw9SE=
-----END CERTIFICATE-----";
    const VALID_INPUT: &[u8] =
        b"Content-Signature:\x00{\"data\":[],\"last_modified\":\"1603992731957\"}";
    const VALID_SIGNATURE: &[u8] = b"fJJcOpwdnkjEWFeHXfdOJN6GaGLuDTPGzQOxA2jn6ldIleIk6KqMhZcy2GZv2uYiGwl6DERWwpaoUfQFLyCAOcVjck1qlaaEFZGY1BQba9p99xEc9FNQ3YPPfvSSZqsw";
    const VALID_HOSTNAME: &str = "remote-settings.content-signature.mozilla.org";

    const INVALID_CERTIFICATE: &[u8] = b"\
    -----BEGIN CERTIFICATE-----
    invalidCertificategIFiJLFfdxFlYwCgYIKoZIzj0EAwMwgaMxCzAJBgNVBAYT
    AlVTMRwwGgYDVQQKExNNb3ppbGxhIENvcnBvcmF0aW9uMS8wLQYDVQQLEyZNb3pp
    bGxhIEFNTyBQcm9kdWN0aW9uIFNpZ25pbmcgU2VydmljZTFFMEMGA1UEAww8Q29u
    dGVudCBTaWduaW5nIEludGVybWVkaWF0ZS9lbWFpbEFkZHJlc3M9Zm94c2VjQG1v
    emlsbGEuY29tMB4XDTIwMDYxNjE3MTYxNVoXDTIwMDkwNDE3MTYxNVowgakxCzAJ
    BgNVBAYTAlVTMRMwEQYDVQQIEwpDYWxpZm9ybmlhMRYwFAYDVQQHEw1Nb3VudGFp
    biBWaWV3MRwwGgYDVQQKExNNb3ppbGxhIENvcnBvcmF0aW9uMRcwFQYDVQQLEw5D
    bG91ZCBTZXJ2aWNlczE2MDQGA1UEAxMtcmVtb3RlLXNldHRpbmdzLmNvbnRlbnQt
    c2lnbmF0dXJlLm1vemlsbGEub3JnMHYwEAYHKoZIzj0CAQYFK4EEACIDYgAEDmOX
    N5IGlUqCvu6xkOKr020Eo3kY2uPdJO0ZihVUoglk1ktQPss184OajFOMKm/BJX4W
    IsZUzQoRL8NgGfZDwBjT95Q87lhOWEWs5AU/nMXIYwDp7rpUPaUqw0QLMikdo4GD
    MIGAMA4GA1UdDwEB/wQEAwIHgDATBgNVHSUEDDAKBggrBgEFBQcDAzAfBgNVHSME
    GDAWgBSgHUoXT4zCKzVF8WPx2nBwp8744TA4BgNVHREEMTAvgi1yZW1vdGUtc2V0
    dGluZ3MuY29udGVudC1zaWduYXR1cmUubW96aWxsYS5vcmcwCgYIKoZIzj0EAwMD
    aQAwZgIxAJvyynyPqRmRMqf95FPH5xfcoT3jb/2LOkUifGDtjtZ338ScpT2glUK8
    HszKVANqXQIxAIygMaeTiD9figEusmHMthBdFoIoHk31x4MHukAy+TWZ863X6/V2
    6/ZrZMpinvalid==
    -----END CERTIFICATE-----";

    #[test]
    fn test_decode_root_hash() {
        nss::ensure_initialized();
        assert!(decode_root_hash("meh!").is_err());
        assert!(decode_root_hash("3C:rr:44").is_err());

        let result = decode_root_hash(ROOT_HASH).unwrap();
        assert_eq!(
            result,
            vec![
                60, 1, 68, 106, 190, 144, 54, 206, 169, 160, 154, 202, 163, 165, 32, 172, 98, 143,
                32, 167, 174, 50, 206, 134, 28, 178, 239, 183, 15, 160, 199, 69
            ]
        );
    }

    #[test]
    fn test_split_pem() {
        assert!(split_pem(b"meh!").is_err());

        assert!(split_pem(
            b"-----BEGIN CERTIFICATE-----
invalidCertificate
-----END CERTIFICATE-----"
        )
        .is_err());

        assert!(split_pem(
            b"-----BEGIN CERTIFICATE-----
bGxhIEFNTyBQcm9kdWN0aW9uIFNp
-----BEGIN CERTIFICATE-----"
        )
        .is_err());

        let result = split_pem(
            b"-----BEGIN CERTIFICATE-----
AQID
BAUG
-----END CERTIFICATE-----
-----BEGIN CERTIFICATE-----
/f7/
-----END CERTIFICATE-----",
        )
        .unwrap();
        assert_eq!(result, vec![vec![1, 2, 3, 4, 5, 6], vec![253, 254, 255]]);
    }

    #[test]
    fn test_verify_fails_if_invalid() {
        nss::ensure_initialized();
        assert!(verify(
            b"msg",
            b"sig",
            b"-----BEGIN CERTIFICATE-----
fdfeff
-----END CERTIFICATE-----",
            42,
            ROOT_HASH,
            "remotesettings.firefox.com",
        )
        .is_err());
    }

    #[test]
    fn test_verify_fails_if_cert_has_expired() {
        nss::ensure_initialized();
        assert!(verify(
            VALID_INPUT,
            VALID_SIGNATURE,
            VALID_CERT_CHAIN,
            1215559719, // July 9, 2008
            ROOT_HASH,
            VALID_HOSTNAME,
        )
        .is_err());
    }

    #[test]
    fn test_verify_fails_if_bad_certificate_chain() {
        nss::ensure_initialized();
        assert!(verify(
            VALID_INPUT,
            VALID_SIGNATURE,
            INVALID_CERTIFICATE,
            1615559719, // March 12, 2021
            ROOT_HASH,
            VALID_HOSTNAME,
        )
        .is_err());
    }

    #[test]
    fn test_verify_fails_if_mismatch() {
        nss::ensure_initialized();
        assert!(verify(
            b"msg",
            VALID_SIGNATURE,
            VALID_CERT_CHAIN,
            1615559719, // March 12, 2021
            ROOT_HASH,
            VALID_HOSTNAME,
        )
        .is_err());
    }

    #[test]
    fn test_verify_fails_if_bad_hostname() {
        nss::ensure_initialized();
        assert!(verify(
            VALID_INPUT,
            VALID_SIGNATURE,
            VALID_CERT_CHAIN,
            1615559719, // March 12, 2021
            ROOT_HASH,
            "some.hostname.org",
        )
        .is_err());
    }

    #[test]
    fn test_verify_fails_if_bad_root_hash() {
        nss::ensure_initialized();
        assert!(verify(
            VALID_INPUT,
            VALID_SIGNATURE,
            VALID_CERT_CHAIN,
            1615559719, // March 12, 2021
            "00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00:00",
            VALID_HOSTNAME,
        )
        .is_err());
    }

    #[test]
    fn test_verify_succeeds_if_valid() {
        nss::ensure_initialized();
        verify(
            VALID_INPUT,
            VALID_SIGNATURE,
            VALID_CERT_CHAIN,
            1615559719, // March 12, 2021
            ROOT_HASH,
            VALID_HOSTNAME,
        )
        .unwrap();
    }
}
