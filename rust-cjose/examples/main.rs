extern crate jose_c;

fn main() {
    let cjose_version = jose_c::version();

    println!("cjose version is {}", cjose_version);


//    let kty = "oct";
//    let k = "lPcH0XzKdjZe5nZ2aRZvpg2PX6tQjd7T-DeLPllm8Fc";
//    let kid = "1521857656-2wUwquQBAEluma_ZtAeKkA";
//
//    let input = "eyJhbGciOiJBMjU2S1ciLCJlbmMiOiJBMjU2R0NNIn0.D0pXfIy0inmvfcpe_QsNkm31Y9rsEUhGmY2p2o67mM4xQAIBP1JWgg.2xdHtQuMWHzREbkb.8nGVFour5PyKttLvzjQvRoDVtlAz226vNcc1PWa8c3hGDbK0ZCD-PnzJ4lSphmCkPrYb_JZxXtNzz4oIegC8WMuEgYDeMSnnFeOBiKt3_pgLcf6E8EQ-C07420UGzZrMYmeBnf1Nfz_90nbLGZKZmyaNkIU3KcFgz9SYjU8duuvZkOg-YuDuLbtZL0iqqXn-w1z-PTX3uprr64OVIPPnOHRW.pYisL1l_6ffPtSgVfbOCfQ";

    //let x = unsafe { snappy_max_compressed_length(100) };
    // let keystore = jose_c::JWK::asKey(100);



    // let decryptor = jose_c::JWE::createDecrypt(keystore);
    // let result = decryptor.decrypt(input);
    //let result = jose_c::JWE::createDecrypt(keystore, input);

    //println!("Decrypted: {}", result);
}
