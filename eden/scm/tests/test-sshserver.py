from __future__ import absolute_import, print_function

import io
import unittest

import silenttestrunner
from sapling import sshserver, wireproto


class SSHServerGetArgsTests(unittest.TestCase):
    def testparseknown(self):
        tests = [
            (b"* 0\nnodes 0\n", ["", {}]),
            (
                b"* 0\nnodes 40\n1111111111111111111111111111111111111111\n",
                ["1111111111111111111111111111111111111111", {}],
            ),
        ]
        for input, expected in tests:
            self.assertparse("known", input, expected)

    def assertparse(self, cmd, input, expected):
        server = mockserver(input)
        _func, spec = wireproto.commands[cmd]
        self.assertEqual(server.getargs(spec), expected)


def mockserver(inbytes):
    ui = mockui(inbytes)
    repo = mockrepo(ui)
    return sshserver.sshserver(ui, repo)


class mockrepo:
    def __init__(self, ui):
        self.ui = ui


class mockui:
    def __init__(self, inbytes):
        self.fin = io.BytesIO(inbytes)
        self.fout = io.BytesIO()
        self.ferr = io.BytesIO()


if __name__ == "__main__":
    silenttestrunner.main(__name__)
