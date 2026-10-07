/* An mbedTLS 2.28 server for scripts/tls_interop.py (docs/tls-assurance.md
 * §5). mbedTLS 2.28 has no TLS 1.3 server, so this is TLS 1.2 only.
 *
 *     mbedtls_server <port> <cert.pem> <key.pem> [<ciphersuite name>]
 *
 * Prints "ready" once listening, then serves connections one at a time:
 * reads the request to its blank line, answers a fixed HTTP/1.0 response
 * and closes with close_notify. Built with
 * `cc mbedtls_server.c -lmbedtls -lmbedx509 -lmbedcrypto`. */
#include <mbedtls/ctr_drbg.h>
#include <mbedtls/entropy.h>
#include <mbedtls/net_sockets.h>
#include <mbedtls/ssl.h>
#include <mbedtls/ssl_ciphersuites.h>
#include <stdio.h>
#include <string.h>

static const char reply[] = "HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, cancho";

int main(int argc, char **argv) {
    if (argc < 4) {
        fprintf(stderr, "usage: mbedtls_server <port> <cert> <key> [<suite>]\n");
        return 2;
    }
    mbedtls_entropy_context entropy;
    mbedtls_ctr_drbg_context drbg;
    mbedtls_x509_crt cert;
    mbedtls_pk_context key;
    mbedtls_ssl_config conf;
    mbedtls_net_context listen, client;
    mbedtls_entropy_init(&entropy);
    mbedtls_ctr_drbg_init(&drbg);
    mbedtls_x509_crt_init(&cert);
    mbedtls_pk_init(&key);
    mbedtls_ssl_config_init(&conf);
    mbedtls_net_init(&listen);
    static int suites[2];
    if (mbedtls_ctr_drbg_seed(&drbg, mbedtls_entropy_func, &entropy, NULL, 0) != 0 ||
        mbedtls_x509_crt_parse_file(&cert, argv[2]) != 0 || mbedtls_pk_parse_keyfile(&key, argv[3], NULL) != 0 ||
        mbedtls_ssl_config_defaults(&conf, MBEDTLS_SSL_IS_SERVER, MBEDTLS_SSL_TRANSPORT_STREAM,
                                    MBEDTLS_SSL_PRESET_DEFAULT) != 0 ||
        mbedtls_ssl_conf_own_cert(&conf, &cert, &key) != 0) {
        fprintf(stderr, "cannot set up\n");
        return 1;
    }
    mbedtls_ssl_conf_rng(&conf, mbedtls_ctr_drbg_random, &drbg);
    mbedtls_ssl_conf_min_version(&conf, MBEDTLS_SSL_MAJOR_VERSION_3, MBEDTLS_SSL_MINOR_VERSION_3);
    if (argc > 4) {
        suites[0] = mbedtls_ssl_get_ciphersuite_id(argv[4]);
        if (suites[0] == 0) {
            fprintf(stderr, "no suite %s\n", argv[4]);
            return 1;
        }
        mbedtls_ssl_conf_ciphersuites(&conf, suites);
    }
    if (mbedtls_net_bind(&listen, "127.0.0.1", argv[1], MBEDTLS_NET_PROTO_TCP) != 0) {
        fprintf(stderr, "cannot bind\n");
        return 1;
    }
    printf("ready\n");
    fflush(stdout);
    for (;;) {
        mbedtls_ssl_context ssl;
        mbedtls_ssl_init(&ssl);
        mbedtls_net_init(&client);
        if (mbedtls_net_accept(&listen, &client, NULL, 0, NULL) != 0 || mbedtls_ssl_setup(&ssl, &conf) != 0) {
            mbedtls_ssl_free(&ssl);
            continue;
        }
        mbedtls_ssl_set_bio(&ssl, &client, mbedtls_net_send, mbedtls_net_recv, NULL);
        char buf[20000];
        int got = 0;
        int done = 0;
        if (mbedtls_ssl_handshake(&ssl) == 0) {
            while (!done && got < (int)sizeof buf - 1) {
                int n = mbedtls_ssl_read(&ssl, (unsigned char *)buf + got, sizeof buf - 1 - got);
                if (n <= 0)
                    break;
                got += n;
                buf[got] = 0;
                done = strstr(buf, "\r\n\r\n") != NULL;
            }
            if (done) {
                mbedtls_ssl_write(&ssl, (const unsigned char *)reply, sizeof reply - 1);
                mbedtls_ssl_close_notify(&ssl);
            }
        }
        mbedtls_ssl_free(&ssl);
        mbedtls_net_free(&client);
    }
}
