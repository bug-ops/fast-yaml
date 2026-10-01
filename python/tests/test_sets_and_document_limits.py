"""Document limits (#529), merge errors (#518), `!!set` (#490), limit names (#508)."""

import pytest

import fast_yaml
from fast_yaml import parallel
from fast_yaml._core import batch
from fast_yaml._core import parallel as core_parallel

COMPLEX_KEY = "YAML complex keys"


class TestMaxDocuments:
    def test_limit_hit_with_config(self):
        config = parallel.ParallelConfig(max_documents=2)
        with pytest.raises(ValueError, match="document count exceeds 2"):
            parallel.parse_parallel("---\na\n" * 3, config)

    def test_default_applies_without_config(self):
        with pytest.raises(ValueError, match="document count exceeds 100000"):
            parallel.parse_parallel("---\na\n" * 100_001)

    def test_builder_sets_limit(self):
        config = parallel.ParallelConfig().with_max_documents(1)
        with pytest.raises(ValueError, match="document count exceeds 1"):
            parallel.parse_parallel("---\na\n---\nb\n", config)

    def test_at_limit_is_accepted(self):
        config = parallel.ParallelConfig(max_documents=3)
        assert parallel.parse_parallel("---\na\n" * 3, config) == ["a", "a", "a"]

    def test_dump_parallel_enforces_limit(self):
        config = parallel.ParallelConfig(max_documents=2)
        with pytest.raises(ValueError, match="cannot serialize to YAML: document count exceeds 2"):
            core_parallel.dump_parallel([1, 2, 3], config)

    def test_safe_load_all_limit(self):
        with pytest.raises(ValueError, match=r"document count exceeds 3 \(document 4\)"):
            fast_yaml.safe_load_all("---\na\n" * 5, max_documents=3)

    def test_safe_load_all_at_limit(self):
        assert list(fast_yaml.safe_load_all("---\na\n" * 3, max_documents=3)) == ["a"] * 3

    def test_safe_load_default_limit(self):
        with pytest.raises(ValueError, match="document count exceeds 100000"):
            fast_yaml.safe_load("---\n" * 100_001)

    @pytest.mark.parametrize("loader", [fast_yaml.load, fast_yaml.load_all])
    def test_load_limit(self, loader):
        with pytest.raises(ValueError, match="document count exceeds 1"):
            loader("---\na\n---\nb\n", max_documents=1)

    def test_lint_and_batch_builders(self):
        from fast_yaml import lint

        assert lint.LintConfig(max_documents=2).with_max_documents(None) is not None
        assert batch.BatchConfig(max_documents=2).with_max_documents(5) is not None
        with pytest.raises(ValueError, match="max_documents must be between"):
            batch.BatchConfig(max_documents=0)

    def test_batch_process_files_limit(self, tmp_path):
        path = tmp_path / "many.yaml"
        path.write_text("---\na\n" * 5)
        result = batch.process_files([str(path)], batch.BatchConfig(max_documents=3))
        assert result.failed == 1
        assert "document count exceeds 3" in result.errors()[0][1]

    @pytest.mark.parametrize("bad", [0, -1, 10_000_001])
    def test_out_of_range(self, bad):
        with pytest.raises(ValueError, match="max_documents must be between 1 and 10000000"):
            parallel.ParallelConfig(max_documents=bad)

    def test_bool_rejected(self):
        with pytest.raises(TypeError):
            parallel.ParallelConfig(max_documents=True)


class TestMaxInputBytes:
    @pytest.mark.parametrize("bad", [0, 2**31])
    def test_parallel_range(self, bad):
        with pytest.raises(ValueError, match="max_input_bytes must be between 1 and 1073741824"):
            parallel.ParallelConfig(max_input_bytes=bad)

    def test_parallel_bool_rejected(self):
        with pytest.raises(TypeError):
            parallel.ParallelConfig(max_input_bytes=True)

    def test_parallel_limit_enforced(self):
        config = parallel.ParallelConfig(max_input_bytes=4)
        with pytest.raises(ValueError, match="exceeds maximum allowed 4 bytes"):
            parallel.parse_parallel("a: 1234567890\n", config)

    def test_old_name_is_gone(self):
        with pytest.raises(TypeError):
            parallel.ParallelConfig(max_input_size=10)  # type: ignore[call-arg]
        with pytest.raises(TypeError):
            batch.BatchConfig(max_input_size=10)  # type: ignore[call-arg]
        assert not hasattr(parallel.ParallelConfig(), "with_max_input_size")

    def test_batch_rejects_zero(self):
        with pytest.raises(ValueError, match="max_input_bytes must be between 1 and"):
            batch.BatchConfig(max_input_bytes=0)


class TestMergeErrorOrder:
    DIVERGENT = "m: {<<: 1, x: {<<: !!set {a}}}\n"

    def test_first_invalid_merge_wins_in_both_loaders(self):
        for load in (fast_yaml.safe_load, parallel.parse_parallel):
            with pytest.raises(ValueError, match=r"line 1, column 5$"):
                load(self.DIVERGENT)

    def test_document_index_is_reported(self):
        doc = "a: 1\n---\nb: 2\n---\nm:\n  <<: 1\n"
        for load in (fast_yaml.safe_load_all, parallel.parse_parallel):
            with pytest.raises(ValueError, match=r"\(document 3\)"):
                load(doc)

    def test_repeated_merge_key_is_rejected(self):
        doc = "{<<: [{1: a}, {true: b}], <<: {c: 1}}"
        with pytest.raises(ValueError, match="duplicate merge key"):
            fast_yaml.safe_load(doc)
        with pytest.raises(ValueError, match="duplicate merge key"):
            parallel.parse_parallel(doc)

    def test_aliased_merge_key_is_validated(self):
        with pytest.raises(ValueError, match="merge key"):
            fast_yaml.safe_load("a: {&k <<: {x: 1}}\nb: {*k : 1}\n")


