#!/usr/bin/env python3
"""
Benchmark script comparing fast-yaml with PyYAML.

Usage:
    pip install pyyaml
    python benches/benchmark.py
"""

import timeit
import json
from io import StringIO

# Test data
SMALL_YAML = """
name: test
value: 123
active: true
"""

MEDIUM_YAML = """
database:
  host: localhost
  port: 5432
  name: myapp
  credentials:
    username: admin
    password: secret

servers:
  - name: web1
    ip: 192.168.1.1
    roles:
      - web
      - api
  - name: web2
    ip: 192.168.1.2
    roles:
      - web
  - name: db1
    ip: 192.168.1.3
    roles:
      - database
      - backup

settings:
  debug: false
  log_level: info
  features:
    - authentication
    - caching
    - monitoring
"""

# Generate large YAML
def generate_large_yaml(num_items=1000):
    items = []
    for i in range(num_items):
        items.append({
            "id": i,
            "name": f"item_{i}",
            "description": f"This is item number {i} with a longer description",
            "active": i % 2 == 0,
            "score": i * 1.5,
            "tags": [f"tag_{j}" for j in range(5)],
            "metadata": {
                "created": "2024-01-01",
                "updated": "2024-12-01",
                "version": i % 10
            }
        })
    
    # Convert to YAML manually for consistency
    import yaml
    return yaml.safe_dump({"items": items})


def benchmark_load(library, yaml_str, iterations=1000):
    """Benchmark YAML loading."""
    if library == "pyyaml":
        import yaml
        func = lambda: yaml.safe_load(yaml_str)
    elif library == "fast_yaml":
        import fast_yaml
        func = lambda: fast_yaml.safe_load(yaml_str)
    else:
        raise ValueError(f"Unknown library: {library}")
    
    time = timeit.timeit(func, number=iterations)
    return time / iterations * 1000  # Convert to milliseconds


def benchmark_dump(library, data, iterations=1000):
    """Benchmark YAML dumping."""
    if library == "pyyaml":
        import yaml
        func = lambda: yaml.safe_dump(data)
    elif library == "fast_yaml":
        import fast_yaml
        func = lambda: fast_yaml.safe_dump(data)
    else:
        raise ValueError(f"Unknown library: {library}")
    
    time = timeit.timeit(func, number=iterations)
    return time / iterations * 1000  # Convert to milliseconds


def run_benchmarks():
    print("=" * 70)
    print("fast-yaml vs PyYAML Benchmark")
    print("=" * 70)
    print()
    
    # Check if libraries are available
    libraries = []
    
    try:
        import yaml
        libraries.append("pyyaml")
        print(f"✓ PyYAML version: {yaml.__version__}")
        
        # Check if libyaml is available
        try:
            from yaml import CSafeLoader
            print("  (with libyaml C extension)")
        except ImportError:
            print("  (pure Python, no libyaml)")
    except ImportError:
        print("✗ PyYAML not installed")
    
    try:
        import fast_yaml
        libraries.append("fast_yaml")
        print(f"✓ fast-yaml version: {fast_yaml.__version__}")
    except ImportError:
        print("✗ fast-yaml not installed")
    
    if len(libraries) < 2:
        print("\nNeed both libraries to run comparison benchmarks.")
        return
    
    print()
    
    # Prepare test data
    import yaml
    small_data = yaml.safe_load(SMALL_YAML)
    medium_data = yaml.safe_load(MEDIUM_YAML)
    
    large_yaml = generate_large_yaml(1000)
    large_data = yaml.safe_load(large_yaml)
    
    test_cases = [
        ("Small (~50 bytes)", SMALL_YAML, small_data, 10000),
        ("Medium (~500 bytes)", MEDIUM_YAML, medium_data, 5000),
        ("Large (~500 KB)", large_yaml, large_data, 100),
    ]
    
    # Run load benchmarks
    print("-" * 70)
    print("LOAD Benchmarks (lower is better)")
    print("-" * 70)
    print(f"{'Test Case':<20} {'PyYAML (ms)':<15} {'fast-yaml (ms)':<15} {'Speedup':<10}")
    print("-" * 70)
    
    for name, yaml_str, data, iterations in test_cases:
        pyyaml_time = benchmark_load("pyyaml", yaml_str, iterations)
        fast_yaml_time = benchmark_load("fast_yaml", yaml_str, iterations)
        speedup = pyyaml_time / fast_yaml_time
        
        print(f"{name:<20} {pyyaml_time:<15.4f} {fast_yaml_time:<15.4f} {speedup:<10.2f}x")
    
    print()
    
    # Run dump benchmarks
    print("-" * 70)
    print("DUMP Benchmarks (lower is better)")
    print("-" * 70)
    print(f"{'Test Case':<20} {'PyYAML (ms)':<15} {'fast-yaml (ms)':<15} {'Speedup':<10}")
    print("-" * 70)
    
    for name, yaml_str, data, iterations in test_cases:
        pyyaml_time = benchmark_dump("pyyaml", data, iterations)
        fast_yaml_time = benchmark_dump("fast_yaml", data, iterations)
        speedup = pyyaml_time / fast_yaml_time
        
        print(f"{name:<20} {pyyaml_time:<15.4f} {fast_yaml_time:<15.4f} {speedup:<10.2f}x")
    
    print()
    print("=" * 70)
    print("Benchmark complete!")


if __name__ == "__main__":
    run_benchmarks()
