#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-strict

import argparse

from . import cmd_util, subcmd as subcmd_mod
from .subcmd import Subcmd
from .trace_cmd import trace_cmd


@trace_cmd("enable", "Enable tracing")
class EnableTraceCmd(Subcmd):
    def run(self, args: argparse.Namespace) -> int:
        instance = cmd_util.get_eden_instance(args)
        with instance.get_thrift_client_legacy() as client:
            client.enableTracing()
        return 0


@trace_cmd("disable", "Disable tracing")
class DisableTraceCmd(Subcmd):
    def run(self, args: argparse.Namespace) -> int:
        instance = cmd_util.get_eden_instance(args)
        with instance.get_thrift_client_legacy() as client:
            client.disableTracing()
        return 0


@subcmd_mod.subcmd("trace", "Commands for managing EdenFS tracing")
# pyre-fixme[13]: Attribute `parser` is never initialized.
class TraceCmd(Subcmd):
    parser: argparse.ArgumentParser

    def setup_parser(self, parser: argparse.ArgumentParser) -> None:
        self.parser = parser
        self.add_subcommands(parser, trace_cmd.commands)

    def run(self, args: argparse.Namespace) -> int:
        self.parser.print_help()
        return 0
