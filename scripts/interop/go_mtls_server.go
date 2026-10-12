// A Go crypto/tls server that asks for client certificates and negotiates ALPN, for scripts/tls_auth_interop.py
// (docs/tls-parity.md §6.10).
//
//	go run go_mtls_server.go <port> <cert.pem> <key.pem> <1.2|1.3> <ca.pem> <none|optional|require> [<alpn,list>]
//
// It prints "ready" once listening, then answers each connection's request (read to the blank line) with an HTTP/1.0
// response whose body says what the server saw: "client=<common name or none> alpn=<protocol or none>". require is
// ClientAuth RequireAndVerifyClientCert, optional VerifyClientCertIfGiven.
package main

import (
	"bufio"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"net"
	"os"
	"strings"
)

func main() {
	cert, err := tls.LoadX509KeyPair(os.Args[2], os.Args[3])
	if err != nil {
		panic(err)
	}
	config := &tls.Config{Certificates: []tls.Certificate{cert}}
	if os.Args[4] == "1.2" {
		config.MinVersion, config.MaxVersion = tls.VersionTLS12, tls.VersionTLS12
	} else {
		config.MinVersion = tls.VersionTLS13
	}
	pem, err := os.ReadFile(os.Args[5])
	if err != nil {
		panic(err)
	}
	pool := x509.NewCertPool()
	pool.AppendCertsFromPEM(pem)
	config.ClientCAs = pool
	switch os.Args[6] {
	case "require":
		config.ClientAuth = tls.RequireAndVerifyClientCert
	case "optional":
		config.ClientAuth = tls.VerifyClientCertIfGiven
	}
	if len(os.Args) > 7 && os.Args[7] != "" {
		config.NextProtos = strings.Split(os.Args[7], ",")
	}
	ln, err := tls.Listen("tcp", "127.0.0.1:"+os.Args[1], config)
	if err != nil {
		panic(err)
	}
	fmt.Println("ready")
	for {
		conn, err := ln.Accept()
		if err != nil {
			continue
		}
		go func(c net.Conn) {
			defer c.Close()
			tc := c.(*tls.Conn)
			if err := tc.Handshake(); err != nil {
				return
			}
			r := bufio.NewReader(c)
			for {
				line, err := r.ReadString('\n')
				if err != nil {
					return
				}
				if strings.TrimRight(line, "\r\n") == "" {
					break
				}
			}
			state := tc.ConnectionState()
			who, proto := "none", "none"
			if len(state.PeerCertificates) > 0 {
				who = state.PeerCertificates[0].Subject.CommonName
			}
			if state.NegotiatedProtocol != "" {
				proto = state.NegotiatedProtocol
			}
			body := "client=" + who + " alpn=" + proto
			c.Write([]byte(fmt.Sprintf("HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n%s", len(body), body)))
		}(conn)
	}
}
