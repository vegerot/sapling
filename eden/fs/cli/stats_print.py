#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-strict


"""
Helper function to print the heading of a Stat Call.
"""

from typing import TextIO


def write_heading(heading: str, out: TextIO) -> None:
    border = "*" * len(heading)
    out.write(_center_strip_right(border, 80))
    out.write(_center_strip_right(heading, 80))
    out.write(_center_strip_right(border, 80) + "\n")


LATENCY_FORMAT_STR = "{:<12} {:^4} {:^10}  {:>10}  {:>15}  {:>10} {:>10}\n"


# Prints a record of latencies with avg, 50'th,90'th and 99'th percentile.
# pyre-fixme[2]: Parameter must be annotated.
def write_latency_record(operation: str, matrix, out: TextIO) -> None:
    border = "-" * 80
    percentile = {0: "avg", 1: "p50", 2: "p90", 3: "p99"}

    for i in range(len(percentile)):
        operation_name = ""
        if i == int(len(percentile) / 2):
            operation_name = operation
        out.write(
            LATENCY_FORMAT_STR.format(
                operation_name,
                "|",
                percentile[i],
                matrix[i][0],
                matrix[i][1],
                matrix[i][2],
                matrix[i][3],
            )
        )
    out.write(border + "\n")


# pyre-fixme[2]: Parameter must be annotated.
def write_latency_table(table, out: TextIO) -> None:
    out.write(
        LATENCY_FORMAT_STR.format(
            "SystemCall",
            "|",
            "Percentile",
            "Last Minute",
            "Last 10 Minutes",
            "Last Hour",
            "All Time",
        )
    )
    border = "-" * 80
    out.write(border + "\n")
    for key in table:
        write_latency_record(key, table[key], out)


# pyre-fixme[2]: Parameter must be annotated.
def write_table(table, heading: str, out: TextIO) -> None:
    key_width = max([len(heading)] + list(map(len, table.keys()))) + 2

    format_str = "{:<{}}{:>15}{:>15}{:>15}{:>15}\n"
    out.write(
        format_str.format(
            heading, key_width, "Last Minute", "Last 10m", "Last Hour", "All Time"
        )
    )
    border = "-" * (key_width + 60)
    out.write(border + "\n")
    for key in table:
        value = table[key]
        out.write(
            format_str.format(key, key_width, value[0], value[1], value[2], value[3])
        )


def _center_strip_right(text: str, width: int) -> str:
    """Returns a string with sufficient leading whitespace such that `text`
    would be centered within the specified `width` plus a trailing newline."""
    space = (width - len(text)) // 2
    return space * " " + text + "\n"


def format_size(size: int) -> str:
    if size > 1000000000:
        return "{:.1f} GB".format(size / 1000000000)
    if size > 1000000:
        return "{:.1f} MB".format(size / 1000000)
    if size > 1000:
        return "{:.1f} KB".format(size / 1000)
    if size > 0:
        return "{} B".format(size)
    return "0"


def format_time(time: int) -> str:
    if time >= 86400:
        return "{:.1f} day(s)".format(time / 86400)
    elif time >= 3600:
        return "{:.1f} hour(s)".format(time / 3600)
    elif time >= 60:
        return "{:.1f} minute(s)".format(time / 60)
    else:
        return "{} second(s)".format(time)
