#!/bin/sh
# The client certificates `scripts/tls_memory.py --client` presents (docs/tls-memory.md §11): test material only, made
# once with OpenSSL and committed so a run on another machine presents the same bytes.
#
#   p256/  a device's certificate: ECDSA P-256 key, signed by an ECDSA P-256 CA that is the store (a chain of one);
#          the subject a country, an organization and a common name, one URI and one DNS subjectAltName, keyUsage
#          digitalSignature, extendedKeyUsage clientAuth, subject and authority key identifiers: about 500 bytes of DER
#   rsa/   for comparison: an RSA-2048 leaf under an RSA-2048 intermediate under an RSA-2048 root (the store), the
#          client sending the leaf and the intermediate
#
# Each directory has `ca.pem` (the store, for `--client-ca`), `chain.pem` (what the client sends) and `key.pem`.
set -eu
cd "$(dirname "$0")"
days=9000
ext() {
  cat <<X
[ca]
basicConstraints = critical, CA:true
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
[leaf]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = clientAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
subjectAltName = URI:urn:dev:mac:0024befffe804ff1, DNS:device-000017.fleet.example
X
}
ext > ext.cnf
mkdir -p p256 rsa
# P-256
openssl ecparam -name prime256v1 -genkey -noout -out p256/ca-key.pem
openssl req -new -x509 -key p256/ca-key.pem -subj "/C=ES/O=Example Fleet/CN=Example Fleet Devices CA" -days $days -sha256 \
  -addext "basicConstraints=critical,CA:true" -addext "keyUsage=critical,keyCertSign,cRLSign" -out p256/ca.pem
openssl ecparam -name prime256v1 -genkey -noout -out p256/key.ec.pem
openssl pkcs8 -topk8 -nocrypt -in p256/key.ec.pem -out p256/key.pem
openssl req -new -key p256/key.pem -subj "/C=ES/O=Example Fleet/CN=device-000017" -out p256/leaf.csr
openssl x509 -req -in p256/leaf.csr -CA p256/ca.pem -CAkey p256/ca-key.pem -set_serial 0x5a17c0de0017 -days $days -sha256 \
  -extfile ext.cnf -extensions leaf -out p256/chain.pem
# RSA-2048, a chain of two
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out rsa/root-key.pem 2>/dev/null
openssl req -new -x509 -key rsa/root-key.pem -subj "/C=ES/O=Example Fleet/CN=Example Fleet Root CA" -days $days -sha256 \
  -addext "basicConstraints=critical,CA:true" -addext "keyUsage=critical,keyCertSign,cRLSign" -out rsa/ca.pem
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out rsa/int-key.pem 2>/dev/null
openssl req -new -key rsa/int-key.pem -subj "/C=ES/O=Example Fleet/CN=Example Fleet Devices CA 2" -out rsa/int.csr
openssl x509 -req -in rsa/int.csr -CA rsa/ca.pem -CAkey rsa/root-key.pem -set_serial 2 -days $days -sha256 \
  -extfile ext.cnf -extensions ca -out rsa/int.pem
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out rsa/key.pem 2>/dev/null
openssl req -new -key rsa/key.pem -subj "/C=ES/O=Example Fleet/CN=device-000017" -out rsa/leaf.csr
openssl x509 -req -in rsa/leaf.csr -CA rsa/int.pem -CAkey rsa/int-key.pem -set_serial 0x5a17c0de0017 -days $days -sha256 \
  -extfile ext.cnf -extensions leaf -out rsa/leaf.pem
cat rsa/leaf.pem rsa/int.pem > rsa/chain.pem
rm -f ext.cnf p256/leaf.csr p256/key.ec.pem p256/ca-key.pem rsa/int.csr rsa/leaf.csr rsa/root-key.pem rsa/int-key.pem rsa/leaf.pem rsa/int.pem
for d in p256 rsa; do
  printf '%s leaf: %s bytes of DER\n' "$d" "$(openssl x509 -in $d/chain.pem -outform DER | wc -c | tr -d ' ')"
done
