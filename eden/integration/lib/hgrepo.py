#!/usr/bin/env python3
# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# pyre-unsafe

import configparser
import datetime
import json
import os
import subprocess
import sys
import textwrap
import typing
from pathlib import Path
from typing import Any, Dict, List, Optional

from eden.test_support.temporary_directory import TempFileManager

from . import repobase
from .error import CommandError
from .find_executables import FindExe


class HgError(CommandError):
    pass


class HgRepository(repobase.Repository):
    hg_bin: str
    hg_environment: Dict[str, str]
    temp_mgr: TempFileManager
    staged_files: List[str]
    filtered: bool = False
    eagerepo: Optional[Path] = None

    def __init__(
        self,
        path: str,
        system_hgrc: Optional[str] = None,
        temp_mgr: Optional[TempFileManager] = None,
        filtered: bool = False,
    ) -> None:
        """
        If hgrc is specified, it will be used as the value of the HGRCPATH
        environment variable when `hg` is run.
        """
        super().__init__(path)
        self.temp_mgr = temp_mgr or TempFileManager()
        self.hg_environment = os.environ.copy()
        # Drop any environment variables starting with 'HG'
        # to ensure the user's environment does not affect the tests.
        self.hg_environment = {
            k: v for k, v in os.environ.items() if not k.startswith("HG")
        }
        self.hg_environment["HGPLAIN"] = "1"
        self.hg_environment["HG_REAL_BIN"] = FindExe.HG_REAL
        self.hg_environment["NOSCMLOG"] = "1"
        self.hg_environment["LOCALE"] = "en_US.UTF-8"
        self.hg_environment["LC_ALL"] = "en_US.UTF-8"
        # Set HGRCPATH to make sure we aren't affected by the local system's
        # mercurial settings from /etc/mercurial/
        if system_hgrc:
            self.hg_environment["HGRCPATH"] = system_hgrc
        else:
            self.hg_environment["HGRCPATH"] = ""
        self.hg_bin = FindExe.HG
        self.staged_files = []
        self.filtered = filtered

    @classmethod
    def get_system_hgrc_contents(cls) -> str:
        common_suffix = textwrap.dedent(
            """
            # Override ui.merge to make sure it does not get set
            # to something that tries to prompt for user input.
            [ui]
            merge = :merge
            """
        )

        hgrc_dir = cls.find_hgrc_dir_from_repo()
        if hgrc_dir is not None:
            if sys.platform == "win32":
                platform_rc = "windows.rc"
            else:
                platform_rc = "posix.rc"
            return (
                textwrap.dedent(
                    f"""
                %include {hgrc_dir}/facebook.rc
                %include {hgrc_dir}/tier-specific/{platform_rc}
                %include {hgrc_dir}/tier-specific/client.rc
                """
                )
                + common_suffix
            )

        # If we could not find configs in the repository, just use the default
        # configuration installed on the system.  This is somewhat less than ideal,
        # since it means the test behavior will depend on the currently installed
        # Mercurial version, but it's the best we can do.
        if sys.platform == "win32":
            system_hgrc = os.path.join(
                os.environ["PROGRAMDATA"], "Facebook", "Mercurial", "system.rc"
            )
            whoami_path = os.path.join("C:", "etc", "fbwhoami")
        else:
            system_hgrc = "/etc/mercurial/system.rc"
            whoami_path = "/etc/fbwhoami"

        if os.path.exists(whoami_path) and not os.path.exists(system_hgrc):
            # Only error on FB machines,  can't expect system mercurial elsewhere
            raise Exception("unable to find the Mercurial system config file")

        if os.path.exists(system_hgrc):
            return "%include {system_hgrc}\n" + common_suffix
        else:
            return common_suffix

    @classmethod
    def find_hgrc_dir_from_repo(cls) -> Optional[Path]:
        hgrc_dir = Path(FindExe.HG_RC_DIR)
        facebook_rc = hgrc_dir / "facebook.rc"
        if os.path.exists(facebook_rc):
            return hgrc_dir

        if FindExe.is_buck_build():
            # Buck-based builds should always find the hgrc files above.
            # Only external CMake-based builds won't find the above config files.
            raise Exception(
                "unable to find Mercurial config files in the repository (looked at {})".format(
                    hgrc_dir
                )
            )

        return None

    def run_hg(
        self,
        *args: str,
        encoding: str = "utf-8",
        stdout: Any = subprocess.PIPE,
        stderr: Any = subprocess.PIPE,
        input: Optional[str] = None,
        hgeditor: Optional[str] = None,
        cwd: Optional[str] = None,
        check: bool = True,
        traceback: bool = True,
        env: Optional[Dict[str, str]] = None,
    ) -> subprocess.CompletedProcess:
        env = self.hg_environment | (env or {})
        argslist = list(args)
        cmd = [self.hg_bin] + (["--traceback"] if traceback else []) + argslist
        print(f"Trying to run {cmd}")
        if hgeditor is not None:
            env["HGEDITOR"] = hgeditor

        # Create a temporary file for the input as a more reliable way than the PIPE
        # and Python threads.
        input_file = None
        stdin = subprocess.DEVNULL
        if input is not None:
            input_bytes = input.encode(encoding)
            input_file = self.temp_mgr.make_temp_binary(prefix="hg_input.")
            input_file.write(input_bytes)
            input_file.seek(0)
            stdin = input_file.fileno()

        # Turn subprocess.PIPE to temporary files to avoid issues with selectors.
        stdout_file = None
        stderr_file = None
        if stdout is subprocess.PIPE:
            stdout = stdout_file = self.temp_mgr.make_temp_binary(prefix="hg_stdout.")
        if stderr is subprocess.PIPE:
            stderr = stderr_file = self.temp_mgr.make_temp_binary(prefix="hg_stdout.")

        if cwd is None:
            cwd = self.path

        result = None
        error = None
        stdout_content = None
        stderr_content = None
        try:
            result = subprocess.run(
                cmd,
                stdout=stdout,
                stderr=stderr,
                stdin=stdin,
                check=check,
                cwd=cwd,
                env=env,
            )
        except subprocess.CalledProcessError as ex:
            error = ex
        finally:
            if input_file:
                input_file.close()
            if stdout_file is not None:
                stdout_file.seek(0)
                stdout_content = stdout_file.read()
                stdout_file.close()
            if stderr_file is not None:
                stderr_file.seek(0)
                stderr_content = stderr_file.read()
                stderr_file.close()

        if error is not None:
            print("----------- Mercurial Crash Report")
            print("cmd: ", " ".join(cmd))
            if stdout_content is not None:
                error.stdout = stdout_content
                print("stdout: ", stdout_content.decode())
            if stderr_content is not None:
                error.stderr = stderr_content
                print("stderr: ", stderr_content.decode())
            print("----------- Mercurial Crash Report End")
            raise HgError(error) from error
        elif result is not None:
            if stdout_content is not None:
                result.stdout = stdout_content
            if stderr_content is not None:
                result.stderr = stderr_content
            return result
        else:
            # practically unreachable, just to make pyre happy.
            raise RuntimeError("either result or error should be set")

    def run(
        self, *args: str, encoding: str = "utf-8", env: Optional[Dict[str, str]] = None
    ) -> str:
        return self.hg(*args, encoding=encoding, env=env)

    def hg(
        self,
        *args: str,
        encoding: str = "utf-8",
        input: Optional[str] = None,
        hgeditor: Optional[str] = None,
        cwd: Optional[str] = None,
        check: bool = True,
        env: Optional[Dict[str, str]] = None,
    ) -> str:
        if "--debug" in args:
            stderr = subprocess.STDOUT
        else:
            stderr = subprocess.PIPE
        completed_process = self.run_hg(
            *args,
            encoding=encoding,
            input=input,
            hgeditor=hgeditor,
            cwd=cwd,
            check=check,
            stderr=stderr,
            env=env,
        )
        return typing.cast(
            str, completed_process.stdout.decode(encoding, errors="replace")
        )

    def init(
        self,
        hgrc: Optional[configparser.ConfigParser] = None,
        init_configs: Optional[List[str]] = None,
    ) -> None:
        """
        Initialize a new hg repository by running 'hg init'

        The hgrc parameter may be a configparser.ConfigParser() object
        describing configuration settings that should be added to the
        repository's .hg/hgrc file.
        """
        init_config_args = [f"--config={c}" for c in init_configs or ()]
        self.hg("init", *init_config_args)
        if hgrc is None:
            hgrc = configparser.ConfigParser()

        cachepath = os.path.join(self.path, ".hg", "remotefilelog_cachepath")
        try:
            os.mkdir(cachepath)
        except FileExistsError:
            pass

        # Eagerepo allows us to fake remote fetches from the server
        eagerepo = self.temp_mgr.make_temp_dir(prefix="eagerepo")

        hgrc.setdefault("remotefilelog", {})
        hgrc["remotefilelog"]["server"] = "false"
        hgrc["remotefilelog"]["reponame"] = "test"
        hgrc["remotefilelog"]["cachepath"] = cachepath

        # We should allow fetching tree aux data along with trees
        hgrc.add_section("scmstore")
        hgrc["scmstore"]["fetch-tree-aux-data"] = "true"

        # Some tests set these configs on their own. We shouldn't overwrite them.
        if not hgrc.has_section("paths"):
            hgrc.add_section("paths")
            hgrc["paths"]["default"] = f"eager://{eagerepo}"

        # Use Rust status.
        hgrc.setdefault("status", {})
        hgrc["status"]["use-rust"] = "true"

        # Use (native) Rust checkout whenever possible
        hgrc.setdefault("checkout", {})
        hgrc["checkout"]["use-rust"] = "true"

        # It's safe to use EdenAPI push for testing purposes
        hgrc.add_section("push")
        hgrc["push"]["edenapi"] = "true"

        # Turn off commit cloud. Some tests require testing the behavior of
        # local-only changes. Commit cloud makes it difficult to test that.
        if not hgrc.has_section("extensions"):
            hgrc.add_section("extensions")
        hgrc["extensions"]["commitcloud"] = "!"

        self.write_hgrc(hgrc)

        storerequirespath = os.path.join(self.path, ".hg", "store", "requires")
        with open(storerequirespath, "r") as f:
            storerequires = set(f.read().split())

        # eagerepo conflicts with remotefilelog repo
        if "eagerepo" not in storerequires:
            requirespath = os.path.join(self.path, ".hg", "requires")
            with open(requirespath, "a") as f:
                f.write("remotefilelog\n")

    def write_hgrc(self, hgrc: configparser.ConfigParser) -> None:
        hgrc_path = os.path.join(self.path, ".hg", "hgrc")
        with open(hgrc_path, "a") as f:
            # Explicitly %include the overridden system hgrc. This ensures
            # hg commands accessing the repo will load the overridden config,
            # even if HGRCPATH is not set properly.
            system_hgrc_path = self.hg_environment.get("HGRCPATH")
            if system_hgrc_path:
                f.write("%%include %s\n" % system_hgrc_path)
            hgrc.write(f)
            f.write("[hooks]\npost-pull.changelo-migrate=")

    def get_type(self) -> str:
        return "filteredhg" if self.filtered else "hg"

    def get_head_hash(self) -> str:
        return self.hg("log", "-r.", "-T{node}")

    def get_canonical_root(self) -> str:
        return self.path

    def add_files(self, paths: List[str]) -> None:
        self.staged_files += paths

    def add_staged_files(self) -> None:
        # add_files() may be called for files that are already tracked.
        # hg will print a warning, but this is fine.
        if self.staged_files:
            self.hg("add", *self.staged_files)
            self.staged_files = []

    def remove_files(self, paths: List[str], force: bool = False) -> None:
        if force:
            self.hg("remove", "--force", *paths)
        else:
            self.hg("remove", *paths)

    def commit(
        self,
        message: str,
        author_name: Optional[str] = None,
        author_email: Optional[str] = None,
        date: Optional[datetime.datetime] = None,
        amend: bool = False,
    ) -> str:
        """
        - message Commit message to use.
        - author_name Author name to use: defaults to self.author_name.
        - author_email Author email to use: defaults to self.author_email.
        - date datetime.datetime to use for the commit. Defaults to
          self.get_commit_time().
        - amend If true, adds the `--amend` argument.
        """
        self.add_staged_files()

        if author_name is None:
            author_name = self.author_name
        if author_email is None:
            author_email = self.author_email
        if date is None:
            date = self.get_commit_time()
        # Mercurial's internal format of <unix_timestamp> <timezone>
        date_str = "{} 0".format(int(date.timestamp()))

        user_config = "ui.username={} <{}>".format(author_name, author_email)

        with self.temp_mgr.make_temp_file(prefix="hg_commit_msg.") as msgf:
            msgf.write(message)
            msgf.flush()

            args = [
                "commit",
                "--config",
                user_config,
                "--date",
                date_str,
                "--logfile",
                msgf.name,
            ]
            if amend:
                args.append("--amend")

            # Do not capture stdout or stderr when running "hg commit"
            # This allows its output to show up in the test logs.
            self.run_hg(
                *args,
                stdout=None,
                stderr=None,
            )

        # Get the commit ID and return it
        return self.hg("log", "-T{node}", "-r.")

    def log(self, template: str = "{node}", revset: str = "::.") -> List[str]:
        """Runs `hg log` with the specified template and revset.

        Returns the log output, as a list with one entry per commit."""
        # Append a separator to the template so we can split up the entries
        # afterwards.  Use a slightly more complex string rather than just a
        # single nul byte, just in case the caller uses internal nuls in their
        # template to split fields.
        escaped_delimiter = r"\0-+-\0"
        delimiter = "\0-+-\0"
        assert escaped_delimiter not in template
        template += escaped_delimiter
        output = self.hg("log", "-T", template, "-r", revset)
        return output.split(delimiter)[:-1]

    def journal(self) -> List[Dict[str, Any]]:
        output = self.hg("journal", "-T", "json")
        json_data = json.loads(output)
        return typing.cast(List[Dict[str, Any]], json_data)

    def status(
        self, include_ignored: bool = False, rev: Optional[str] = None, **opts
    ) -> Dict[str, str]:
        """Returns the output of `hg status` as a dictionary of {path: status char}.

        The status characters are the same as the ones documented by `hg help status`
        """
        args = ["status", "--print0"]
        if include_ignored:
            args.append("-mardui")
        if rev is not None:
            args.append("--rev")
            args.append(rev)

        output = self.hg(*args, **opts)
        status = {}
        for entry in output.split("\0"):
            if not entry:
                continue
            flag = entry[0]
            path = entry[2:]
            if path != "default.profraw":
                status[path] = flag

        return status

    def update(self, rev: str, clean: bool = False, merge: bool = False, **opts) -> str:
        args = ["update"]
        if clean:
            args.append("--clean")
        if merge:
            args.append("--merge")
        args.append(rev)
        return self.hg(*args, **opts)

    def reset(self, rev: str, keep: bool = True) -> None:
        if keep:
            args = ["reset", "--keep", rev]
        else:
            args = ["reset", rev]
        self.run_hg(*args, stdout=None, stderr=None)

    def push(self, rev: str, target: str, create: bool = False) -> str:
        args = ["push", "-r", rev, "--to", target]
        if create:
            args.append("--create")

        return self.hg(*args)
