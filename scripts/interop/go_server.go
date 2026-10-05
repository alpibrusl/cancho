// A Go crypto/tls server for scripts/tls_interop.py (docs/tls-assurance.md §5).
//
//	go run go_server.go <port> <cert.pem> <key.pem> <1.2|1.3> [<cipher suite name>]
//
// It prints "ready" once listening, then answers each connection's request
// (read to the blank line) with a fixed HTTP/1.0 response, and closes with
// close_notify. With a suite name and TLS 1.2, that suite only (Go does not
// let TLS 1.3's suites be chosen).
package main

import (
	"bufio"
	"crypto/tls"
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
	if len(os.Args) > 5 {
		for _, s := range tls.CipherSuites() {
			if s.Name == os.Args[5] {
				config.CipherSuites = []uint16{s.ID}
			}
		}
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
			c.Write([]byte("HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, lexsys"))
		}(conn)
	}
}
