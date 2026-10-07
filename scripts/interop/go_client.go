// A Go crypto/tls client for scripts/tls_server_interop.py (docs/tls-server.md §8, step 2).
//
//	go_client <host:port> <server name> <ca.pem> <curves> <alpn> <message>
//
// `curves` is a comma-separated list of X25519, P256, P384 and P521, in order of preference: Go sends a key share
// for the first only, so a first curve the server does not have makes it ask with a HelloRetryRequest. `alpn` is
// a comma-separated list, or `-`. TLS 1.3 only; Go does not let a client choose among TLS 1.3's suites. It sends
// `message`, reads until the server closes, and prints `ok <version> <suite> <alpn> <bytes read>`, or `error
// <what>` and exits 1.
package main

import (
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"io"
	"os"
	"strings"
)

func main() {
	pem, err := os.ReadFile(os.Args[3])
	if err != nil {
		panic(err)
	}
	roots := x509.NewCertPool()
	roots.AppendCertsFromPEM(pem)
	config := &tls.Config{ServerName: os.Args[2], RootCAs: roots, MinVersion: tls.VersionTLS13}
	names := map[string]tls.CurveID{"X25519": tls.X25519, "P256": tls.CurveP256, "P384": tls.CurveP384, "P521": tls.CurveP521}
	for _, c := range strings.Split(os.Args[4], ",") {
		config.CurvePreferences = append(config.CurvePreferences, names[c])
	}
	if os.Args[5] != "-" {
		config.NextProtos = strings.Split(os.Args[5], ",")
	}
	conn, err := tls.Dial("tcp", os.Args[1], config)
	if err != nil {
		fmt.Println("error", err)
		os.Exit(1)
	}
	if _, err := conn.Write([]byte(os.Args[6])); err != nil {
		fmt.Println("error", err)
		os.Exit(1)
	}
	conn.CloseWrite()
	got, err := io.ReadAll(conn)
	if err != nil {
		fmt.Println("error", err)
		os.Exit(1)
	}
	if string(got) != os.Args[6] {
		fmt.Printf("error echoed %q\n", got)
		os.Exit(1)
	}
	st := conn.ConnectionState()
	proto := st.NegotiatedProtocol
	if proto == "" {
		proto = "-"
	}
	fmt.Printf("ok %x %s %s %d\n", st.Version, tls.CipherSuiteName(st.CipherSuite), proto, len(got))
}
