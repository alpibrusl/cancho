/* A wolfSSL client that reconnects with the session it was sent, for scripts/tls_server_tickets_interop.py
 * (docs/tls-server.md §12.8).
 *
 *     wolfssl_resume_client <port> <server name> <ca.pem> <rounds> <message>
 *
 * TLS 1.3 only, to 127.0.0.1. `rounds` connections, each a new WOLFSSL given the session the last one ended
 * with (wolfSSL_get1_session / wolfSSL_set_session), each sending `message` and reading the echo. Prints `ok` and
 * for each round `full` or `resumed` (wolfSSL_session_reused), or `error <code>` and exits 1.
 * Built with `cc wolfssl_resume_client.c -lwolfssl`. */
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

static int fail(int code) {
    printf("error %d\n", code);
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 6) {
        fprintf(stderr, "usage: wolfssl_resume_client <port> <name> <ca> <rounds> <message>\n");
        return 2;
    }
    wolfSSL_Init();
    WOLFSSL_CTX *ctx = wolfSSL_CTX_new(wolfTLSv1_3_client_method());
    if (ctx == NULL || wolfSSL_CTX_load_verify_locations(ctx, argv[3], NULL) != WOLFSSL_SUCCESS)
        return fail(-1);
    WOLFSSL_SESSION *session = NULL;
    int rounds = atoi(argv[4]);
    printf("ok");
    for (int i = 0; i < rounds; i++) {
        WOLFSSL *ssl = wolfSSL_new(ctx);
        if (wolfSSL_UseSNI(ssl, WOLFSSL_SNI_HOST_NAME, argv[2], strlen(argv[2])) != WOLFSSL_SUCCESS ||
            wolfSSL_check_domain_name(ssl, argv[2]) != WOLFSSL_SUCCESS)
            return fail(-3);
        if (session != NULL && wolfSSL_set_session(ssl, session) != WOLFSSL_SUCCESS)
            return fail(-4);
        int s = socket(AF_INET, SOCK_STREAM, 0);
        struct sockaddr_in a = {0};
        a.sin_family = AF_INET;
        a.sin_port = htons(atoi(argv[1]));
        a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        if (connect(s, (struct sockaddr *)&a, sizeof a) != 0)
            return fail(-7);
        wolfSSL_set_fd(ssl, s);
        int ret = wolfSSL_connect(ssl);
        if (ret != WOLFSSL_SUCCESS)
            return fail(wolfSSL_get_error(ssl, ret));
        size_t want = strlen(argv[5]);
        if (wolfSSL_write(ssl, argv[5], want) != (int)want)
            return fail(-8);
        char buf[65536];
        size_t got = 0;
        while (got < want) {
            int r = wolfSSL_read(ssl, buf + got, sizeof buf - got);
            if (r <= 0)
                break;
            got += r;
        }
        if (got != want || memcmp(buf, argv[5], want) != 0)
            return fail(-9);
        printf(" %s", wolfSSL_session_reused(ssl) ? "resumed" : "full");
        if (session != NULL)
            wolfSSL_SESSION_free(session);
        session = wolfSSL_get1_session(ssl);
        wolfSSL_shutdown(ssl);
        wolfSSL_free(ssl);
        close(s);
    }
    printf("\n");
    wolfSSL_CTX_free(ctx);
    return 0;
}
