use std::time::{SystemTime, UNIX_EPOCH};

use base64;
use errors::*;
use serde_json;

use fxa_client::browser_id::{SigningPrivateKey, VerifyingPublicKey};

const DEFAULT_ASSERTION_ISSUER: &str = "127.0.0.1";
const DEFAULT_ASSERTION_DURATION: u64 = 60 * 60 * 1000;

pub fn create_assertion(private_key: &SigningPrivateKey, certificate: &str,
                        audience: &str) -> Result<String> {
  let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)
    .expect("Something is very wrong.");
  let issued_at = since_epoch.as_secs() * 1000 +
                  since_epoch.subsec_nanos() as u64 / 1_000_000;
  let expires_at = issued_at + DEFAULT_ASSERTION_DURATION;
  let issuer = DEFAULT_ASSERTION_ISSUER;
  create_assertion_full(private_key, certificate, audience, issuer, issued_at, expires_at)
}

pub fn create_assertion_full(private_key: &SigningPrivateKey, certificate: &str,
                             audience: &str, issuer: &str,
                             issued_at: u64, expires_at: u64) -> Result<String> {
  let assertion = SignedJWTBuilder::new(private_key, issuer, issued_at, expires_at)
    .audience(&audience)
    .build()
    .chain_err(|| "Could not build assertion.")?;
  Ok(format!("{}~{}", &certificate, &assertion))
}

pub fn create_certificate(public_key: &VerifyingPublicKey, email: &str,
                          issuer: &str, issued_at: u64, expires_at: u64,
                          private_key: &SigningPrivateKey) -> Result<String> {
  let public_key_json = public_key.to_json()
    .chain_err(|| "Could not get public key json representation.")?;
  let principal = json!({
    "email": email
  });
  let payload = json!({
    "principal": principal,
    "public-key": public_key_json
  });
  SignedJWTBuilder::new(private_key, issuer, issued_at, expires_at)
    .payload(payload)
    .build()
    .chain_err(|| "Could not build certificate.")
}

struct SignedJWTBuilder<'a> {
  private_key: &'a SigningPrivateKey,
  issuer: &'a str,
  issued_at: u64,
  expires_at: u64,
  audience: Option<&'a str>,
  payload: Option<serde_json::Value>
}

impl<'a> SignedJWTBuilder<'a> {
  fn new(private_key: &'a SigningPrivateKey, issuer: &'a str,
         issued_at: u64, expires_at: u64) -> SignedJWTBuilder<'a> {
    SignedJWTBuilder {
      private_key,
      issuer,
      issued_at,
      expires_at,
      audience: None,
      payload: None
    }
  }

  fn audience(mut self, audience: &'a str) -> SignedJWTBuilder<'a> {
    self.audience = Some(audience);
    self
  }

  fn payload(mut self, payload: serde_json::Value) -> SignedJWTBuilder<'a> {
    self.payload = Some(payload);
    self
  }

  fn build(self) -> Result<String> {
    let payload_string = self.get_payload_string()?;
    encode(&payload_string, self.private_key)
  }

  fn get_payload_string(&self) -> Result<String> {
    let mut payload = match self.payload {
      Some(ref payload) => payload.clone(),
      None => json!({})
    };
    let obj = match payload.as_object_mut() {
      Some(obj) => obj,
      None => bail!("Not an object!")
    };
    if let Some(ref audience) = self.audience {
      obj.insert("aud".to_string(), json!(audience));
    }
    obj.insert("iss".to_string(), json!(self.issuer));
    obj.insert("iat".to_string(), json!(self.issued_at));
    obj.insert("exp".to_string(), json!(self.expires_at));
    Ok(json!(obj).to_string())
  }
}

fn encode(payload: &str, private_key: &SigningPrivateKey) -> Result<String> {
  let headers_str = json!({"alg": private_key.get_algo()}).to_string();
  let encoded_header = base64::encode_config(headers_str.as_bytes(), base64::URL_SAFE_NO_PAD);
  let encoded_payload = base64::encode_config(payload.as_bytes(), base64::URL_SAFE_NO_PAD);
  let message = format!("{}.{}", encoded_header, encoded_payload);
  let signature = private_key.sign(message.as_bytes())?;
  let encoded_signature = base64::encode_config(&signature, base64::URL_SAFE_NO_PAD);
  Ok(format!("{}.{}", message, encoded_signature))
}

