#!/usr/bin/env bash
# Policy gate for the unsafe-code lint level.
#
# Every tracked line that names the lint (outside the documentation paths listed in DOC_PATHS)
# must match an entry of scripts/unsafe-allow.allowlist exactly, compared as a multiset with
# line numbers ignored. An entry is `path<TAB>trimmed line<TAB>item`, where item is, for an
# outer attribute (`#[...]`) in a Rust file, the first following line that is not an attribute,
# comment or blank (the signature the attribute attaches to), and empty otherwise. A line
# added, removed or changed, or an attribute moved to another item, fails.
#
# It also fails on `cap-lints`, on `#[path = ...]` or `include!(...)` naming a file that is not
# `.rs` (such a file would escape the scan of documentation paths), on a manifest that does not
# inherit the workspace lints, and on a shipped crate that depends on `fuzz/`.
#
# Only tracked files are scanned: a new file must be added to the index before this script sees
# it (CI always checks tracked files). To approve a change, edit the allowlist in the same PR.
#
#   scripts/check-unsafe-allow.sh              check the working tree
#   scripts/check-unsafe-allow.sh --self-test  check that the gate rejects known violations
set -euo pipefail

DOC_PATHS=(
  ':(exclude,glob)*.md'
  ':(exclude)specs/'
  ':(exclude)docs/'
  ':(exclude,glob).github/**/*.md'
)

allowlist=scripts/unsafe-allow.allowlist
self=scripts/check-unsafe-allow.sh

