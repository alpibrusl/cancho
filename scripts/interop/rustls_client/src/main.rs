//! A rustls client that reconnects with the session it was sent (`docs/tls-server.md` §12.8).
//!
//!     rustls_client <port> <server name> <ca.pem> <rounds>
//!
//! `rounds` connections to 127.0.0.1 with one `ClientConfig` (rustls keeps sessions in memory by default), each
//! sending a short HTTP request and reading until the server's close_notify. Prints `ok` and, for each round,
//! `full` or `resumed` (`handshake_kind`), or `error <what>` and exits 1. TLS 1.3 only.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls::HandshakeKind;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_file_iter(&args[3]).expect("the roots read") {
        roots.add(cert.expect("a certificate")).expect("a root");
    }
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("the versions are supported")
        .with_root_certificates(roots)
        .with_no_client_auth();
    let config = Arc::new(config);
    let rounds: usize = args[4].parse().unwrap();
    let mut out = String::from("ok");
    for _ in 0..rounds {
        let name = ServerName::try_from(args[2].clone()).expect("a name");
        let mut conn = rustls::ClientConnection::new(config.clone(), name).expect("a connection");
        let mut sock = TcpStream::connect(("127.0.0.1", args[1].parse::<u16>().unwrap())).expect("connects");
        let mut tls = rustls::Stream::new(&mut conn, &mut sock);
        if let Err(e) = tls.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n") {
            println!("error {e}");
            std::process::exit(1);
        }
        let mut body = Vec::new();
        let _ = tls.read_to_end(&mut body);
        if !String::from_utf8_lossy(&body).contains("hello from cancho") {
            println!("error body {:?}", String::from_utf8_lossy(&body));
            std::process::exit(1);
        }
        out.push_str(match conn.handshake_kind() {
            Some(HandshakeKind::Resumed) => " resumed",
            _ => " full",
        });
    }
    println!("{out}");
}
