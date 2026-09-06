//! `sscl-probe <host> [port]` — prints what the Session Info sidebar shows.
//!
//! A small companion binary. Handy for checking a server from a script, and
//! used by the integration test.

use sscl::sshinfo;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(host) = args.next() else {
        eprintln!("usage: sscl-probe <host> [port]");
        std::process::exit(2);
    };
    let port: u16 = args.next().and_then(|p| p.parse().ok()).unwrap_or(22);

    let info = sshinfo::probe(&host, port);
    println!("host: {} port: {}", info.host, info.port);
    println!("ip: {}", info.ip.clone().unwrap_or_else(|| "-".into()));
    println!("banner: {}", info.banner);
    println!("protocol: {}", info.protocol);
    println!("software: {}", info.software);
    println!("comment: {}", info.comment);
    for (name, list) in [
        ("kex", &info.kex),
        ("host_key", &info.host_key_algorithms),
        ("ciphers_s2c", &info.ciphers_s2c),
        ("ciphers_c2s", &info.ciphers_c2s),
        ("macs_s2c", &info.macs_s2c),
        ("macs_c2s", &info.macs_c2s),
        ("compression_s2c", &info.compression_s2c),
        ("compression_c2s", &info.compression_c2s),
    ] {
        println!("{name}[{}]: {}", list.len(), list.join(", "));
    }
    for key in &info.host_keys {
        println!(
            "host key: {} {} {}",
            key.algorithm,
            key.bits.map(|b| format!("{b} bit")).unwrap_or_default(),
            key.fingerprint
        );
    }
    for clue in &info.os_clues {
        println!("os clue: {clue}");
    }
    for q in &info.quirks {
        println!("quirk: {q}");
    }
    for n in &info.notes {
        println!("note: {n}");
    }
    if let Some(e) = &info.error {
        println!("error: {e}");
        std::process::exit(1);
    }
}
