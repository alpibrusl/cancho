/* The C reference for the handshake-cost numbers: the same work as `tls_nb <ip> <port> <host> <total> 1 0 <verify> <cafile>`
 * (connect, TLS 1.2+ handshake with the same context settings, close_notify, close) in the simplest blocking C an OpenSSL
 * example would write, one connection at a time. Prints CPU time (user+sys, from getrusage) per connection.
 *
 *   cc -O2 -o c_ref c_ref.c -lssl -lcrypto
 *   c_ref <ip> <port> <host> <total> <verify 0|1> <cafile|-> [fd|bio]
 */
#include <arpa/inet.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <openssl/x509_vfy.h>
#include <openssl/x509v3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <unistd.h>

static double cpu(void) {
    struct rusage r;
    getrusage(RUSAGE_SELF, &r);
    return r.ru_utime.tv_sec + r.ru_stime.tv_sec + (r.ru_utime.tv_usec + r.ru_stime.tv_usec) / 1e6;
}

int main(int argc, char **argv) {
    if (argc < 7) { fprintf(stderr, "usage\n"); return 2; }
    const char *ip = argv[1]; int port = atoi(argv[2]); const char *host = argv[3]; int total = atoi(argv[4]);
    int verify = atoi(argv[5]); const char *ca = argv[6];
    SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
    SSL_CTX_set_mode(ctx, SSL_MODE_ENABLE_PARTIAL_WRITE | SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER);
    if (verify) {
        SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
        if (strcmp(ca, "-") && SSL_CTX_load_verify_file(ctx, ca) != 1) { fprintf(stderr, "ca\n"); return 3; }
    }
    struct sockaddr_in a; memset(&a, 0, sizeof a); a.sin_family = AF_INET; a.sin_port = htons(port); inet_pton(AF_INET, ip, &a.sin_addr);
    int ok = 0;
    double t0 = cpu();
    for (int i = 0; i < total; i++) {
        int fd = socket(AF_INET, SOCK_STREAM, 0);
        if (connect(fd, (struct sockaddr *)&a, sizeof a) != 0) { close(fd); continue; }
        SSL *s = SSL_new(ctx);
        SSL_set_fd(s, fd);
        SSL_set_tlsext_host_name(s, host);
        if (verify) {
            X509_VERIFY_PARAM *p = SSL_get0_param(s);
            X509_VERIFY_PARAM_set_hostflags(p, X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS);
            X509_VERIFY_PARAM_set1_host(p, host, 0);
        }
        if (SSL_connect(s) == 1) ok++;
        SSL_shutdown(s);
        SSL_free(s);
        close(fd);
    }
    double t1 = cpu();
    printf("c_ref total=%d ok=%d cpu_ms_per_conn=%.3f\n", total, ok, (t1 - t0) / total * 1000);
    return 0;
}