class TestSets:
    def test_parse_parallel_returns_set(self):
        assert parallel.parse_parallel("!!set {a, b}") == [{"a", "b"}]
        assert isinstance(parallel.parse_parallel("!!set {a}")[0], set)

    def test_nested_set(self):
        assert parallel.parse_parallel("m: !!set {a}\n") == [{"m": {"a"}}]

    def test_block_set(self):
        assert parallel.parse_parallel("s: !!set\n  ? a\n  ? b\n") == [{"s": {"a", "b"}}]

    def test_numeric_clash_raises(self):
        with pytest.raises(ValueError, match="distinct in YAML"):
            parallel.parse_parallel("!!set {1, true}")

    @pytest.mark.parametrize("value", [{1, 2}, frozenset({"a", "b"}), {"k": {"x", "y"}}])
    def test_dump_round_trip(self, value):
        text = fast_yaml.safe_dump(value)
        assert "!!set" in text
        assert fast_yaml.safe_load(text) == value
        assert parallel.parse_parallel(text) == [value]

    def test_frozenset_loads_as_set(self):
        assert fast_yaml.safe_load(fast_yaml.safe_dump(frozenset({1}))) == {1}

    def test_dump_parallel_writes_set_tag(self):
        text = core_parallel.dump_parallel([{1, 2}, {3}])
        assert text.count("!!set") == 2

    def test_sort_keys_keeps_set_tag(self):
        text = fast_yaml.safe_dump({"b": {2, 1}, "a": 1}, sort_keys=True)
        assert fast_yaml.safe_load(text) == {"a": 1, "b": {1, 2}}

    @pytest.mark.parametrize(
        "doc", ["? !!set {a}: 1\n", "{? !!set {a} : 1}\n", "? !!set {a}\n: 1\n"]
    )
    def test_set_as_key_is_complex_key(self, doc):
        with pytest.raises(ValueError, match=COMPLEX_KEY):
            fast_yaml.safe_load(doc)
        with pytest.raises(ValueError, match=COMPLEX_KEY):
            parallel.parse_parallel(doc)

    @pytest.mark.parametrize(
        "doc", ["!!set {? !!set {a}}\n", "!!set {? [a]}\n", "!!set {? {a: 1}}\n"]
    )
    def test_collection_member_is_complex_key(self, doc):
        with pytest.raises(ValueError, match=COMPLEX_KEY):
            fast_yaml.safe_load(doc)
        with pytest.raises(ValueError, match=COMPLEX_KEY):
            parallel.parse_parallel(doc)

    @pytest.mark.parametrize(
        "doc", ["!!set {a: 1, b: 2}\n", "!!set {a, b: [x]}\n", "k: !!set\n  a: {b}\n"]
    )
    def test_set_member_with_a_value_is_rejected(self, doc):
        match = r"!!set member has a non-null value.* at line \d+, column \d+"
        with pytest.raises(ValueError, match=match):
            fast_yaml.safe_load(doc)
        with pytest.raises(ValueError, match=match):
            parallel.parse_parallel(doc)

    def test_set_value_error_position(self):
        with pytest.raises(ValueError, match=r"line 1, column 8"):
            fast_yaml.safe_load("!!set {a: 1}")
        with pytest.raises(ValueError, match=r"line 4, column 3 \(document 2\)"):
            fast_yaml.safe_load_all("a: 1\n---\n!!set\n  x: 1\n")

    def test_null_set_members_are_accepted(self):
        assert fast_yaml.safe_load("!!set {a, b: , c: ~, d: null}\n") == {"a", "b", "c", "d"}
        assert parallel.parse_parallel("!!set {a, b: ~}\n") == [{"a", "b"}]

    def test_tagged_sequence_stays_a_list(self):
        assert fast_yaml.safe_load("!!set [a, b]\n") == ["a", "b"]
        assert parallel.parse_parallel("!!set [a, b]\n") == [["a", "b"]]

    def test_duplicate_members_collapse(self):
        assert fast_yaml.safe_load("!!set {a, a}\n") == {"a"}
        assert parallel.parse_parallel("!!set {a, a}\n") == [{"a"}]

    @pytest.mark.parametrize("empty", [set(), frozenset()])
    def test_empty_set_round_trip(self, empty):
        text = fast_yaml.safe_dump(empty)
        assert "!!set" in text
        assert fast_yaml.safe_load(text) == set()
        assert parallel.parse_parallel(text) == [set()]


def test_safe_dump_sorts_set_members_with_sort_keys():
    dumped = fast_yaml.safe_dump({"c", "a", "b"}, sort_keys=True)
    assert dumped.index("a") < dumped.index("b") < dumped.index("c")
    assert fast_yaml.safe_load(dumped) == {"a", "b", "c"}


def test_set_as_a_mapping_key_cannot_be_dumped():
    with pytest.raises(ValueError, match="cannot be a mapping key"):
        fast_yaml.safe_dump({frozenset({"a"}): 1})
