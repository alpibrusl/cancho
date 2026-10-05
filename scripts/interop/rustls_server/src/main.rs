//! A rustls server for `scripts/tls_interop.py` (`docs/tls-assurance.md` §5).
//!
//!     rustls_server <port> <cert.pem> <key.pem> <1.2|1.3> [<suite>]
//!
//! It prints `ready` once listening, then answers each connection's request
//! (read to the blank line) with a fixed HTTP/1.0 response and closes with
//! close_notify, one thread a connection. `<suite>` is a rustls suite name,
//! such as `TLS13_AES_128_GCM_SHA256`, to offer that suite only.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let certs: Vec<CertificateDer> = CertificateDer::pem_file_iter(&args[2])
        .expect("the certificate file reads")
        .map(|c| c.expect("a certificate"))
        .collect();
    let key = PrivateKeyDer::from_pem_file(&args[3]).expect("the key reads");
    let mut provider = rustls::crypto::ring::default_provider();
    if let Some(name) = args.get(5) {
        provider.cipher_suites.retain(|s| format!("{:?}", s.suite()) == *name);
        assert!(!provider.cipher_suites.is_empty(), "no suite {name}");
    }
    let versions: &[&rustls::SupportedProtocolVersion] =
        if args[4] == "1.2" { &[&rustls::version::TLS12] } else { &[&rustls::version::TLS13] };
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(versions)
        .expect("the versions are supported")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("the key matches");
    let config = Arc::new(config);
    let listener = TcpListener::bind(("127.0.0.1", args[1].parse::<u16>().unwrap())).unwrap();
    println!("ready");
    std::io::stdout().flush().unwrap();
    for stream in listener.incoming() {
        let Ok(mut sock) = stream else { continue };
        let config = config.clone();
        std::thread::spawn(move || {
            let mut conn = rustls::ServerConnection::new(config).unwrap();
            let mut tls = rustls::Stream::new(&mut conn, &mut sock);
            let mut reader = BufReader::new(&mut tls);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) if line.trim_end().is_empty() => break,
                    Ok(_) => {}
                }
            }
            drop(reader);
            let _ = tls.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, lexsys");
            conn.send_close_notify();
            let _ = conn.complete_io(&mut sock);
        });
    }
}
