"""Resource-limit and cyclic-structure tests (#336, #337)."""

import threading

import pytest

import fast_yaml

DEEP = "- " * 20_000 + "x"
BOMB = "a0: &a0 [x,x,x,x,x,x,x,x,x]\n" + "".join(
    f"a{i}: &a{i} [{','.join([f'*a{i - 1}'] * 9)}]\n" for i in range(1, 9)
)


STRBOMB = (
    'a0: &a0 "'
    + "x" * 1024
    + '"\n'
    + "".join(f"a{i}: &a{i} [{','.join([f'*a{i - 1}'] * 9)}]\n" for i in range(1, 7))
)


TAGBOMB = (
    "a0: &a0 !<tag:"
    + "x" * 10_000
    + '> ""\n'
    + "".join(f"a{i}: &a{i} [{','.join([f'*a{i - 1}'] * 9)}]\n" for i in range(1, 6))
)


TAGPREFIX = (
    "%TAG !e! tag:e.com,"
    + "a" * 100_000
    + "\n---\n"
    + "".join(f"k{i}: !e!x v\n" for i in range(1_000))
)


def _nested(depth):
    data = current = []
    for _ in range(depth - 1):
        nxt = []
        current.append(nxt)
        current = nxt
    return data


def _with_stack(func, size):
    """Run ``func`` on a thread with a ``size``-byte stack.

    Emitting a valid 256-deep tree recurses inside the third-party YAML emitter
    (about 1.5 KB per level in debug builds), which does not fit a 1 MB host stack.
    """
    outcome = []

    def target():
        try:
            outcome.append(func())
        except BaseException as exc:  # noqa: BLE001
            outcome.append(exc)

    previous = threading.stack_size(size)
    try:
        thread = threading.Thread(target=target)
        thread.start()
        thread.join()
    finally:
        threading.stack_size(previous)
    result = outcome[0]
    if isinstance(result, BaseException):
        raise result
    return result


def _with_big_stack(func):
    return _with_stack(func, 64 * 1024 * 1024)


def _self_list():
    a = []
    a.append(a)
    return a


def _self_dict():
    d = {}
    d["x"] = d
    return d


@pytest.mark.parametrize(
    "text",
    [DEEP, BOMB, STRBOMB, TAGBOMB, TAGPREFIX],
    ids=["deep", "bomb", "strbomb", "tagbomb", "tagprefix"],
)
def test_safe_load_rejects_hostile_input(text):
    with pytest.raises(ValueError, match="limit exceeded"):
        fast_yaml.safe_load(text)


@pytest.mark.parametrize(
    "text",
    [DEEP, BOMB, STRBOMB, TAGBOMB, TAGPREFIX],
    ids=["deep", "bomb", "strbomb", "tagbomb", "tagprefix"],
)
def test_safe_load_all_rejects_hostile_input(text):
    with pytest.raises(ValueError, match="limit exceeded"):
        list(fast_yaml.safe_load_all(text))


def test_cross_document_alias_is_rejected():
    with pytest.raises(ValueError, match="unknown anchor"):
        list(fast_yaml.safe_load_all("--- &a [x]\n--- *a\n"))


@pytest.mark.parametrize("make", [_self_list, _self_dict], ids=["list", "dict"])
@pytest.mark.parametrize(
    "dump",
    [
        fast_yaml.safe_dump,
        lambda obj: fast_yaml.safe_dump_all([obj]),
        fast_yaml.dump,
    ],
    ids=["safe_dump", "safe_dump_all", "dump"],
)
def test_dump_rejects_cycles(make, dump):
    with pytest.raises(ValueError, match="circular reference"):
        dump(make())


def test_deep_list_round_trips():
    data = current = []
    for _ in range(199):
        nxt = []
        current.append(nxt)
        current = nxt
    current.append("leaf")
    assert _with_big_stack(lambda: fast_yaml.safe_load(fast_yaml.safe_dump(data))) == data


def test_shared_aliases_still_load():
    data = fast_yaml.safe_load("a: &x [1, 2]\nb: *x\n")
    assert data == {"a": [1, 2], "b": [1, 2]}
    assert data["a"] is data["b"]


def test_dump_depth_boundary():
    assert _with_big_stack(
        lambda: fast_yaml.safe_load(fast_yaml.safe_dump(_nested(256)))
    ) == _nested(256)
    with pytest.raises(ValueError, match="circular reference"):
        fast_yaml.safe_dump(_nested(257))


def test_conversion_does_not_depend_on_host_stack():
    deep_map = "".join(" " * i + "k:\n" for i in range(255)) + " " * 255 + "x"

    def work():
        with pytest.raises(ValueError, match="circular reference"):
            fast_yaml.safe_dump(_self_list())
        with pytest.raises(ValueError, match="limit exceeded"):
            fast_yaml.safe_load(DEEP)
        assert fast_yaml.safe_load(deep_map) is not None

    _with_stack(work, 512 * 1024)
