#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERT_DIR="${ROOT_DIR}/certs"

mkdir -p "${CERT_DIR}/ca"

CA_KEY="${CERT_DIR}/ca/ca.key"
CA_CERT="${CERT_DIR}/ca/ca.crt"

if [[ ! -f "${CA_KEY}" ]]; then
  openssl genrsa -out "${CA_KEY}" 4096
fi

if [[ ! -f "${CA_CERT}" ]]; then
  openssl req -x509 -new -nodes -key "${CA_KEY}" -sha256 -days 3650 \
    -subj "/CN=astragraph-ca" -out "${CA_CERT}"
fi

for service in proxy graph policy verifier; do
  SERVICE_DIR="${CERT_DIR}/${service}"
  mkdir -p "${SERVICE_DIR}"
  KEY="${SERVICE_DIR}/${service}.key"
  CSR="${SERVICE_DIR}/${service}.csr"
  CRT="${SERVICE_DIR}/${service}.crt"
  EXT="${SERVICE_DIR}/${service}.ext"

  openssl genrsa -out "${KEY}" 4096
  openssl req -new -key "${KEY}" -subj "/CN=${service}" -out "${CSR}"
  cat > "${EXT}" <<EOF
basicConstraints=CA:FALSE
subjectKeyIdentifier=hash
authorityKeyIdentifier=keyid,issuer
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectAltName = @alt_names
[alt_names]
DNS.1 = ${service}
DNS.2 = astragraph-${service}
DNS.3 = localhost
IP.1 = 127.0.0.1
EOF
  openssl x509 -req -in "${CSR}" -CA "${CA_CERT}" -CAkey "${CA_KEY}" -CAcreateserial \
    -out "${CRT}" -days 365 -sha256 -extfile "${EXT}"
done

echo "Certificates generated under ${CERT_DIR}"
