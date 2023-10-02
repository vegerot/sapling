# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

import asyncio
from typing import Any, Dict, Sequence, Union

import ghstack.github
from ghstack.github_gh_cli import JsonDict, make_request
from sapling.result import Result


class GitHubCLIEndpoint(ghstack.github.GitHubEndpoint):
    """Alternative to RealGitHubEndpoint that makes all of its requests via the
    GitHub CLI. The primary benefit to end-users is that there is no need to
    create a ~/.ghstackrc file, which can be a stumbling block when getting
    started with ghstack.

    Though note the primary tradeoff is that invoking a method of this class
    entails spawning a new process, which may be problematic for Windows users.
    """

    def __init__(self, hostname: str):
        """The hostname of the GitHub Enterprise instance or 'github.com' if the
        consumer instance."""
        self.hostname = hostname

    def push_hook(self, refName: Sequence[str]) -> None:
        pass

    def rest(self, method: str, path: str, **kwargs: Any) -> Any:
        params: Dict[str, Union[str, int, bool]] = dict(kwargs)
        loop = asyncio.get_event_loop()
        result = loop.run_until_complete(
            make_request(params, hostname=self.hostname, endpoint=path, method=method)
        )
        if result.is_err():
            raise RuntimeError(result.unwrap_err())
        else:
            return result.unwrap()

    def graphql_sync(self, query: str, **kwargs: Any) -> Any:
        params: Dict[str, Union[str, int, bool]] = dict(kwargs)
        params["query"] = query
        loop = asyncio.get_event_loop()
        result = loop.run_until_complete(make_request(params, hostname=self.hostname))
        if result.is_err():
            raise RuntimeError(result.unwrap_err())
        else:
            return result.unwrap()

    async def graphql(self, query: str, **kwargs: Any) -> Result[JsonDict, str]:
        params: Dict[str, Union[str, int, bool]] = dict(kwargs)
        params["query"] = query
        return await make_request(params, hostname=self.hostname)
