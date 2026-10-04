#!/bin/sh
# Make the certificates the TLS tests use, in the directory given (default ./certs).
#
#   good        leaf for hooks.test, signed by the trusted CA            (RSA 2048 and ECDSA P-256 variants)
#   wronghost   leaf for other.test, signed by the trusted CA            -> X509_V_ERR_HOSTNAME_MISMATCH (62)
#   expired     leaf for hooks.test, valid 2020-01-01..2020-01-02        -> X509_V_ERR_CERT_HAS_EXPIRED (10)
#   notyet      leaf for hooks.test, valid from 2090                     -> X509_V_ERR_CERT_NOT_YET_VALID (9)
#   selfsigned  a self-signed leaf for hooks.test                        -> X509_V_ERR_DEPTH_ZERO_SELF_SIGNED_CERT (18)
#   otherpki    leaf + intermediate + root of a PKI the client does not trust, all sent
#                                                                         -> X509_V_ERR_SELF_SIGNED_CERT_IN_CHAIN (19)
#   noint       leaf signed by an intermediate of the TRUSTED CA, which the server does not send
#                                                                         -> X509_V_ERR_UNABLE_TO_GET_ISSUER_CERT_LOCALLY (20)
#   withint     the same leaf, with the intermediate sent                 -> verifies
# The trust store the client is given is ca.pem only.
set -eu
D=${1:-certs}
mkdir -p "$D"
cd "$D"
rm -rf ca.db && mkdir ca.db && : > ca.db/index.txt && echo 1000 > ca.db/serial

cat > ca.cnf <<CNF
[ca]
default_ca = CA_default
[CA_default]
dir = ./ca.db
database = ./ca.db/index.txt
new_certs_dir = ./ca.db
serial = ./ca.db/serial
default_md = sha256
policy = policy_any
unique_subject = no
copy_extensions = copy
[policy_any]
commonName = supplied
[v3_ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
[v3_int]
basicConstraints = critical,CA:TRUE,pathlen:0
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid
CNF

# make_ca <name> : a self-signed CA in <name>.pem/<name>.key
make_ca() {
  openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout "$1.key" -out "$1.pem" \
    -days 3650 -subj "/CN=$1" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
}
# make_leaf <name> <dns> <signer> <keyspec> [start end] : leaf <name>.pem/<name>.key signed by <signer>
make_leaf() {
  name=$1; dns=$2; signer=$3; keyspec=$4; start=${5:-}; end=${6:-}
  case "$keyspec" in
    rsa) openssl req -newkey rsa:2048 -nodes -keyout "$name.key" -out "$name.csr" -subj "/CN=$dns" 2>/dev/null ;;
    ec)  openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout "$name.key" -out "$name.csr" -subj "/CN=$dns" 2>/dev/null ;;
  esac
  printf "subjectAltName=DNS:%s\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nbasicConstraints=critical,CA:FALSE\n" "$dns" > "$name.ext"
  if [ -n "$start" ]; then
    openssl ca -config ca.cnf -batch -notext -in "$name.csr" -out "$name.pem" -cert "$signer.pem" -keyfile "$signer.key" \
       -startdate "$start" -enddate "$end" -extfile "$name.ext" 2>/dev/null
  else
    openssl x509 -req -in "$name.csr" -CA "$signer.pem" -CAkey "$signer.key" -CAcreateserial -out "$name.pem" -days 365 -extfile "$name.ext" 2>/dev/null
  fi
  rm -f "$name.csr" "$name.ext"
}

make_ca ca
make_leaf good_rsa hooks.test ca rsa
make_leaf good_ec hooks.test ca ec
make_leaf wronghost other.test ca ec
make_leaf expired hooks.test ca ec 20200101000000Z 20200102000000Z
make_leaf notyet hooks.test ca ec 20900101000000Z 20910101000000Z

# self-signed leaf
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout selfsigned.key -out selfsigned.pem \
  -days 365 -subj "/CN=hooks.test" -addext "subjectAltName=DNS:hooks.test" 2>/dev/null

# another PKI the client does not trust: root -> intermediate -> leaf; the server sends all three
make_ca otherroot
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout otherint.key -out otherint.csr -subj "/CN=otherint" 2>/dev/null
openssl x509 -req -in otherint.csr -CA otherroot.pem -CAkey otherroot.key -CAcreateserial -out otherint.pem -days 365 -extfile ca.cnf -extensions v3_int 2>/dev/null
make_leaf otherleaf hooks.test otherint ec
cat otherleaf.pem otherint.pem otherroot.pem > otherpki_chain.pem

# an intermediate of the trusted CA; a leaf under it; chain with and without the intermediate
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout int.key -out int.csr -subj "/CN=int" 2>/dev/null
openssl x509 -req -in int.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out int.pem -days 365 -extfile ca.cnf -extensions v3_int 2>/dev/null
make_leaf intleaf hooks.test int ec
cp intleaf.pem noint_chain.pem
cat intleaf.pem int.pem > withint_chain.pem

# for the client that wants a CA file that does not contain the right CA
cp otherroot.pem wrongca.pem
rm -f *.csr *.srl
ls
