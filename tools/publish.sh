#!/usr/bin/env bash
# Publishes the add-on: builds and packs it, signs a release for it, refuses
# a version already in the bucket, uploads the package and then release.json
# (whose presence makes it a release), reads both back, and tags v<version>.
#
#   tools/publish.sh --dry-run|--publish --notes "text" | --notes-file <file> [--allow-dirty]
#
# Objects in the bucket are locked: they can never be replaced or deleted, so
# a published version is final and the next one must be bumped.
set -euo pipefail

usage() {
  echo "usage: tools/publish.sh --dry-run|--publish (--notes \"text\" | --notes-file <file>) [--allow-dirty]" >&2
  exit 2
}
die() { echo "publish: $*" >&2; exit 1; }

mode="" notes="" notes_set=0 allow_dirty=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) mode=dry ;;
    --publish) mode=publish ;;
    --allow-dirty) allow_dirty=1 ;;
    --notes) [ $# -ge 2 ] || usage; notes=$2; notes_set=1; shift ;;
    --notes-file) [ $# -ge 2 ] || usage; [ -f "$2" ] || die "no notes file $2"; notes=$(cat -- "$2"); notes_set=1; shift ;;
    -h|--help) usage ;;
    *) echo "publish: unknown argument $1" >&2; usage ;;
  esac
  shift
done
[ -n "$mode" ] || usage
[ "$notes_set" = 1 ] || die "release notes are required (--notes or --notes-file)"

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
secrets="$(cd "$root/.." && pwd)/secrets"
r2_env=${ADDONS_R2_ENV:-$secrets/addons-r2.env}
key=${ADDONS_SIGNING_KEY:-$secrets/addons-signing.pem}
key_id="addons-1"
expected_pub="i0PVdz0mpjBbK2pWYZp+g4ljLnqpr2PoAnyjzILmK20="
max_pkg=$((64 * 1024 * 1024))
max_env=$((16 * 1024))

for t in jq openssl unzip git sha256sum base64; do
  command -v "$t" >/dev/null || die "$t is not installed"
done
rclone=$(command -v rclone || true)
[ -n "$rclone" ] || rclone="$HOME/.local/bin/rclone"
[ -x "$rclone" ] || die "rclone is not installed"
[ -r "$key" ] || die "signing key not readable"
[ -r "$r2_env" ] || die "R2 credentials not readable"

notes_len=$(printf '%s' "$notes" | LC_ALL=C.UTF-8 wc -m)
[ "$notes_len" -ge 1 ] || die "release notes are empty"
[ "$notes_len" -le 400 ] || die "release notes are $notes_len characters, at most 400"

# 1. A clean tree, so what ships is what is committed.
if [ "$allow_dirty" = 0 ] && [ -n "$(git status --porcelain)" ]; then
  die "the working tree has changes (commit them, or --allow-dirty)"
fi

# 2. The repository's own build and pack.
src_id=$(jq -er '.id' ui/addon.json)
src_ver=$(jq -er '.version' ui/addon.json)
tools/pack.sh >/dev/null
pkg="dist/$src_id-$src_ver.apocaddon"
[ -f "$pkg" ] || die "pack did not produce $pkg"

# 3. The manifest as it ships, read from inside the package.
manifest=$(unzip -p "$pkg" addon.json) || die "the package has no addon.json"
id=$(jq -er '.id | strings' <<<"$manifest") || die "addon.json has no id"
version=$(jq -er '.version | strings' <<<"$manifest") || die "addon.json has no version"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version $version is not major.minor.patch"
[[ "$id" =~ ^[a-z0-9][a-z0-9-]*$ ]] || die "id $id is not a plain name"
[ "$id" = "$src_id" ] && [ "$version" = "$src_ver" ] || die "the package's manifest does not match ui/addon.json"

# 4. Limits.
size=$(stat -c %s "$pkg")
[ "$size" -le "$max_pkg" ] || die "package is $size bytes, at most $max_pkg"
sha=$(sha256sum "$pkg" | cut -d' ' -f1)

