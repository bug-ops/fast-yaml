# Security Policy

## Supported Versions

We release security updates for the following versions:

| Version | Supported          |
| ------- | ------------------ |
| 0.4.x   | :white_check_mark: |
| < 0.4   | :x:                |

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
- fast-yaml-core v0.4.0
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
- Rust: 1.88.0
- fast-yaml: 0.4.0
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

**Input Validation:**
```rust
// Always validate input size before parsing
const MAX_INPUT_SIZE: usize = 10 * 1024 * 1024; // 10 MB

if input.len() > MAX_INPUT_SIZE {
    return Err("Input too large");
}

fast_yaml_core::parse(input)?;
```

**Resource Limits:**
```python
import fast_yaml

# Set reasonable limits for production
yaml_content = open('data.yaml').read()
if len(yaml_content) > 10 * 1024 * 1024:  # 10 MB
    raise ValueError("YAML file too large")

data = fast_yaml.safe_load(yaml_content)
```

**Untrusted Input:**
- Always use `safe_load()` for untrusted YAML (not `load()`)
- Validate YAML structure against expected schema
- Set resource limits (file size, parsing time)
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
npm audit

# Fail on high/critical only
npm audit --audit-level=high

# Auto-fix when possible
npm audit fix

# Generate detailed report
npm audit --json > audit-report.json
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
   - Recursive parsing depth limits
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
- Written in Rust with `unsafe_code = "forbid"` (zero unsafe blocks)
- No manual memory management
- Automatic bounds checking

**Input Validation:**
- YAML 1.2.2 spec compliance
- Safe schema support only (no arbitrary code execution)
- Configurable resource limits

**Dependency Security:**
- Automated dependency scanning with cargo-audit
- License compliance checks with cargo-deny
- Regular security updates

**Testing:**
- Fuzzing for parser robustness (planned)
- Security test cases in test suite
- Coverage: ≥80% for critical paths

### Known Limitations

**Large File Handling:**
- Very large YAML files (>1GB) may cause high memory usage
- Recommendation: Use streaming/chunking for large files
- Parallel processing available for multi-document streams

**Nested Structures:**
- Deep nesting (>100 levels) may impact performance
- Stack overflow protection in place
- Configurable recursion limits (future feature)

## Security Maintenance

### Dependency Updates

We monitor and update dependencies regularly:

- **Dependabot** enabled for automatic security updates
- Weekly dependency review
- Quarterly major version updates

### Vulnerability Scanning

Automated scanning in CI/CD pipeline:

```yaml
# .github/workflows/ci.yml
security:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: taiki-e/install-action@cargo-deny
    - run: cargo deny check
```

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
