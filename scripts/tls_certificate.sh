#!/usr/bin/env bash

set -euo pipefail

temporary=""

cleanup() {
  [[ -z "$temporary" ]] || rm -rf -- "$temporary"
}

trap cleanup EXIT

die() {
  echo "ERROR: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command '$1' is not installed"
}

normalize_fingerprint() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/^sha256://; s/://g; s/[[:space:]]//g'
}

inspect_chain() {
  local host="$1" port="$2" chain cert count=0
  temporary="$(mktemp -d)"
  chain="$temporary/chain.pem"

  openssl s_client -showcerts -connect "$host:$port" -servername "$host" </dev/null >"$chain" 2>/dev/null ||
    die "could not retrieve the TLS chain from $host:$port"

  awk -v out="$temporary" '
    /-----BEGIN CERTIFICATE-----/ { n++; file=sprintf("%s/cert-%02d.pem", out, n) }
    file != "" { print > file }
    /-----END CERTIFICATE-----/ { close(file); file="" }
  ' "$chain"

  for cert in "$temporary"/cert-*.pem; do
    [[ -f "$cert" ]] || continue
    count=$((count + 1))
    echo "Certificate $count"
    openssl x509 -in "$cert" -noout -subject -issuer -dates -fingerprint -sha256
    echo
  done

  [[ "$count" -gt 0 ]] || die "the server returned no certificates"
}

verify_server() {
  local host="$1" port="$2" ca_file="$3"
  [[ -f "$ca_file" ]] || die "CA file '$ca_file' does not exist; run make tls-update with reviewed CA inputs"
  openssl x509 -in "$ca_file" -noout >/dev/null || die "'$ca_file' is not a valid PEM certificate"
  openssl s_client -brief -verify_return_error -CAfile "$ca_file" \
    -connect "$host:$port" -servername "$host" </dev/null
}

update_ca() {
  local destination="$1" url="$2" expected="$3" candidate actual
  temporary="$(mktemp -d)"
  candidate="$temporary/root-ca.pem"

  curl --fail --location --proto '=https' --tlsv1.2 --output "$candidate" "$url"
  openssl x509 -in "$candidate" -noout >/dev/null || die "downloaded file is not a valid PEM certificate"
  actual="$(openssl x509 -in "$candidate" -outform DER | openssl dgst -sha256 | sed 's/^.*= //')"

  [[ "$(normalize_fingerprint "$actual")" == "$(normalize_fingerprint "$expected")" ]] ||
    die "CA fingerprint mismatch (expected $expected, received $actual)"

  mkdir -p "$(dirname "$destination")"
  cp "$candidate" "$destination"
  echo "Updated $destination"
  openssl x509 -in "$destination" -noout -subject -issuer -dates -fingerprint -sha256
}

require_command openssl

case "${1:-}" in
  inspect)
    [[ "$#" -eq 3 ]] || die "usage: $0 inspect HOST PORT"
    inspect_chain "$2" "$3"
    ;;
  verify)
    [[ "$#" -eq 4 ]] || die "usage: $0 verify HOST PORT CA_FILE"
    verify_server "$2" "$3" "$4"
    ;;
  update)
    [[ "$#" -eq 4 ]] || die "usage: $0 update CA_FILE CA_URL CA_SHA256"
    require_command curl
    update_ca "$2" "$3" "$4"
    ;;
  *)
    die "usage: $0 {inspect|verify|update} ..."
    ;;
esac
