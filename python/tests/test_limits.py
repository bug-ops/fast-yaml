"""Resource-limit and cyclic-structure tests (#336, #337)."""

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


def _nested(depth):
    data = current = []
    for _ in range(depth - 1):
        nxt = []
        current.append(nxt)
        current = nxt
    return data


def _self_list():
    a = []
    a.append(a)
    return a


def _self_dict():
    d = {}
    d["x"] = d
    return d


@pytest.mark.parametrize(
    "text", [DEEP, BOMB, STRBOMB, TAGBOMB], ids=["deep", "bomb", "strbomb", "tagbomb"]
)
def test_safe_load_rejects_hostile_input(text):
    with pytest.raises(ValueError, match="limit exceeded"):
        fast_yaml.safe_load(text)


@pytest.mark.parametrize(
    "text", [DEEP, BOMB, STRBOMB, TAGBOMB], ids=["deep", "bomb", "strbomb", "tagbomb"]
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
    assert fast_yaml.safe_load(fast_yaml.safe_dump(data)) == data


def test_shared_aliases_still_load():
    data = fast_yaml.safe_load("a: &x [1, 2]\nb: *x\n")
    assert data == {"a": [1, 2], "b": [1, 2]}
    assert data["a"] is data["b"]


def test_dump_depth_boundary():
    assert fast_yaml.safe_load(fast_yaml.safe_dump(_nested(256))) == _nested(256)
    with pytest.raises(ValueError, match="circular reference"):
        fast_yaml.safe_dump(_nested(257))