# 5. The release, signed and checked.
published=$(date -u +%Y-%m-%dT%H:%M:%SZ)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
jq -cj --arg size "$size" --arg sha "$sha" --arg notes "$notes" --arg published "$published" '{
    schema: "addon-release/1",
    id, name, about, version,
    games: (.games // []),
    needs: (.needs // []),
    size: ($size | tonumber),
    sha256: $sha,
    notes: $notes,
    published: $published
  }' <<<"$manifest" >"$work/release"
openssl pkeyutl -sign -rawin -inkey "$key" -in "$work/release" -out "$work/sig" 2>/dev/null || die "signing failed"
openssl pkey -in "$key" -pubout -out "$work/pub.pem" 2>/dev/null || die "cannot derive the public key"
pub=$(openssl pkey -pubin -in "$work/pub.pem" -outform DER 2>/dev/null | tail -c 32 | base64 -w0)
[ "$pub" = "$expected_pub" ] || die "the signing key is not $key_id"
openssl pkeyutl -verify -rawin -pubin -inkey "$work/pub.pem" -in "$work/release" -sigfile "$work/sig" >/dev/null 2>&1 \
  || die "the signature does not verify"
jq -cnj --arg keyId "$key_id" --arg release "$(base64 -w0 "$work/release")" --arg signature "$(base64 -w0 "$work/sig")" \
  '{keyId: $keyId, release: $release, signature: $signature}' >"$work/release.json"
env_size=$(stat -c %s "$work/release.json")
[ "$env_size" -le "$max_env" ] || die "release.json is $env_size bytes, at most $max_env"

# The bucket, through environment variables only: nothing is written to disk.
set +x
# shellcheck disable=SC1090
. "$r2_env"
export RCLONE_CONFIG_ADDONS_TYPE=s3
export RCLONE_CONFIG_ADDONS_PROVIDER=Cloudflare
export RCLONE_CONFIG_ADDONS_ACCESS_KEY_ID="$R2_PUBLISH_ACCESS_KEY_ID"
export RCLONE_CONFIG_ADDONS_SECRET_ACCESS_KEY="$R2_PUBLISH_SECRET_ACCESS_KEY"
export RCLONE_CONFIG_ADDONS_ENDPOINT="https://$R2_ACCOUNT_ID.r2.cloudflarestorage.com"
export RCLONE_CONFIG_ADDONS_NO_CHECK_BUCKET=true
export RCLONE_CONFIG=/dev/null
unset R2_READ_ACCESS_KEY_ID R2_READ_SECRET_ACCESS_KEY R2_PUBLISH_ACCESS_KEY_ID R2_PUBLISH_SECRET_ACCESS_KEY
prefix="addons:$R2_BUCKET/$id/$version"
pkg_obj="$prefix/$id-$version.apocaddon"
rel_obj="$prefix/release.json"

# 6. Whether either object is already there. A path that is not there can
# still stat as a directory on S3, so the version's folder is listed instead.
rc=0
listing=$("$rclone" lsf --files-only "$prefix/" 2>/dev/null) || rc=$?
case $rc in
  0|3) ;;
  *) die "cannot reach the bucket to check $id/$version (rclone exit $rc)" ;;
esac
taken=""
for name in "$id-$version.apocaddon" release.json; do
  if grep -qxF -- "$name" <<<"$listing"; then taken="$taken $id/$version/$name"; fi
done

echo "$id $version  $size bytes  sha256 $sha"
if [ "$mode" = dry ]; then
  cp "$work/release.json" "dist/$id-$version.release.json"
  base64 -d <<<"$(jq -r .release "$work/release.json")" | jq .
  echo "envelope: dist/$id-$version.release.json ($env_size bytes), signature verified"
  if [ -n "$taken" ]; then echo "already in the bucket:$taken (publishing would refuse)"; else echo "not yet in the bucket"; fi
  exit 0
fi

[ -z "$taken" ] || die "already in the bucket:$taken; locked objects cannot be replaced, bump the version"
git rev-parse -q --verify "refs/tags/v$version" >/dev/null && die "tag v$version already exists"

# 7. The package first, release.json last.
"$rclone" copyto --s3-no-check-bucket "$pkg" "$pkg_obj" >/dev/null 2>&1 || die "uploading the package failed"
"$rclone" copyto --s3-no-check-bucket "$work/release.json" "$rel_obj" >/dev/null 2>&1 || die "uploading release.json failed (the package is up; rerun cannot replace it)"

# 8. Read back.
got=$("$rclone" cat "$pkg_obj" 2>/dev/null | sha256sum | cut -d' ' -f1)
[ "$got" = "$sha" ] || die "the package read back differs ($got)"
want=$(sha256sum "$work/release.json" | cut -d' ' -f1)
got=$("$rclone" cat "$rel_obj" 2>/dev/null | sha256sum | cut -d' ' -f1)
[ "$got" = "$want" ] || die "release.json read back differs ($got)"

# 9. Tag.
git tag -a "v$version" -m "$id $version" -m "$notes"
echo "published $id/$version, read back and matching; tagged v$version"
echo "push with: git push origin v$version"
