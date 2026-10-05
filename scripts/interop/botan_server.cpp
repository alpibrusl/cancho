// A Botan 2 server for scripts/tls_interop.py (docs/tls-assurance.md §5).
// Botan 2 has no TLS 1.3, so this is TLS 1.2 only, and its own `botan
// tls_server` has no mode that answers and closes.
//
//     botan_server <port> <cert.pem> <key.pem> [<cipher>]
//
// `<cipher>` is a Botan cipher name, such as `ChaCha20Poly1305`, to offer
// that one only. Prints "ready" once listening, then serves connections one
// at a time: reads the request to its blank line, answers a fixed HTTP/1.0
// response and closes with close_notify. Built with
// `c++ -std=c++17 -I/usr/include/botan-2 botan_server.cpp -lbotan-2`.
#include <arpa/inet.h>
#include <botan/auto_rng.h>
#include <botan/credentials_manager.h>
#include <botan/pkcs8.h>
#include <botan/tls_server.h>
#include <botan/tls_session_manager.h>
#include <botan/x509cert.h>
#include <unistd.h>

#include <cstdio>
#include <cstring>
#include <string>

static const std::string reply = "HTTP/1.0 200 OK\r\nContent-Length: 13\r\n\r\nhello, lexsys";

class Credentials : public Botan::Credentials_Manager {
  public:
    Credentials(const std::string &cert, const std::string &key, Botan::RandomNumberGenerator &rng)
        : chain{Botan::X509_Certificate(cert)}, priv(Botan::PKCS8::load_key(key, rng)) {}
    std::vector<Botan::X509_Certificate> cert_chain(const std::vector<std::string> &algos, const std::string &,
                                                    const std::string &) override {
        for (const auto &a : algos)
            if (a == priv->algo_name())
                return chain;
        return {};
    }
    Botan::Private_Key *private_key_for(const Botan::X509_Certificate &, const std::string &,
                                        const std::string &) override {
        return priv.get();
    }

  private:
    std::vector<Botan::X509_Certificate> chain;
    std::unique_ptr<Botan::Private_Key> priv;
};

class Policy : public Botan::TLS::Policy {
  public:
    explicit Policy(std::string c) : cipher(std::move(c)) {}
    std::vector<std::string> allowed_ciphers() const override {
        if (!cipher.empty())
            return {cipher};
        return {"ChaCha20Poly1305", "AES-256/GCM", "AES-128/GCM"};
    }
    std::vector<std::string> allowed_key_exchange_methods() const override { return {"ECDH"}; }
    bool allow_tls10() const override { return false; }
    bool allow_tls11() const override { return false; }

  private:
    std::string cipher;
};

class Callbacks : public Botan::TLS::Callbacks {
  public:
    explicit Callbacks(int s) : fd(s) {}
    void tls_emit_data(const uint8_t data[], size_t size) override {
        while (size > 0) {
            ssize_t n = write(fd, data, size);
            if (n <= 0)
                return;
            data += n;
            size -= n;
        }
    }
    void tls_record_received(uint64_t, const uint8_t data[], size_t size) override {
        request.append(reinterpret_cast<const char *>(data), size);
    }
    void tls_alert(Botan::TLS::Alert) override {}
    bool tls_session_established(const Botan::TLS::Session &) override { return false; }
    std::string request;

  private:
    int fd;
};

int main(int argc, char **argv) {
    if (argc < 4) {
        std::fprintf(stderr, "usage: botan_server <port> <cert> <key> [<cipher>]\n");
        return 2;
    }
    Botan::AutoSeeded_RNG rng;
    Credentials creds(argv[2], argv[3], rng);
    Policy policy(argc > 4 ? argv[4] : "");
    Botan::TLS::Session_Manager_Noop sessions;
    int ls = socket(AF_INET, SOCK_STREAM, 0);
    int on = 1;
    setsockopt(ls, SOL_SOCKET, SO_REUSEADDR, &on, sizeof on);
    sockaddr_in a{};
    a.sin_family = AF_INET;
    a.sin_port = htons(std::atoi(argv[1]));
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(ls, reinterpret_cast<sockaddr *>(&a), sizeof a) != 0 || listen(ls, 256) != 0) {
        std::perror("bind");
        return 1;
    }
    std::printf("ready\n");
    std::fflush(stdout);
    for (;;) {
        int s = accept(ls, nullptr, nullptr);
        if (s < 0)
            continue;
        Callbacks cb(s);
        try {
            Botan::TLS::Server server(cb, sessions, creds, policy, rng);
            uint8_t buf[16384];
            bool replied = false;
            while (!server.is_closed()) {
                ssize_t n = read(s, buf, sizeof buf);
                if (n <= 0)
                    break;
                server.received_data(buf, n);
                if (!replied && cb.request.find("\r\n\r\n") != std::string::npos) {
                    replied = true;
                    server.send(reply);
                    server.close();
                }
            }
        } catch (const std::exception &) {
        }
        close(s);
    }
}
