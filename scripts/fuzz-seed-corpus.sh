#!/usr/bin/env bash
# Seeds fuzz/corpus/<target> from the YAML fixtures in the repository.
# Usage: scripts/fuzz-seed-corpus.sh   (run from the repository root)
set -euo pipefail

for target in parse format lint; do
    dir="fuzz/corpus/${target}"
    mkdir -p "${dir}"
    find tests/fixtures crates -path '*fixtures*' \( -name '*.yaml' -o -name '*.yml' \) -type f -print0 |
        while IFS= read -r -d '' file; do
            cp "${file}" "${dir}/$(printf '%s' "${file}" | tr '/' '_')"
        done
done
