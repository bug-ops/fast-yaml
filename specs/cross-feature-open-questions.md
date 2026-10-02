---
tags:
  - sdd
  - spec
created: 2026-10-01
status: living
related:
  - "[[constitution]]"
---

# Cross-feature open questions

Items that span several capabilities and are not owned by one spec. A feature spec keeps the item that is local to it; this page keeps the decision that must be taken once for all surfaces. Remove a row when its spec states the requirement.

| # | Topic | Surfaces | State | Decision needed |
|---|-------|----------|-------|-----------------|
| X-1 | Option precedence between a formatter-style flag and a rule option | CLI, Python, Node.js | Resolved: the dedicated option wins on every surface ([[003-lint/spec]] D-30). | closed |
| X-3 | Machine-format error reporting | `lint`, `format`, `convert`, `parse` | `lint` prints a `syntax` diagnostic in `json`, `github`, `sarif`, `parsable`; text stays stderr-only ([[003-lint/spec]] D-3). | Whether `text` and the other subcommands get a machine-readable error form. |
| X-4 | ACLs and large extended attributes in atomic writes | `format -i`, `convert -o`, `lint -o` | Xattrs are copied on Unix; ACLs are not and a very large attribute is read whole ([[005-batch-parallel/spec]] item 15, [[009-limits-security/spec]] item 13). | Copy ACLs where the platform exposes them, or cap attribute size. |
| X-5 | Lint exit codes by failure class | CLI | A single syntax error exits 1, batch exits 2 ([[003-lint/spec]] D-1). | One code per class on every path. |
| X-6 | Lint output formats in bindings | Python, Node.js | Structured diagnostics and text/json only ([[003-lint/spec]] D-16). | Expose github, sarif, parsable. |
| X-7 | Per-rule `ignore` anchoring | CLI, Python, Node.js | CLI anchors at the declaring config's directory; bindings at the process working directory (also Python `with_rule_config`); yamllint matches the typed path relative to the working directory ([[003-lint/spec]] D-26). | Confirm the canonical-path semantics. |
| X-8 | Lint speed on flow-heavy input | CLI, bindings | Wall time is within noise of 5db14de on equal output after #600 ([[003-lint/spec]] D-34); the residual is RSS proportional to the diagnostic count. | No regression gate exists in CI. |
| X-9 | Emitter options accepted but never applied: `width`, `allow_unicode`, batch `sort_keys`; `default_flow_style` is `Option<bool>` with two equal states | CLI (`--width`), Python, Node.js, core `EmitterConfig` | Validated on every surface, no effect on output; `sort_keys` is implemented twice in the bindings, already diverged, and sorts integer keys as text (#639, #640; [[002-format/spec]] item 1, [[007-python-api/spec]], [[008-nodejs-api/spec]]). | Implement folding, escaping and sorting once in core, or remove the options on every surface (proposed: remove; sorting opt-in). |
| X-10 | Comment loss in the bindings' file formatters | Python `format_files*`, Node.js `formatFiles*` | Comments are stripped silently, also in place; the CLI refuses without `--strip-comments` ([[002-format/spec]] item 7). | Mirror the CLI comment policy (proposed: refuse unless explicitly allowed). |
| X-11 | Duplicate-key policy outside lint | `fy convert`, Python and Node.js loaders | Duplicates collapse to first position, last value with no warning; Node.js `allowDuplicateKeys`, `schema` and `filename` are accepted and ignored ([[004-convert/spec]] item 1, [[008-nodejs-api/spec]]). | Opt-in strict loader that rejects duplicates; lint stays the default reporter. |
| X-12 | Tab after a mapping colon (`a:\t1`) | core scanner, every surface | Rejected by parse and format although YAML 1.2.2 allows it; inherited from the parser ([[002-format/spec]] item 6, [[007-python-api/spec]]). | Accepted limitation tracked upstream, or a parser fix. |
| X-13 | Error contract in the bindings | Python, Node.js | Python raises plain `ValueError` and never attaches `Mark`; Node.js errors have no `code` for loader and limit failures ([[007-python-api/spec]], [[008-nodejs-api/spec]]). | Typed exceptions with marks and one error-code table (proposed). |
| X-14 | Integers between 2^53 and i64 in Node.js | Node.js | Rounded to a double; integers beyond i64 load as exact decimal strings, unlike Python and the CLI, which keep every integer exact ([[008-nodejs-api/spec]]). | `bigint` for integers outside the safe range (proposed). |
| X-15 | Performance claims and supported runtime versions | Python, Node.js READMEs | README claims (5-10x loader, 3-6x parallel) have no reproducible benchmark in the repository; CI covers a subset of Python 3.10-3.14 ([[007-python-api/spec]], [[008-nodejs-api/spec]]). | Add a reproducible benchmark before keeping the claims; fix the supported-version policy (proposed: latest minor only). |
