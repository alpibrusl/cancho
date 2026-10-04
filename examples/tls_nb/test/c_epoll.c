/* The C reference for `tls_nb`'s handshake numbers, written the way a C programmer would for many connections on one thread:
 * non-blocking sockets, `SSL_set_fd`, one epoll set, up to <conc> handshakes in flight, each ends with close_notify and close.
 * The same work as `tls_nb <ip> <port> <host> <total> <conc> 0 <verify> <cafile> 0 0 1 0 0 0 1` (reqs 0, I/O mode fd).
 *
 *   cc -O2 -o c_epoll c_epoll.c -lssl -lcrypto
 *   c_epoll <ip> <port> <host> <total> <conc> <verify 0|1> <cafile|->
 */
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <openssl/err.h>
#include <openssl/ssl.h>
#include <openssl/x509_vfy.h>
#include <openssl/x509v3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/epoll.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <unistd.h>

static double cpu(void) {
    struct rusage r;
    getrusage(RUSAGE_SELF, &r);
    return r.ru_utime.tv_sec + r.ru_stime.tv_sec + (r.ru_utime.tv_usec + r.ru_stime.tv_usec) / 1e6;
}

struct slot { int fd; SSL *ssl; int live; int connecting; };

int main(int argc, char **argv) {
    if (argc < 8) { fprintf(stderr, "usage\n"); return 2; }
    const char *ip = argv[1]; int port = atoi(argv[2]); const char *host = argv[3]; int total = atoi(argv[4]);
    int conc = atoi(argv[5]); int verify = atoi(argv[6]); const char *ca = argv[7];
    SSL_CTX *ctx = SSL_CTX_new(TLS_client_method());
    SSL_CTX_set_min_proto_version(ctx, TLS1_2_VERSION);
    SSL_CTX_set_mode(ctx, SSL_MODE_ENABLE_PARTIAL_WRITE | SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER);
    if (verify) {
        SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
        if (strcmp(ca, "-") && SSL_CTX_load_verify_file(ctx, ca) != 1) { fprintf(stderr, "ca\n"); return 3; }
    }
    struct sockaddr_in a; memset(&a, 0, sizeof a); a.sin_family = AF_INET; a.sin_port = htons(port); inet_pton(AF_INET, ip, &a.sin_addr);
    int ep = epoll_create1(0);
    struct slot *slots = calloc(conc, sizeof *slots);
    int started = 0, ended = 0, ok = 0, live = 0;
    double t0 = cpu();
    struct epoll_event evs[64];
    while (ended < total) {
        while (started < total && live < conc) {
            int s = 0; while (slots[s].live) s++;
            int fd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0);
            int r = connect(fd, (struct sockaddr *)&a, sizeof a);
            if (r != 0 && errno != EINPROGRESS) { close(fd); started++; ended++; continue; }
            slots[s].fd = fd; slots[s].live = 1; slots[s].connecting = 1; slots[s].ssl = NULL; live++; started++;
            struct epoll_event e = { .events = EPOLLOUT, .data.u32 = s };
            epoll_ctl(ep, EPOLL_CTL_ADD, fd, &e);
        }
        int n = epoll_wait(ep, evs, 64, 200);
        for (int i = 0; i < n; i++) {
            struct slot *sl = &slots[evs[i].data.u32];
            if (!sl->live) continue;
            int done = 0, good = 0;
            if (sl->connecting) {
                int err = 0; socklen_t l = sizeof err; getsockopt(sl->fd, SOL_SOCKET, SO_ERROR, &err, &l);
                if (err) { done = 1; }
                else {
                    sl->connecting = 0;
                    sl->ssl = SSL_new(ctx); SSL_set_fd(sl->ssl, sl->fd); SSL_set_tlsext_host_name(sl->ssl, host);
                    if (verify) {
                        X509_VERIFY_PARAM *p = SSL_get0_param(sl->ssl);
                        X509_VERIFY_PARAM_set_hostflags(p, X509_CHECK_FLAG_NO_PARTIAL_WILDCARDS);
                        X509_VERIFY_PARAM_set1_host(p, host, 0);
                    }
                    SSL_set_connect_state(sl->ssl);
                }
            }
            if (!done) {
                ERR_clear_error();
                int r = SSL_do_handshake(sl->ssl);
                if (r == 1) { done = 1; good = 1; }
                else {
                    int e = SSL_get_error(sl->ssl, r);
                    struct epoll_event ev = { .data.u32 = evs[i].data.u32 };
                    if (e == SSL_ERROR_WANT_READ) { ev.events = EPOLLIN; epoll_ctl(ep, EPOLL_CTL_MOD, sl->fd, &ev); }
                    else if (e == SSL_ERROR_WANT_WRITE) { ev.events = EPOLLOUT; epoll_ctl(ep, EPOLL_CTL_MOD, sl->fd, &ev); }
                    else done = 1;
                }
            }
            if (done) {
                if (good) { SSL_shutdown(sl->ssl); ok++; }
                if (sl->ssl) SSL_free(sl->ssl);
                close(sl->fd); sl->live = 0; live--; ended++;
            }
        }
    }
    double t1 = cpu();
    printf("c_epoll total=%d ok=%d cpu_ms_per_conn=%.3f\n", total, ok, (t1 - t0) / total * 1000);
    return 0;
}
