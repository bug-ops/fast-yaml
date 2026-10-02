# Security Policy

## Supported Versions

We release security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.7.x   | :white_check_mark: |
| < 0.7   | :x:                |

**Note:** We only support the latest minor version. Please upgrade to receive security updates.

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please follow responsible disclosure practices.

### Private Disclosure Process

**DO NOT** open a public GitHub issue for security vulnerabilities.

Instead, please report security issues privately:

1. **Email:** Send details to the project maintainers (contact via GitHub profile)
2. **GitHub Security Advisory:** Use [GitHub's private vulnerability reporting](https://github.com/bug-ops/fast-yaml/security/advisories/new)

### What to Include

Please provide as much information as possible:

- **Vulnerability Description**: What is the security issue?
- **Affected Components**: Which parts of the project are affected?
  - Rust crates (fast-yaml-core, fast-yaml-linter, etc.)
  - Python bindings
  - NodeJS bindings
- **Impact Assessment**: What can an attacker accomplish?
- **Reproduction Steps**: Detailed steps to reproduce the issue
- **Environment Details**:
  - Operating system
  - Rust version
  - Python/NodeJS version (if applicable)
  - fast-yaml version
- **Proof of Concept**: Code example demonstrating the issue (if available)
- **Suggested Fix**: Any ideas for remediation (optional)

### Example Report

```
Subject: [SECURITY] Buffer overflow in YAML parser

Description:
A buffer overflow vulnerability exists in the YAML parser when
processing malformed input with deeply nested structures.

Affected Component:
- fast-yaml-core v0.7.0
- All language bindings (Python, NodeJS)

Impact:
An attacker can cause a denial of service or potentially execute
arbitrary code by providing a crafted YAML file with 1000+ levels
of nesting.

Reproduction:
1. Create a YAML file with deeply nested mappings (see attached poc.yaml)
2. Parse the file using fast_yaml.safe_load()
3. Application crashes with segmentation fault

Environment:
- OS: Ubuntu 22.04
- Rust: 1.91.0
- fast-yaml: 0.7.0
- Python: 3.11

PoC: [Attached or link to private repository]
```

## Response Timeline

We aim to respond to security reports according to the following timeline:

| Stage | Timeline |
|-------|----------|
| **Initial Response** | Within 48 hours |
| **Vulnerability Assessment** | Within 7 days |
| **Fix Development** | Within 30 days (depending on severity) |
| **Security Patch Release** | As soon as fix is ready and tested |
| **Public Disclosure** | 90 days after patch release (coordinated) |

### Severity Levels

We classify vulnerabilities using the following severity levels:

**Critical:**
- Remote code execution
- Authentication bypass
- Data exfiltration

**High:**
- Denial of service
- Privilege escalation
- Memory corruption

**Medium:**
- Information disclosure
- Logic errors with security implications

**Low:**
- Minor information leaks
- Best practice violations

## Security Best Practices

### For Users

**Resource Limits (CLI):**
```bash
# Every limit has a safe default; tighten them for untrusted input
fy parse --max-input-bytes 10MiB --max-depth 64 --max-alias-bytes 1MiB --max-documents 1000 untrusted.yaml
```

| Limit | Flag | Default | Range |
|-------|------|---------|-------|
| Input size | `--max-input-bytes` | 100 MiB | 1 B to 1 GiB |
| Nesting depth | `--max-depth` | 256 | 1 to 512 |
| Alias expansion | `--max-alias-bytes` | 64 MiB | 1 B to 1 GiB |
| Documents per stream | `--max-documents` | 100000 | 1 to 10000000 |
| Parser scan-ahead | `--max-scan-ahead` | 4 MiB | 1 B to 1 GiB |

A limit that is hit fails with a `YAML resource limit exceeded` error (exit 1) and a hint naming the flag to raise. For `fy lint`, `max-input-bytes`, `max-scan-ahead` and `max-diagnostics` can also be set in the config file.

**Resource Limits (Python):**
```python
import fast_yaml

data = fast_yaml.safe_load(
    yaml_content,
    max_depth=64,
    max_alias_bytes=1024 * 1024,
    max_documents=1000,
)
```

**Resource Limits (Node.js):** `processFiles` and `formatFiles` accept `maxDepth`, `maxAliasBytes`, `maxScanAhead` and `maxDocuments`; `LintConfig` and `BatchConfig` accept `maxInputBytes`.

**Untrusted Input:**
- Always use `safe_load()` for untrusted YAML (not `load()`)
- Validate YAML structure against expected schema
- Keep the built-in limits on and tighten them (input size, depth, alias bytes, documents)
- Bound wall-clock time in the caller; the limits cap memory and stack, not CPU time
- Run in sandboxed environments for untrusted sources

### For Contributors

**Security Tooling:**

All contributors must run security checks before submitting PRs:

**Rust dependencies:**
```bash
# Check for known vulnerabilities
cargo audit

# Comprehensive dependency check
cargo deny check

# Check only security advisories
cargo deny check advisories

# Check license compliance
cargo deny check licenses
```

**NodeJS dependencies:**
```bash
cd nodejs

# Check for vulnerabilities
pnpm audit

# Fail on high/critical only
pnpm audit --audit-level=high
```

**Python dependencies:**
```bash
cd python

# Check with pip-audit (if available)
uv pip install pip-audit
uv run pip-audit
```

### Code Review Focus Areas

Security-critical code areas requiring extra scrutiny:

1. **FFI Boundaries:**
   - Python bindings (`python/src/`)
   - NodeJS bindings (`nodejs/src/`)
   - Memory safety across language boundaries

2. **Parser Logic:**
   - Input validation in `fast-yaml-core`
   - Resource limits (`fast_yaml_core::limits`) and recursion depth
   - Memory allocation patterns

3. **Parallel Processing:**
   - Thread safety in `fast-yaml-parallel`
   - Data race prevention
   - Resource cleanup

4. **Error Handling:**
   - Panic-free error propagation
   - No information leaks in error messages
   - Safe error recovery

## Security Features

### Current Protections

**Memory Safety:**
- Written in Rust with `unsafe_code = "deny"` (minimal unsafe limited to FFI boundaries)
- All unsafe code explicitly documented with SAFETY comments
- Core parsing and linting logic is 100% safe Rust
- No manual memory management in safe code
- Automatic bounds checking

**Input Validation:**
- YAML 1.2.2 spec compliance
- Safe schema support only (no arbitrary code execution)
- Configurable, always-on resource limits: input size, nesting depth, alias expansion, document count, parser scan-ahead
- Input that is not valid UTF-8, contains NUL, or is UTF-16/UTF-32 is rejected with an error
- File writes (`-i`, `-o`) are atomic, and `-o` refuses to overwrite an input file
- File names are escaped in terminal output so a hostile name cannot forge lines or move the cursor

**Dependency Security:**
- Automated dependency scanning with cargo-audit
- License compliance checks with cargo-deny
- Regular security updates

**Testing:**
- Fuzz targets for parse, format, lint and a differential validator (`fuzz/`), run in CI
- Security test cases in test suite
- Coverage: ≥80% for critical paths

### Known Limitations

**Large File Handling:**
- Inputs above `--max-input-bytes` (default 100 MiB) are rejected; raising the limit raises memory use accordingly
- Parser memory is roughly 190x the `--max-scan-ahead` value per input; batch runs with many workers multiply that by the worker count
- Parallel processing is available for multi-document streams and file batches

**Nested Structures and Aliases:**
- Nesting depth is capped (default 256, maximum 512; flow collections stop at 255)
- Alias expansion is budgeted per input (default 64 MiB); peak memory in parallel runs can reach workers x this budget
- A depth of 512 needs about 1 MiB of stack on the calling thread; do not run it on very small thread stacks

**CPU time:**
- There is no CPU-time limit; callers that parse hostile input should enforce a timeout

## Security Maintenance

### Dependency Updates

We monitor and update dependencies regularly:

- **Dependabot** enabled for automatic security updates
- Weekly dependency review
- Quarterly major version updates

### Vulnerability Scanning

Automated scanning in CI/CD pipeline:

`cargo deny check` runs in the `security` job of `.github/workflows/ci.yml` (actions are pinned to commit SHAs), and the fuzz targets run from `.github/workflows/fuzz.yml`.

### Security Audits

- Internal security reviews before major releases
- Community security audits welcome
- Professional security audit planned for v1.0

## Acknowledgments

We appreciate security researchers who responsibly disclose vulnerabilities:

- Security contributors will be credited in CHANGELOG.md
- Public acknowledgment after coordinated disclosure
- Recognition in security advisories

## Security Contact

For security concerns:

- **Private Reports:** Use GitHub Security Advisories or email maintainers
- **General Questions:** Open a GitHub Discussion
- **Public Issues:** Only for non-security bugs

## Additional Resources

- [OWASP YAML Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/YAML_Security_Cheat_Sheet.html)
- [Rust Security Guidelines](https://anssi-fr.github.io/rust-guide/)
- [YAML 1.2.2 Specification](https://yaml.org/spec/1.2.2/)

## License

This security policy is part of the fast-yaml project and is licensed under MIT and Apache-2.0.
