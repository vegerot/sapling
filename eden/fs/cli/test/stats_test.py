#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-strict

import unittest
from io import StringIO
from typing import Optional

from .. import stats_print
from ..stats import DiagInfoCounters, get_counter_table, get_store_latency


class StatsTest(unittest.TestCase):
    maxDiff: Optional[int] = None

    def test_print_heading(self) -> None:
        expected_output = """\
                                   **********
                                   TheHeading
                                   **********

"""
        out = StringIO()
        stats_print.write_heading("TheHeading", out)
        self.assertEqual(out.getvalue(), expected_output)

    def test_print_latency_record(self) -> None:
        matrix = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12], [13, 14, 15, 16]]
        expected_output = """\
              |      avg               1                2           3          4
              |      p50               5                6           7          8
access        |      p90               9               10          11         12
              |      p99              13               14          15         16
--------------------------------------------------------------------------------
"""

        out = StringIO()
        stats_print.write_latency_record("access", matrix, out)
        self.assertEqual(out.getvalue(), expected_output)

    def test_print_table(self) -> None:
        table = {
            "key1": [1, 2, 3, 4],
            "key2": [5, 6, 7, 8],
            "key3": [9, 10, 11, 12],
            "key4": [13, 14, 15, 16],
        }
        expected_output = """\
SystemCall      Last Minute       Last 10m      Last Hour       All Time
------------------------------------------------------------------------
key1                      1              2              3              4
key2                      5              6              7              8
key3                      9             10             11             12
key4                     13             14             15             16
"""
        out = StringIO()
        stats_print.write_table(table, "SystemCall", out)
        self.assertEqual(expected_output, out.getvalue())

    def test_print_table_with_shorter_header_and_key_column(self) -> None:
        table = {"key": [1, 2, 3, 4]}
        # Verifies the width of the first column depends on the header's and
        # key's lengths.
        expected_output = """\
SC       Last Minute       Last 10m      Last Hour       All Time
-----------------------------------------------------------------
key                1              2              3              4
"""
        out = StringIO()
        stats_print.write_table(table, "SC", out)
        self.assertEqual(expected_output, out.getvalue())

    def test_format_size(self) -> None:
        self.assertEqual("1.5 GB", stats_print.format_size(1500000000))
        # rounds up
        self.assertEqual("1.6 GB", stats_print.format_size(1590000000))
        self.assertEqual("123.4 MB", stats_print.format_size(123400000))
        self.assertEqual("12 B", stats_print.format_size(12))
        self.assertEqual("0", stats_print.format_size(0))

    def test_time_formats_correctly(self) -> None:
        self.assertEqual(stats_print.format_time(0), "0 second(s)")
        self.assertEqual(stats_print.format_time(30), "30 second(s)")
        self.assertEqual(stats_print.format_time(60), "1.0 minute(s)")
        self.assertEqual(stats_print.format_time(90), "1.5 minute(s)")
        self.assertEqual(stats_print.format_time(120), "2.0 minute(s)")
        self.assertEqual(stats_print.format_time(60 * 60), "1.0 hour(s)")
        self.assertEqual(stats_print.format_time(60 * 30 * 5), "2.5 hour(s)")
        self.assertEqual(stats_print.format_time(60 * 60 * 3), "3.0 hour(s)")
        self.assertEqual(stats_print.format_time(60 * 60 * 23), "23.0 hour(s)")
        self.assertEqual(stats_print.format_time(60 * 60 * 24), "1.0 day(s)")
        self.assertEqual(stats_print.format_time(60 * 60 * 32), "1.3 day(s)")


class SaplingBackingStoreStatsTest(unittest.TestCase):
    def test_get_stats_from_right_store(self) -> None:
        counters: DiagInfoCounters = {
            "store.mononoke.get_blob.p50": 10,
            "store.mononoke.get_blob.p50.60": 20,
            "store.mononoke.get_blob.p50.600": 30,
            "store.mononoke.get_blob.p50.3600": 40,
            "store.sapling.get_blob.p50": 40,
            "store.sapling.get_blob.p50.60": 30,
            "store.sapling.get_blob.p50.600": 20,
            "store.sapling.get_blob.p50.3600": 10,
        }
        table = get_store_latency(counters, "mononoke")
        result = table.get("get_blob")
        if result:
            self.assertEqual(result[1], ["20 μs", "30 μs", "40 μs", "10 μs"])
        else:
            # make pyre happy
            self.assertTrue(False, "should return result")

    def test_get_store_latency_correctly(self) -> None:
        counters: DiagInfoCounters = {
            "store.mononoke.get_blob.count": 10,
            "store.mononoke.get_blob.count.60": 20,
            "store.mononoke.get_blob.count.600": 30,
            "store.mononoke.get_blob.count.3600": 40,
            "store.mononoke.get_blob.p50": 5010,
            "store.mononoke.get_blob.p50.60": 5020,
            "store.mononoke.get_blob.p50.600": 5030,
            "store.mononoke.get_blob.p50.3600": 5040,
            "store.mononoke.get_blob.p90": 9010,
            "store.mononoke.get_blob.p90.60": 9020,
            "store.mononoke.get_blob.p90.600": 9030,
            "store.mononoke.get_blob.p90.3600": 9040,
            "store.mononoke.get_blob.p99": 9910,
            "store.mononoke.get_blob.p99.60": 9920,
            "store.mononoke.get_blob.p99.600": 9930,
            "store.mononoke.get_blob.p99.3600": 9940,
        }
        table = get_store_latency(counters, "mononoke")
        self.assertEqual(
            table.get("get_blob"),
            [
                ["", "", "", ""],
                ["5020 μs", "5030 μs", "5040 μs", "5010 μs"],
                ["9020 μs", "9030 μs", "9040 μs", "9010 μs"],
                ["9920 μs", "9930 μs", "9940 μs", "9910 μs"],
            ],
        )
