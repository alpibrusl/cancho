/* A wolfSSL client for scripts/tls_server_interop.py (docs/tls-server.md §8, step 2).
 *
 *     wolfssl_client <port> <server name> <ca.pem> <cipher> <groups> <alpn> <message> [<client chain.pem> <client key.pem>]
 *
 * TLS 1.3 only, to 127.0.0.1. `cipher` is a wolfSSL suite name (TLS13-AES128-GCM-SHA256, TLS13-AES256-GCM-SHA384
 * or TLS13-CHACHA20-POLY1305-SHA256). `groups` is a comma-separated list of X25519, P256, P384 and P521, the
 * first the one key share sent, or `none:` before the list to send no share at all, either way making a server
 * without that group ask with a HelloRetryRequest. `alpn` is a comma-separated list, or `-`. It verifies the
 * chain and the name (and, with the last two arguments, presents the client certificate chain and key a server
 * asks for: docs/tls-server.md §13), sends `message`, half-closes, reads until the server closes, and prints `ok <suite>
 * <curve> <bytes read>`, or `error <code>` and exits 1. Built with `cc wolfssl_client.c -lwolfssl`. */
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

static int group_of(const char *name) {
    if (strcmp(name, "X25519") == 0) return WOLFSSL_ECC_X25519;
    if (strcmp(name, "P256") == 0) return WOLFSSL_ECC_SECP256R1;
    if (strcmp(name, "P384") == 0) return WOLFSSL_ECC_SECP384R1;
    if (strcmp(name, "P521") == 0) return WOLFSSL_ECC_SECP521R1;
    return 0;
}

static int fail(WOLFSSL *ssl, int ret) {
    printf("error %d\n", ssl ? wolfSSL_get_error(ssl, ret) : ret);
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 8) {
        fprintf(stderr, "usage: wolfssl_client <port> <name> <ca> <cipher> <groups> <alpn> <message>\n");
        return 2;
    }
    wolfSSL_Init();
    WOLFSSL_CTX *ctx = wolfSSL_CTX_new(wolfTLSv1_3_client_method());
    if (ctx == NULL || wolfSSL_CTX_load_verify_locations(ctx, argv[3], NULL) != WOLFSSL_SUCCESS)
        return fail(NULL, -1);
    if (wolfSSL_CTX_set_cipher_list(ctx, argv[4]) != WOLFSSL_SUCCESS)
        return fail(NULL, -2);
    if (argc >= 10 && (wolfSSL_CTX_use_certificate_chain_file(ctx, argv[8]) != WOLFSSL_SUCCESS ||
                       wolfSSL_CTX_use_PrivateKey_file(ctx, argv[9], WOLFSSL_FILETYPE_PEM) != WOLFSSL_SUCCESS))
        return fail(NULL, -10);
    WOLFSSL *ssl = wolfSSL_new(ctx);
    if (wolfSSL_UseSNI(ssl, WOLFSSL_SNI_HOST_NAME, argv[2], strlen(argv[2])) != WOLFSSL_SUCCESS ||
        wolfSSL_check_domain_name(ssl, argv[2]) != WOLFSSL_SUCCESS)
        return fail(NULL, -3);
    char list[256];
    snprintf(list, sizeof list, "%s", argv[5]);
    int no_share = strncmp(list, "none:", 5) == 0;
    int groups[8], n = 0;
    for (char *g = strtok(list + (no_share ? 5 : 0), ","); g && n < 8; g = strtok(NULL, ","))
        groups[n++] = group_of(g);
    if (wolfSSL_set_groups(ssl, groups, n) != WOLFSSL_SUCCESS)
        return fail(NULL, -4);
    if (no_share) {
        if (wolfSSL_NoKeyShares(ssl) != WOLFSSL_SUCCESS)
            return fail(NULL, -5);
    } else if (wolfSSL_UseKeyShare(ssl, groups[0]) != WOLFSSL_SUCCESS) {
        return fail(NULL, -5);
    }
    if (strcmp(argv[6], "-") != 0) {
        char protos[256];
        snprintf(protos, sizeof protos, "%s", argv[6]);
        if (wolfSSL_UseALPN(ssl, protos, strlen(protos), WOLFSSL_ALPN_FAILED_ON_MISMATCH) != WOLFSSL_SUCCESS)
            return fail(NULL, -6);
    }
    int s = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = {0};
    a.sin_family = AF_INET;
    a.sin_port = htons(atoi(argv[1]));
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (connect(s, (struct sockaddr *)&a, sizeof a) != 0)
        return fail(NULL, -7);
    wolfSSL_set_fd(ssl, s);
    int ret = wolfSSL_connect(ssl);
    if (ret != WOLFSSL_SUCCESS)
        return fail(ssl, ret);
    size_t want = strlen(argv[7]);
    if (wolfSSL_write(ssl, argv[7], want) != (int)want)
        return fail(ssl, -8);
    char buf[65536];
    size_t got = 0;
    for (;;) {
        int r = wolfSSL_read(ssl, buf + got, sizeof buf - got);
        if (r <= 0)
            break;
        got += r;
        if (got == want)
            break;
    }
    if (got != want || memcmp(buf, argv[7], want) != 0)
        return fail(NULL, -9);
    printf("ok %s %s %zu\n", wolfSSL_get_cipher_name(ssl), wolfSSL_get_curve_name(ssl), got);
    wolfSSL_shutdown(ssl);
    wolfSSL_free(ssl);
    wolfSSL_CTX_free(ctx);
    close(s);
    return 0;
}
