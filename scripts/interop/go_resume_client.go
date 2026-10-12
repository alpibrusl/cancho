// A Go crypto/tls client that reconnects with the session it was sent, for scripts/tls_server_tickets_interop.py
// (docs/tls-server.md §12.8).
//
//	go_resume_client <host:port> <server name> <ca.pem> <rounds> <message>
//
// `rounds` connections with one tls.Config and a ClientSessionCache, each sending `message`, reading the echo
// until the server closes. It prints `ok` and, for each round, `full` or `resumed` (ConnectionState.DidResume),
// or `error <what>` and exits 1. TLS 1.3 only.
package main

import (
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"io"
	"os"
	"strconv"
)

func main() {
	pem, err := os.ReadFile(os.Args[3])
	if err != nil {
		panic(err)
	}
	roots := x509.NewCertPool()
	roots.AppendCertsFromPEM(pem)
	config := &tls.Config{ServerName: os.Args[2], RootCAs: roots, MinVersion: tls.VersionTLS13,
		ClientSessionCache: tls.NewLRUClientSessionCache(4)}
	rounds, _ := strconv.Atoi(os.Args[4])
	out := "ok"
	for i := 0; i < rounds; i++ {
		conn, err := tls.Dial("tcp", os.Args[1], config)
		if err != nil {
			fmt.Println("error", err)
			os.Exit(1)
		}
		conn.Write([]byte(os.Args[5]))
		conn.CloseWrite()
		got, err := io.ReadAll(conn)
		if err != nil || string(got) != os.Args[5] {
			fmt.Println("error", err, string(got))
			os.Exit(1)
		}
		if conn.ConnectionState().DidResume {
			out += " resumed"
		} else {
			out += " full"
		}
		conn.Close()
	}
	fmt.Println(out)
}