# `path<TAB>trimmed line<TAB>item` for every line of $1 naming the lint
entries_of() {
  awk -v path="$1" '
    function trim(s) { gsub(/^[[:space:]]+|[[:space:]]+$/, "", s); return s }
    { lines[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        if (lines[i] !~ /unsafe[-_]code/) continue
        text = trim(lines[i])
        item = ""
        if (path ~ /\.rs$/ && text ~ /^#\[/) {
          for (j = i + 1; j <= NR; j++) {
            next_line = trim(lines[j])
            if (next_line == "" || next_line ~ /^(#|\/\/|\)|\])/) continue
            item = next_line
            break
          }
        }
        printf "%s\t%s\t%s\n", path, text, item
      }
    }' "$1"
}

check() {
  local status=0 actual expected

  actual=$(
    git grep -I -l -E 'unsafe[-_]code' -- . "${DOC_PATHS[@]}" ":!$self" ":!$allowlist" |
      while IFS= read -r file; do entries_of "$file"; done | LC_ALL=C sort
  ) || true
  expected=$(tr -d '\r' <"$allowlist" | LC_ALL=C sort)

  if [ "$actual" != "$expected" ]; then
    echo "error: lines naming the unsafe-code lint differ from $allowlist:" >&2
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") |
      sed -n 's/^< /  removed: /p; s/^> /  added:   /p' >&2 || true
    status=1
  fi

  local caps
  caps=$(git grep -I -n 'cap-lints' -- . "${DOC_PATHS[@]}" ":!$self" || true)
  if [ -n "$caps" ]; then
    echo "error: cap-lints would cap the lint level:" >&2
    printf '%s\n' "$caps" | sed 's/^/  /' >&2
    status=1
  fi

  local includes
  includes=$(
    git grep -I -n -E '#\[path[[:space:]]*=|include!\(' -- '*.rs' |
      grep -v -E '\.rs"' || true
  )
  if [ -n "$includes" ]; then
    echo "error: #[path] or include! must name a .rs file:" >&2
    printf '%s\n' "$includes" | sed 's/^/  /' >&2
    status=1
  fi

  # Crates inherit the workspace lint table; nodejs/ declares its own table, which the
  # allowlist pins. fuzz/ is outside the workspace, so it must never be a dependency of a
  # shipped crate.
  local manifest
  while IFS= read -r manifest; do
    case "$manifest" in
      fuzz/*) continue ;;
    esac
    if grep -q -E 'path[[:space:]]*=[[:space:]]*"[^"]*fuzz' "$manifest"; then
      echo "error: $manifest depends on fuzz/, which has no workspace lints" >&2
      status=1
    fi
    case "$manifest" in
      Cargo.toml | nodejs/Cargo.toml) continue ;;
    esac
    if ! awk '
      /^\[/ { in_lints = ($0 == "[lints]") }
      in_lints && /^workspace[[:space:]]*=[[:space:]]*true[[:space:]]*$/ { found = 1 }
      END { exit !found }
    ' "$manifest"; then
      echo "error: $manifest does not inherit the workspace lints ([lints] workspace = true)" >&2
      status=1
    fi
  done < <(git ls-files '*Cargo.toml')

  return "$status"
}

# Runs the copy of the gate in $1 against a mutated copy of the tree; $2 is the expected result
expect() {
  local dir=$1 want=$2 name=$3 got=0
  (cd "$dir" && ./$self >/dev/null 2>&1) || got=$?
  if { [ "$want" = pass ] && [ "$got" -ne 0 ]; } || { [ "$want" = fail ] && [ "$got" -eq 0 ]; }; then
    echo "self-test FAILED: $name (expected $want, exit $got)" >&2
    return 1
  fi
  echo "self-test ok: $name ($want)"
}

# Applies the mutation $2.. to a fresh copy of the tree and expects the gate to $1
case_in_copy() {
  local want=$1 name=$2
  shift 2
  local dir
  dir=$(mktemp -d)
  git ls-files -z --cached --others --exclude-standard | tar -cf - --null -T - | tar -xf - -C "$dir"
  (cd "$dir" && git init -q && git add -A && "$@")
  local rc=0
  expect "$dir" "$want" "$name" || rc=1
  rm -rf "$dir"
  return "$rc"
}

self_test() {
  local rc=0
  # shellcheck disable=SC2016
  {
    case_in_copy pass "clean tree" true || rc=1
    case_in_copy pass "allowlist with CRLF line ends" \
      sh -c "awk '{ printf \"%s\\r\\n\", \$0 }' $allowlist >a.tmp && mv a.tmp $allowlist" || rc=1
    case_in_copy fail "forbid turned into allow" \
      sed -i.bak 's/forbid(unsafe_code)/allow(unsafe_code)/' crates/fast-yaml-core/src/lib.rs || rc=1
    case_in_copy fail "workspace level turned into allow" \
      sed -i.bak 's/^unsafe_code = "deny"/unsafe_code = "allow"/' Cargo.toml || rc=1
    case_in_copy fail "allow moved to a wider item" \
      sh -c "awk '/^    #\\[allow\\(unsafe_code\\)\\]\$/ { next } /^impl Task for ParseParallelTask/ { print \"#[allow(unsafe_code)]\" } { print }' nodejs/src/parallel.rs >p.tmp && mv p.tmp nodejs/src/parallel.rs" || rc=1
    case_in_copy fail "hyphenated spelling in a workflow" \
      sh -c "echo '  RUSTFLAGS: \"-A unsafe-code\"' >>.github/workflows/ci.yml" || rc=1
    case_in_copy fail "cap-lints" \
      sh -c "echo 'cap-lints = \"allow\"' >>.config/mise/config.toml" || rc=1
    case_in_copy fail "path attribute naming a Markdown file" \
      sh -c "printf '#[path = \"x.md\"]\\nmod x;\\n' >>crates/fast-yaml-core/src/lib.rs" || rc=1
    case_in_copy fail "include! naming a Markdown file" \
      sh -c "printf 'include!(\"x.md\");\\n' >>crates/fast-yaml-core/src/lib.rs" || rc=1
    case_in_copy fail "shipped crate depending on fuzz" \
      sh -c "echo 'fuzz = { path = \"../../fuzz\" }' >>crates/fast-yaml-cli/Cargo.toml" || rc=1
    case_in_copy fail "crate without workspace lints" \
      sed -i.bak '/^workspace = true$/d' crates/fast-yaml-linter/Cargo.toml || rc=1
  }
  return "$rc"
}

cd "$(git rev-parse --show-toplevel)"

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit
fi

if check; then
  echo "ok: lint-level allowances match $allowlist"
else
  exit 1
fi
