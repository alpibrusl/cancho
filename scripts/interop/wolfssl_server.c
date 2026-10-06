/* A wolfSSL server for scripts/tls_interop.py (docs/tls-assurance.md §5).
 *
 *     wolfssl_server <port> <cert.pem> <key.pem> <1.2|1.3> [<cipher list>]
 *
 * Prints "ready" once listening, then serves connections one at a time:
 * reads the request to its blank line, answers a fixed HTTP/1.0 response
 * and closes with close_notify. Built with
 * `cc wolfssl_server.c -lwolfssl`. */
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

static const char reply[] = "HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, cancho";

int main(int argc, char **argv) {
    if (argc < 5) {
        fprintf(stderr, "usage: wolfssl_server <port> <cert> <key> <1.2|1.3> [<ciphers>]\n");
        return 2;
    }
    wolfSSL_Init();
    WOLFSSL_CTX *ctx = wolfSSL_CTX_new(strcmp(argv[4], "1.2") == 0 ? wolfTLSv1_2_server_method()
                                                                   : wolfTLSv1_3_server_method());
    if (ctx == NULL || wolfSSL_CTX_use_certificate_chain_file(ctx, argv[2]) != WOLFSSL_SUCCESS ||
        wolfSSL_CTX_use_PrivateKey_file(ctx, argv[3], WOLFSSL_FILETYPE_PEM) != WOLFSSL_SUCCESS) {
        fprintf(stderr, "cannot load the certificate or key\n");
        return 1;
    }
    if (argc > 5 && wolfSSL_CTX_set_cipher_list(ctx, argv[5]) != WOLFSSL_SUCCESS) {
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
        WOLFSSL *ssl = wolfSSL_new(ctx);
        wolfSSL_set_fd(ssl, s);
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
                wolfSSL_write(ssl, reply, sizeof reply - 1);
                wolfSSL_shutdown(ssl);
            }
        }
        wolfSSL_free(ssl);
        close(s);
    }
}