fn decode(token: &str, public_key: &VerifyingPublicKey) -> Result<String> {
  let segments: Vec<&str> = token.split(".").collect();
  let message = format!("{}.{}", &segments[0], &segments[1]);
  let message_bytes = message.as_bytes();
  let signature = base64::decode_config(&segments[2], base64::URL_SAFE_NO_PAD)
    .chain_err(|| "Could not decode base64 signature.")?;
  let verified = public_key.verify_message(&message_bytes, &signature)
    .chain_err(|| "Could not verify message.")?;
  if !verified {
    bail!("Could not verify token.")
  }
  let payload = base64::decode_config(&segments[1], base64::URL_SAFE_NO_PAD)
    .chain_err(|| "Could not decode base64 payload.")?;
  String::from_utf8(payload).chain_err(|| "Could not decode UTF-8 payload.")
}

#[cfg(test)]
mod tests {
  use fxa_client::browser_id::rsa;
  use fxa_client::browser_id::{BrowserIDKeyPair};
  use super::*;
  use openssl::bn::BigNum;

  // These tests are copied directly from Firefox for Android's TestJSONWebTokenUtils.
  // They could probably be improved a lot.

  fn do_test_encode_decode(key_pair: &BrowserIDKeyPair) {
    let public_key = key_pair.public_key();
    let private_key = key_pair.private_key();
    let payload = json!({"key": "value"}).to_string();

    let token = encode(&payload, private_key).unwrap();
    let decoded = decode(&token, public_key).unwrap();
    assert_eq!(decoded, payload);

    let token_corrupted = format!("{}x", token);
    assert!(decode(&token_corrupted, public_key).is_err());
  }

  #[test]
  fn test_rsa_encode_decode() {
    do_test_encode_decode(&rsa::generate_keypair(1024).unwrap());
    do_test_encode_decode(&rsa::generate_keypair(2048).unwrap());
  }

