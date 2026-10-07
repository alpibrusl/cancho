# The clients `scripts/tls_server_interop.py` runs against `packages/tls`'s
# server (docs/tls-server.md §8, step 2), on top of the image the client's
# interop matrix used (`lexsys-interop`: Ubuntu 24.04 with OpenSSL 3.0.13,
# curl, Go, wolfSSL 5.6.6, Rust and python3-cryptography;
# docs/tls-assurance.md §2): mosquitto's clients and the wolfSSL headers'
# examples' dependencies.
#
#     docker build -t lexsys-tls-server -f scripts/interop/server.Dockerfile scripts/interop
FROM lexsys-interop
RUN apt-get update && apt-get install -y --no-install-recommends mosquitto-clients curl \
    && rm -rf /var/lib/apt/lists/*
