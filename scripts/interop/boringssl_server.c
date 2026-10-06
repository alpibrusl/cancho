/* A BoringSSL server for scripts/tls_interop.py (docs/tls-assurance.md §5),
 * against Android's build of BoringSSL (android-libboringssl-dev). Its own
 * `bssl-tool server` cannot be used: it binds only IPv6.
 *
 *     boringssl_server <port> <cert.pem> <key.pem> <1.2|1.3> [<cipher list>]
 *
 * Prints "ready" once listening, then serves connections one at a time:
 * reads the request to its blank line, answers a fixed HTTP/1.0 response
 * and closes with close_notify. The cipher list applies to TLS 1.2 only:
 * BoringSSL does not let TLS 1.3's suites be chosen. Built with
 * `cc -I/usr/include/android boringssl_server.c
 *  -L/usr/lib/x86_64-linux-gnu/android -Wl,-rpath,/usr/lib/x86_64-linux-gnu/android -lssl -lcrypto`. */
#include <arpa/inet.h>
#include <openssl/ssl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char reply[] = "HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, cancho";

int main(int argc, char **argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: boringssl_server <port> <cert> <key> <1.2|1.3> [<ciphers>]\n");
        return 2;
    }
    SSL_CTX *ctx = SSL_CTX_new(TLS_server_method());
    uint16_t v = strcmp(argv[4], "1.2") == 0 ? TLS1_2_VERSION : TLS1_3_VERSION;
    if (ctx == NULL || !SSL_CTX_set_min_proto_version(ctx, v) || !SSL_CTX_set_max_proto_version(ctx, v) ||
        SSL_CTX_use_certificate_chain_file(ctx, argv[2]) != 1 ||
        SSL_CTX_use_PrivateKey_file(ctx, argv[3], SSL_FILETYPE_PEM) != 1) {
        fprintf(stderr, "cannot load the certificate or key\n");
        return 1;
    }
    if (argc > 5 && !SSL_CTX_set_strict_cipher_list(ctx, argv[5])) {
        fprintf(stderr, "cipher list refused: %s\n", argv[5]);
        return 1;
    }
    int ls = socket(AF_INET, SOCK_STREAM, 0);
    int on = 1;
    setsockopt(ls, SOL_SOCKET, SO_REUSEADDR, &on, sizeof on);
    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_port = htons(atoi(argv[1]));
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(ls, (struct sockaddr *)&a, sizeof a) != 0 || listen(ls, 256) != 0) {
        perror("bind");
        return 1;
    }
    printf("ready\n");
    fflush(stdout);
    for (;;) {
        int s = accept(ls, NULL, NULL);
        if (s < 0)
            continue;
        SSL *ssl = SSL_new(ctx);
        SSL_set_fd(ssl, s);
        char buf[20000];
        int got = 0;
        int done = 0;
        if (SSL_accept(ssl) == 1) {
            while (!done && got < (int)sizeof buf - 1) {
                int n = SSL_read(ssl, buf + got, sizeof buf - 1 - got);
                if (n <= 0)
                    break;
                got += n;
                buf[got] = 0;
                done = strstr(buf, "\r\n\r\n") != NULL;
            }
            if (done) {
                SSL_write(ssl, reply, sizeof reply - 1);
                SSL_shutdown(ssl);
            }
        }
        SSL_free(ssl);
        close(s);
    }
}
