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
| X-1 | Option precedence between a formatter-style flag and a rule option | CLI, Python, Node.js | CLI `--indent-size` overrides `rules.indentation.spaces`; in the bindings the `rules` patch is applied last and wins over `indent_size`/`indentSize` ([[003-lint/spec]] D-30). | One winner on every surface. |
| X-3 | Machine-format error reporting | `lint`, `format`, `convert`, `parse` | `lint` prints a `syntax` diagnostic in `json`, `github`, `sarif`, `parsable`; text stays stderr-only ([[003-lint/spec]] D-3). | Whether `text` and the other subcommands get a machine-readable error form. |
| X-4 | ACLs and large extended attributes in atomic writes | `format -i`, `convert -o`, `lint -o` | Xattrs are copied on Unix; ACLs are not and a very large attribute is read whole ([[005-batch-parallel/spec]] item 15, [[009-limits-security/spec]] item 13). | Copy ACLs where the platform exposes them, or cap attribute size. |
| X-5 | Lint exit codes by failure class | CLI | A single syntax error exits 1, batch exits 2 ([[003-lint/spec]] D-1). | One code per class on every path. |
| X-6 | Lint output formats in bindings | Python, Node.js | Structured diagnostics and text/json only ([[003-lint/spec]] D-16). | Expose github, sarif, parsable. |
| X-7 | Per-rule `ignore` anchoring | CLI, Python, Node.js | CLI anchors at the declaring config's directory; bindings at the process working directory; yamllint matches the typed path relative to the working directory ([[003-lint/spec]] D-26). | Confirm the canonical-path semantics. |
| X-8 | Lint speed on flow-heavy input | CLI, bindings | Wall time is within noise of 5db14de on equal output after #600 ([[003-lint/spec]] D-34); the residual is RSS proportional to the diagnostic count. | No regression gate exists in CI. |
