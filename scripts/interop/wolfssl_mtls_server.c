/* A wolfSSL server that asks for client certificates and negotiates ALPN, for scripts/tls_auth_interop.py
 * (docs/tls-parity.md §6.10).
 *
 *     wolfssl_mtls_server <port> <cert.pem> <key.pem> <1.2|1.3> <ca.pem> <none|optional|require> [<alpn,list>]
 *
 * Prints "ready" once listening, then serves connections one at a time: reads the request to its blank line and
 * answers an HTTP/1.0 response whose body says what the server saw: "client=<common name or none> alpn=<protocol
 * or none>". Built with `cc wolfssl_mtls_server.c -lwolfssl`. */
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

int main(int argc, char **argv) {
    if (argc < 7) {
        fprintf(stderr, "usage: wolfssl_mtls_server <port> <cert> <key> <1.2|1.3> <ca> <none|optional|require> [<alpn>]\n");
        return 2;
    }
    wolfSSL_Init();
    WOLFSSL_CTX *ctx = wolfSSL_CTX_new(strcmp(argv[4], "1.2") == 0 ? wolfTLSv1_2_server_method()
                                                                   : wolfTLSv1_3_server_method());
    if (ctx == NULL || wolfSSL_CTX_use_certificate_chain_file(ctx, argv[2]) != WOLFSSL_SUCCESS ||
        wolfSSL_CTX_use_PrivateKey_file(ctx, argv[3], WOLFSSL_FILETYPE_PEM) != WOLFSSL_SUCCESS ||
        wolfSSL_CTX_load_verify_locations(ctx, argv[5], NULL) != WOLFSSL_SUCCESS) {
        fprintf(stderr, "cannot load the certificate, key or CA\n");
        return 1;
    }
    if (strcmp(argv[6], "require") == 0)
        wolfSSL_CTX_set_verify(ctx, WOLFSSL_VERIFY_PEER | WOLFSSL_VERIFY_FAIL_IF_NO_PEER_CERT, NULL);
    else if (strcmp(argv[6], "optional") == 0)
        wolfSSL_CTX_set_verify(ctx, WOLFSSL_VERIFY_PEER, NULL);
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
        WOLFSSL *ssl = wolfSSL_new(ctx);
        wolfSSL_set_fd(ssl, s);
        if (argc > 7 && argv[7][0] != 0) {
            char list[256];
            strncpy(list, argv[7], sizeof list - 1);
            list[sizeof list - 1] = 0;
            wolfSSL_UseALPN(ssl, list, strlen(list), WOLFSSL_ALPN_CONTINUE_ON_MISMATCH);
        }
        char buf[20000];
        int got = 0;
        int done = 0;
        if (wolfSSL_accept(ssl) == WOLFSSL_SUCCESS) {
            while (!done && got < (int)sizeof buf - 1) {
                int n = wolfSSL_read(ssl, buf + got, sizeof buf - 1 - got);
                if (n <= 0)
                    break;
                got += n;
                buf[got] = 0;
                done = strstr(buf, "\r\n\r\n") != NULL;
            }
            if (done) {
                char who[256] = "none", proto[256] = "none", body[600], reply[800];
                WOLFSSL_X509 *peer = wolfSSL_get_peer_certificate(ssl);
                if (peer != NULL) {
                    char *name = wolfSSL_X509_NAME_oneline(wolfSSL_X509_get_subject_name(peer), NULL, 0);
                    char *cn = name ? strstr(name, "/CN=") : NULL;
                    if (cn) {
                        strncpy(who, cn + 4, sizeof who - 1);
                        who[sizeof who - 1] = 0;
                        char *end = strchr(who, '/');
                        if (end) *end = 0;
                    }
                    if (name) XFREE(name, NULL, DYNAMIC_TYPE_OPENSSL);
                    wolfSSL_X509_free(peer);
                }
                char *p = NULL;
                unsigned short pl = 0;
                if (wolfSSL_ALPN_GetProtocol(ssl, &p, &pl) == WOLFSSL_SUCCESS && p && pl < sizeof proto) {
                    memcpy(proto, p, pl);
                    proto[pl] = 0;
                }
                int bl = snprintf(body, sizeof body, "client=%s alpn=%s", who, proto);
                int rl = snprintf(reply, sizeof reply, "HTTP/1.0 200 OK\r\nContent-Length: %d\r\n\r\n%s", bl, body);
                wolfSSL_write(ssl, reply, rl);
                wolfSSL_shutdown(ssl);
            }
        }
        wolfSSL_free(ssl);
        close(s);
    }
}
