from __future__ import absolute_import, print_function

from sapling import pycompat, ui, util, wireproto


stringio = util.stringio


class proto:
    def __init__(self, args):
        self.args = args

    def getargs(self, spec):
        args = self.args
        args.setdefault("*", {})
        names = spec.split()
        return [args[n] for n in names]


class clientpeer(wireproto.wirepeer):
    def __init__(self, serverrepo):
        self.serverrepo = serverrepo

    @property
    def ui(self):
        return self.serverrepo.ui

    def url(self):
        return "test"

    def local(self):
        return None

    def peer(self):
        return self

    def canpush(self):
        return True

    def close(self):
        pass

    def capabilities(self):
        return ["batch"]

    def _call(self, cmd, **args):
        res = wireproto.dispatch(self.serverrepo, proto(args), cmd)
        if isinstance(res, str):
            res = pycompat.encodeutf8(res)
        return res

    def _callstream(self, cmd, **args):
        return stringio(self._call(cmd, **args))

    @wireproto.batchable
    def greet(self, name):
        f = wireproto.future()
        yield {"name": mangle(name)}, f
        yield unmangle(pycompat.decodeutf8(f.value))


class serverrepo:
    def __init__(self):
        self.ui = ui.ui()

    def greet(self, name):
        return "Hello, " + name

    def filtered(self, name):
        return self


def mangle(s):
    return "".join(chr(ord(c) + 1) for c in s)


def unmangle(s):
    return "".join(chr(ord(c) - 1) for c in s)


def greet(repo, proto, name):
    return mangle(repo.greet(unmangle(name)))


wireproto.commands["greet"] = (greet, "name")

srv = serverrepo()
clt = clientpeer(srv)

print(clt.greet("Foobar"))
b = clt.iterbatch()
list(map(b.greet, ("Fo, =;:<o", "Bar")))
b.submit()
print([r for r in b.results()])