  #[test]
  fn test_rsa_generation() {
    let mock_modulus = BigNum::from_dec_str("15498874758090276039465094105837231567265546373975960480941122651107772824121527483107402353899846252489837024870191707394743196399582959425513904762996756672089693541009892030848825079649783086005554442490232900875792851786203948088457942416978976455297428077460890650409549242124655536986141363719589882160081480785048965686285142002320767066674879737238012064156675899512503143225481933864507793118457805792064445502834162315532113963746801770187685650408560424682654937744713813773896962263709692724630650952159596951348264005004375017610441835956073275708740239518011400991972811669493356682993446554779893834303").unwrap();
    let mock_public_exponent = BigNum::from_dec_str("65537").unwrap();
    let mock_private_exponent = BigNum::from_dec_str("6539906961872354450087244036236367269804254381890095841127085551577495913426869112377010004955160417265879626558436936025363204803913318582680951558904318308893730033158178650549970379367915856087364428530828396795995781364659413467784853435450762392157026962694408807947047846891301466649598749901605789115278274397848888140105306063608217776127549926721544215720872305194645129403056801987422794114703255989202755511523434098625000826968430077091984351410839837395828971692109391386427709263149504336916566097901771762648090880994773325283207496645630792248007805177873532441314470502254528486411726581424522838833").unwrap();

    let n = BigNum::from_dec_str("20332459213245328760269530796942625317006933400814022542511832260333163206808672913301254872114045771215470352093046136365629411384688395020388553744886954869033696089099714200452682590914843971683468562019706059388121176435204818734091361033445697933682779095713376909412972373727850278295874361806633955236862180792787906413536305117030045164276955491725646610368132167655556353974515423042221261732084368978523747789654468953860772774078384556028728800902433401131226904244661160767916883680495122225202542023841606998867411022088440946301191503335932960267228470933599974787151449279465703844493353175088719018221").unwrap();
    let e = BigNum::from_dec_str("65537").unwrap();
    let d = BigNum::from_dec_str("9362542596354998418106014928820888151984912891492829581578681873633736656469965533631464203894863562319612803232737938923691416707617473868582415657005943574434271946791143554652502483003923911339605326222297167404896789026986450703532494518628015811567189641735787240372075015553947628033216297520493759267733018808392882741098489889488442349031883643894014316243251108104684754879103107764521172490019661792943030921873284592436328217485953770574054344056638447333651425231219150676837203185544359148474983670261712939626697233692596362322419559401320065488125670905499610998631622562652935873085671353890279911361").unwrap();

    let issuer = "127.0.0.1";
    let audience = "http://localhost:8080";
    let iat: u64 = 1352995809210;
    let dur: u64 = 60 * 60 * 1000;
    let exp: u64 = iat + dur;

    let mock_public_key = rsa::create_public_key(mock_modulus.to_owned().unwrap(), mock_public_exponent.to_owned().unwrap()).unwrap();
    let mock_private_key = rsa::create_private_key(mock_modulus.to_owned().unwrap(), mock_public_exponent.to_owned().unwrap(), mock_private_exponent.to_owned().unwrap()).unwrap();
    let public_key_to_sign = rsa::create_public_key(n.to_owned().unwrap(), e.to_owned().unwrap()).unwrap();
    let private_key_to_sign = rsa::create_private_key(n.to_owned().unwrap(), e.to_owned().unwrap(), d.to_owned().unwrap()).unwrap();

    let certificate = create_certificate(&public_key_to_sign, "test@mockmyid.com", "mockmyid.com", iat, exp, &mock_private_key).unwrap();
    let assertion = create_assertion_full(&private_key_to_sign, &certificate, audience, issuer, iat, exp).unwrap();
    let payload = decode(&certificate, &mock_public_key).unwrap();
    let expected_payload = "{\"exp\":1352999409210,\"iat\":1352995809210,\"iss\":\"mockmyid.com\",\"principal\":{\"email\":\"test@mockmyid.com\"},\"public-key\":{\"algorithm\":\"RS\",\"e\":\"65537\",\"n\":\"20332459213245328760269530796942625317006933400814022542511832260333163206808672913301254872114045771215470352093046136365629411384688395020388553744886954869033696089099714200452682590914843971683468562019706059388121176435204818734091361033445697933682779095713376909412972373727850278295874361806633955236862180792787906413536305117030045164276955491725646610368132167655556353974515423042221261732084368978523747789654468953860772774078384556028728800902433401131226904244661160767916883680495122225202542023841606998867411022088440946301191503335932960267228470933599974787151449279465703844493353175088719018221\"}}";
    assert_eq!(payload, expected_payload);

    let expected_certificate = "eyJhbGciOiJSUzI1NSJ9.eyJleHAiOjEzNTI5OTk0MDkyMTAsImlhdCI6MTM1Mjk5NTgwOTIxMCwiaXNzIjoibW9ja215aWQuY29tIiwicHJpbmNpcGFsIjp7ImVtYWlsIjoidGVzdEBtb2NrbXlpZC5jb20ifSwicHVibGljLWtleSI6eyJhbGdvcml0aG0iOiJSUyIsImUiOiI2NTUzNyIsIm4iOiIyMDMzMjQ1OTIxMzI0NTMyODc2MDI2OTUzMDc5Njk0MjYyNTMxNzAwNjkzMzQwMDgxNDAyMjU0MjUxMTgzMjI2MDMzMzE2MzIwNjgwODY3MjkxMzMwMTI1NDg3MjExNDA0NTc3MTIxNTQ3MDM1MjA5MzA0NjEzNjM2NTYyOTQxMTM4NDY4ODM5NTAyMDM4ODU1Mzc0NDg4Njk1NDg2OTAzMzY5NjA4OTA5OTcxNDIwMDQ1MjY4MjU5MDkxNDg0Mzk3MTY4MzQ2ODU2MjAxOTcwNjA1OTM4ODEyMTE3NjQzNTIwNDgxODczNDA5MTM2MTAzMzQ0NTY5NzkzMzY4Mjc3OTA5NTcxMzM3NjkwOTQxMjk3MjM3MzcyNzg1MDI3ODI5NTg3NDM2MTgwNjYzMzk1NTIzNjg2MjE4MDc5Mjc4NzkwNjQxMzUzNjMwNTExNzAzMDA0NTE2NDI3Njk1NTQ5MTcyNTY0NjYxMDM2ODEzMjE2NzY1NTU1NjM1Mzk3NDUxNTQyMzA0MjIyMTI2MTczMjA4NDM2ODk3ODUyMzc0Nzc4OTY1NDQ2ODk1Mzg2MDc3Mjc3NDA3ODM4NDU1NjAyODcyODgwMDkwMjQzMzQwMTEzMTIyNjkwNDI0NDY2MTE2MDc2NzkxNjg4MzY4MDQ5NTEyMjIyNTIwMjU0MjAyMzg0MTYwNjk5ODg2NzQxMTAyMjA4ODQ0MDk0NjMwMTE5MTUwMzMzNTkzMjk2MDI2NzIyODQ3MDkzMzU5OTk3NDc4NzE1MTQ0OTI3OTQ2NTcwMzg0NDQ5MzM1MzE3NTA4ODcxOTAxODIyMSJ9fQ.a_DXs5LysXoBb6zw3eKVjqIEr8PwXBCqJ0UaLOTNranN18Lw1gAlNDs0wEKvIslvdR3fhWyCm5jRISWTsYlZ8E5XAGwL9LPyFliplxaEVBly-g4mBcZzdDGx37832pwvNHGYnc0qknsjWr0oT8DkZj-ShE3YdVbIlyeGf8191DEJR4aGKccNB2o6itNaa5vrXgMLuZDvXfSDRvE6k2vbQb1wLQQCx_kBwRa6ADmejzVDIqRoKtK7-wCS1zXQzpP3Sa9tOfnKSMHuPkuRTJdrxWHULRkdE0iYmch1YSrGHCtx2kiG09o7YkwH7E53pBSrGcn8mFAdRkNdDrqTdnLV2Q";
    assert_eq!(certificate, expected_certificate);

    let expected_assertion = "eyJhbGciOiJSUzI1NSJ9.eyJleHAiOjEzNTI5OTk0MDkyMTAsImlhdCI6MTM1Mjk5NTgwOTIxMCwiaXNzIjoibW9ja215aWQuY29tIiwicHJpbmNpcGFsIjp7ImVtYWlsIjoidGVzdEBtb2NrbXlpZC5jb20ifSwicHVibGljLWtleSI6eyJhbGdvcml0aG0iOiJSUyIsImUiOiI2NTUzNyIsIm4iOiIyMDMzMjQ1OTIxMzI0NTMyODc2MDI2OTUzMDc5Njk0MjYyNTMxNzAwNjkzMzQwMDgxNDAyMjU0MjUxMTgzMjI2MDMzMzE2MzIwNjgwODY3MjkxMzMwMTI1NDg3MjExNDA0NTc3MTIxNTQ3MDM1MjA5MzA0NjEzNjM2NTYyOTQxMTM4NDY4ODM5NTAyMDM4ODU1Mzc0NDg4Njk1NDg2OTAzMzY5NjA4OTA5OTcxNDIwMDQ1MjY4MjU5MDkxNDg0Mzk3MTY4MzQ2ODU2MjAxOTcwNjA1OTM4ODEyMTE3NjQzNTIwNDgxODczNDA5MTM2MTAzMzQ0NTY5NzkzMzY4Mjc3OTA5NTcxMzM3NjkwOTQxMjk3MjM3MzcyNzg1MDI3ODI5NTg3NDM2MTgwNjYzMzk1NTIzNjg2MjE4MDc5Mjc4NzkwNjQxMzUzNjMwNTExNzAzMDA0NTE2NDI3Njk1NTQ5MTcyNTY0NjYxMDM2ODEzMjE2NzY1NTU1NjM1Mzk3NDUxNTQyMzA0MjIyMTI2MTczMjA4NDM2ODk3ODUyMzc0Nzc4OTY1NDQ2ODk1Mzg2MDc3Mjc3NDA3ODM4NDU1NjAyODcyODgwMDkwMjQzMzQwMTEzMTIyNjkwNDI0NDY2MTE2MDc2NzkxNjg4MzY4MDQ5NTEyMjIyNTIwMjU0MjAyMzg0MTYwNjk5ODg2NzQxMTAyMjA4ODQ0MDk0NjMwMTE5MTUwMzMzNTkzMjk2MDI2NzIyODQ3MDkzMzU5OTk3NDc4NzE1MTQ0OTI3OTQ2NTcwMzg0NDQ5MzM1MzE3NTA4ODcxOTAxODIyMSJ9fQ.a_DXs5LysXoBb6zw3eKVjqIEr8PwXBCqJ0UaLOTNranN18Lw1gAlNDs0wEKvIslvdR3fhWyCm5jRISWTsYlZ8E5XAGwL9LPyFliplxaEVBly-g4mBcZzdDGx37832pwvNHGYnc0qknsjWr0oT8DkZj-ShE3YdVbIlyeGf8191DEJR4aGKccNB2o6itNaa5vrXgMLuZDvXfSDRvE6k2vbQb1wLQQCx_kBwRa6ADmejzVDIqRoKtK7-wCS1zXQzpP3Sa9tOfnKSMHuPkuRTJdrxWHULRkdE0iYmch1YSrGHCtx2kiG09o7YkwH7E53pBSrGcn8mFAdRkNdDrqTdnLV2Q~eyJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJodHRwOi8vbG9jYWxob3N0OjgwODAiLCJleHAiOjEzNTI5OTk0MDkyMTAsImlhdCI6MTM1Mjk5NTgwOTIxMCwiaXNzIjoiMTI3LjAuMC4xIn0.Vi9vl8frqV-devCgV5EEfxyP5omfoWYgehcBMPPBtt-rFgylAUMT48gQb4UQlkRuvdUP7bkfc32KPK6lHCrWNKlsX2O0hnry4lTyFp4g2PGRdCdIGkrQ82hrxWpt-s16x_qW2SkcwcauPYMjOmXkuUnWS5Yx-kjEV07fcy-njl-15NZX8sYFO0uocuRsUXMSp5wibBVbDEEkm9IgRoqBPT9SqnpEwO4RBj0Dx16y4t9eKIvbh_3Jpa3GPUGJWP07t7t2w-622Fmoekcf4Bjfsu-NYtMPj_NE_ZnbZ0VFIv6IdPfPsMHUwwCSy-vFh8ZgvD2EVT1fycT1wTS0Puq-dQ";
    assert_eq!(assertion, expected_assertion);
  }
}
