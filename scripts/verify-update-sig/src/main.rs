use std::{env, fs, process};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: verify-update-sig <exe-path> <sig-b64> <pubkey-b64>");
        process::exit(2);
    }

    let data = fs::read(&args[1]).expect("read exe");
    let sig_b64 = &args[2];
    let pubkey_b64 = &args[3];

    let pubkey_str = String::from_utf8(STANDARD.decode(pubkey_b64).expect("decode pubkey b64"))
        .expect("pubkey utf8");
    let sig_str = String::from_utf8(STANDARD.decode(sig_b64).expect("decode sig b64"))
        .expect("sig utf8");

    let public_key = PublicKey::decode(&pubkey_str).expect("parse pubkey");
    let signature = Signature::decode(&sig_str).expect("parse signature");

    match public_key.verify(&data, &signature, true) {
        Ok(()) => {
            println!("OK: signature valid for {}", args[1]);
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            process::exit(1);
        }
    }
}
