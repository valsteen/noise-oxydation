#!/usr/bin/env bash
# Downloads the checksum-pinned evaluation audio into audio/sources/ (ignored by Git).
#
# Speech: Open Speech Repository, American English 8 kHz Harvard sentences (attribution: Open Speech Repository).
# Noise: DEMAND (Thiemann, Ito and Vincent, Zenodo record 1227121, DOI 10.5281/zenodo.1227121, CC BY-SA 3.0),
# channel 1 of the OOFFICE and PCAFETER 16 kHz recordings. Only the byte range holding each channel's raw-deflate
# zip member payload is fetched; it is inflated locally and verified by CRC32 and SHA-256.
#
# The script is idempotent: verified files are kept, anything else is fetched again. Any checksum mismatch stops it
# with a non-zero exit status. Never commit the downloaded audio or anything derived from it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sources="$root/audio/sources"
mkdir -p "$sources"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

fail() {
    echo "error: $*" >&2
    exit 1
}

# `curl -q` must come first so that a user ~/.curlrc (for example one adding `-w '\n'`) is ignored.
download() {
    local url="$1" destination="$2"
    shift 2
    curl -q --fail --location --silent --show-error --retry 3 "$@" --output "$destination" "$url"
}

verified() {
    local path="$1" sha256="$2"
    [[ -f "$path" ]] && [[ "$(sha256_of "$path")" == "$sha256" ]]
}

fetch_file() {
    local name="$1" url="$2" size="$3" sha256="$4"
    local path="$sources/$name"
    if verified "$path" "$sha256"; then
        echo "ok       $name (already verified)"
        return
    fi
    local partial="$path.partial"
    download "$url" "$partial"
    local actual_size
    actual_size="$(wc -c <"$partial" | tr -d ' ')"
    [[ "$actual_size" == "$size" ]] || fail "$name: expected $size bytes, downloaded $actual_size"
    local actual_sha256
    actual_sha256="$(sha256_of "$partial")"
    [[ "$actual_sha256" == "$sha256" ]] || fail "$name: expected sha256 $sha256, downloaded $actual_sha256"
    mv "$partial" "$path"
    echo "fetched  $name ($size bytes, sha256 verified)"
}

fetch_zip_member() {
    local name="$1" url="$2" first="$3" last="$4" crc32="$5" sha256="$6"
    local path="$sources/$name"
    if verified "$path" "$sha256"; then
        echo "ok       $name (already verified)"
        return
    fi
    local compressed="$path.deflate.partial" partial="$path.partial"
    download "$url" "$compressed" --range "$first-$last"
    local expected_size=$((last - first + 1)) actual_size
    actual_size="$(wc -c <"$compressed" | tr -d ' ')"
    [[ "$actual_size" == "$expected_size" ]] ||
        fail "$name: expected $expected_size compressed bytes, downloaded $actual_size (range request ignored?)"
    python3 - "$compressed" "$partial" "$crc32" <<'PYTHON' || fail "$name: inflating the zip member failed"
import sys
import zlib

source, destination, expected_crc32 = sys.argv[1], sys.argv[2], sys.argv[3]
with open(source, "rb") as handle:
    compressed = handle.read()
inflater = zlib.decompressobj(-15)
data = inflater.decompress(compressed) + inflater.flush()
if not inflater.eof or inflater.unused_data:
    sys.exit("the byte range is not exactly one raw-deflate stream")
actual_crc32 = format(zlib.crc32(data) & 0xFFFFFFFF, "08x")
if actual_crc32 != expected_crc32:
    sys.exit(f"CRC32 mismatch: expected {expected_crc32}, got {actual_crc32}")
with open(destination, "wb") as handle:
    handle.write(data)
PYTHON
    rm -f "$compressed"
    local actual_sha256
    actual_sha256="$(sha256_of "$partial")"
    [[ "$actual_sha256" == "$sha256" ]] || fail "$name: expected sha256 $sha256, extracted $actual_sha256"
    mv "$partial" "$path"
    echo "fetched  $name (CRC32 $crc32 and sha256 verified)"
}

osr="https://www.voiptroubleshooter.com/open_speech/american"
demand="https://zenodo.org/api/records/1227121/files"

fetch_file OSR_us_000_0030_8k.wav "$osr/OSR_us_000_0030_8k.wav" 750886 \
    d3e1cfba98a44ddfc6e99508e663794c8dc41bdb2baced224f779c453143b622
fetch_file OSR_us_000_0031_8k.wav "$osr/OSR_us_000_0031_8k.wav" 674110 \
    dff349ae6b5d71a696c22bb838319addd5eb10dc0fafd1b15ddfbde13d57a43c
fetch_zip_member DEMAND_OOFFICE_ch01.wav "$demand/OOFFICE_16k.zip/content" 33612426 39094287 b9dbc8ba \
    a831879b26e15732ed511a8d3bf72e23aa1689383aac6c5ba76bcaaa528bc2f2
fetch_zip_member DEMAND_PCAFETER_ch01.wav "$demand/PCAFETER_16k.zip/content" 40516682 47176540 5942571b \
    854dd0e5ef334a66abdd02d6d2b3e9d01b685df033d0acd1cd06e471c062550b

echo "all evaluation sources verified in audio/sources/"
