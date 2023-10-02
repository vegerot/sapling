# Portions Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

# match.py - filename matching
#
#  Copyright 2008, 2009 Olivia Mackall <olivia@selenic.com> and others
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2 or any later version.

from __future__ import absolute_import, print_function

import copy
import os
import re
from typing import List, Optional, Pattern, Sized, Tuple

from bindings import pathmatcher

from . import error, identity, pathutil, pycompat, util
from .i18n import _
from .pycompat import decodeutf8


MAX_RE_SIZE = 20000


allpatternkinds = (
    "re",
    "glob",
    "path",
    "relglob",
    "relpath",
    "relre",
    "listfile",
    "listfile0",
    "set",
    "rootfilesin",
)
cwdrelativepatternkinds = ("relpath", "glob")

propertycache = util.propertycache


def _rematcher(regex):
    """compile the regexp with the best available regexp engine and return a
    matcher function"""
    m = util.re.compile(regex)
    try:
        # slightly faster, provided by facebook's re2 bindings
        return m.test_match
    except AttributeError:
        return m.match


def _expandsets(kindpats, ctx):
    """Returns the kindpats list with the 'set' patterns expanded."""
    fset = set()
    other = []

    for kind, pat, source in kindpats:
        if kind == "set":
            if not ctx:
                raise error.ProgrammingError("fileset expression with no " "context")
            s = ctx.getfileset(pat)
            fset.update(s)
            continue
        other.append((kind, pat, source))
    return fset, other


def _kindpatsalwaysmatch(kindpats) -> bool:
    """ "Checks whether the kindspats match everything, as e.g.
    'relpath:.' does.
    """
    if not kindpats:
        return False

    emptymeansalways = {"relpath"}
    if _emptyglobalwaysmatches:
        emptymeansalways.add("glob")

    for kind, pat, source in kindpats:
        # TODO: update me?
        if pat != "" or kind not in emptymeansalways:
            return False
    return True


def match(
    root,
    cwd,
    patterns=None,
    include=None,
    exclude=None,
    default: str = "glob",
    ctx=None,
    warn=None,
    badfn=None,
    icasefs: bool = False,
):
    """build an object to match a set of file patterns

    arguments:
    root - the canonical root of the tree you're matching against
    cwd - the current working directory, if relevant
    patterns - patterns to find
    include - patterns to include (unless they are excluded)
    exclude - patterns to exclude (even if they are included)
    default - if a pattern in patterns has no explicit type, assume this one
    warn - optional function used for printing warnings
    badfn - optional bad() callback for this matcher instead of the default
    icasefs - make a matcher for wdir on case insensitive filesystems, which
        normalizes the given patterns to the case in the filesystem

    a pattern is one of:
    'glob:<glob>' - a glob relative to cwd
    're:<regexp>' - a regular expression
    'path:<path>' - a path relative to repository root, which is matched
                    recursively
    'rootfilesin:<path>' - a path relative to repository root, which is
                    matched non-recursively (will not match subdirectories)
    'relglob:<glob>' - an unrooted glob (*.c matches C files in all dirs)
    'relpath:<path>' - a path relative to cwd
    'relre:<regexp>' - a regexp that needn't match the start of a name
    'set:<fileset>' - a fileset expression
    '<something>' - a pattern of the specified default type
    """

    if _userustmatcher:
        try:
            hm = hintedmatcher(
                root,
                cwd,
                patterns or [],
                include or [],
                exclude or [],
                default,
                ctx,
                casesensitive=not icasefs,
                badfn=badfn,
            )
        except (error.UncategorizedNativeError, ValueError) as ex:
            if util.istest():
                raise
            if ctx:
                ctx.repo().ui.log("pathmatcher_info", hinted_matcher_error=str(ex))
            pass
        else:
            if warn:
                for warning in hm.warnings():
                    warn("warning: " + identity.replace(warning) + "\n")
            return hm

    normalize = _donormalize
    if icasefs:
        dirstate = ctx.repo().dirstate
        dsnormalize = dirstate.normalize

        def normalize(patterns, default, root, cwd, warn):
            kp = _donormalize(patterns, default, root, cwd, warn)
            kindpats = []
            for kind, pat, source in kp:
                if kind not in ("re", "relre"):  # regex can't be normalized
                    p = pat
                    pat = dsnormalize(pat)

                    # Preserve the original to handle a case only rename.
                    if p != pat and p in dirstate:
                        kindpats.append((kind, p, source))

                kindpats.append((kind, pat, source))
            return kindpats

    m = None
    if not patterns:
        m = alwaysmatcher(root, cwd, badfn)

    patternskindpats = not m and normalize(patterns, default, root, cwd, warn)
    includekindpats = include and normalize(include, "glob", root, cwd, warn)
    excludekindpats = exclude and normalize(exclude, "glob", root, cwd, warn)

    # Try to use Rust dyn matcher if possible. Currently, Rust dyn matcher is
    # missing below features:
    # * explicit files in Matcher trait
    # * pattern kinds other than 'glob' and 're'
    if _usedynmatcher and not m:
        matcher = _builddynmatcher(
            root=root,
            cwd=cwd,
            patternskindpats=patternskindpats or [],
            includekindpats=includekindpats or [],
            excludekindpats=excludekindpats or [],
            default=default,
            badfn=badfn,
        )
        if matcher:
            return matcher
        # else fallback to original logic

    if not m:
        if _kindpatsalwaysmatch(patternskindpats):
            m = alwaysmatcher(root, cwd, badfn, relativeuipath=True)
        else:
            m = _buildpatternmatcher(root, cwd, patternskindpats, ctx=ctx, badfn=badfn)
    if include:
        im = _buildpatternmatcher(
            root,
            cwd,
            includekindpats,
            ctx=ctx,
            badfn=None,
            fallbackmatcher=includematcher,
        )
        m = intersectmatchers(m, im)
    if exclude:
        em = _buildpatternmatcher(
            root,
            cwd,
            excludekindpats,
            ctx=ctx,
            badfn=None,
            fallbackmatcher=includematcher,
        )
        m = differencematcher(m, em)
    return m


def _builddynmatcher(
    root,
    cwd,
    patternskindpats: List[str],
    includekindpats: List[str],
    excludekindpats: List[str],
    default: str = "glob",
    badfn=None,
) -> Optional["dynmatcher"]:
    def generatenormalizedpatterns(
        kindpats, default, recursive, ispatterns
    ) -> Optional[List[str]]:
        if ispatterns:
            if not kindpats:
                # empty patterns means nevermatcher here
                return None
            elif _kindpatsalwaysmatch(kindpats) or any(_explicitfiles(kindpats)):
                # * Rust AlwaysMatcher doesn't support relative ui path now
                # * Rust Matchers doesn't support explicit files
                return None

        res = _kindpatstoglobsregexs(kindpats, recursive=recursive)
        if not res:
            # Rust build matcher only supports 'glob' and 're' now
            return None
        globs, regexs = res
        return [f"glob:{x}" for x in globs] + [f"re:{x}" for x in regexs]

    normalizedpatterns = generatenormalizedpatterns(
        patternskindpats, default, False, True
    )
    if normalizedpatterns is None:
        return None
    normalizedinclude = generatenormalizedpatterns(includekindpats, "glob", True, False)
    if normalizedinclude is None:
        return None
    normalizedexclude = generatenormalizedpatterns(excludekindpats, "glob", True, False)
    if normalizedexclude is None:
        return None

    try:
        m = dynmatcher(
            root,
            cwd,
            normalizedpatterns,
            normalizedinclude,
            normalizedexclude,
            casesensitive=True,
            badfn=badfn,
        )
        return m
    except (error.UncategorizedNativeError, ValueError):
        # possible exceptions:
        #   * TreeMatcher: Regex("Compiled regex exceeds size limit of 10485760 bytes.")
        #   * RegexMatcher: doesn't support '\b' and '\B'
        return None


def exact(root, cwd, files, badfn=None) -> "exactmatcher":
    return exactmatcher(root, cwd, files, badfn=badfn)


def always(root, cwd) -> "alwaysmatcher":
    return alwaysmatcher(root, cwd)


def never(root, cwd) -> "nevermatcher":
    return nevermatcher(root, cwd)


def union(matches, root, cwd):
    """Union a list of matchers.

    If the list is empty, return nevermatcher.
    If the list only contains one non-None value, return that matcher.
    Otherwise return a union matcher.
    """
    matches = list(filter(None, matches))
    if len(matches) == 0:
        return nevermatcher(root, cwd)
    elif len(matches) == 1:
        return matches[0]
    else:
        return unionmatcher(matches)


def badmatch(match, badfn):
    """Make a copy of the given matcher, replacing its bad method with the given
    one.
    """
    m = copy.copy(match)
    m.bad = badfn
    return m


def _donormalize(patterns, default, root, cwd, warn):
    """Convert 'kind:pat' from the patterns list to tuples with kind and
    normalized and rooted patterns and with listfiles expanded."""
    kindpats = []
    for kind, pat in [_patsplit(p, default) for p in patterns]:
        if warn and kind in {"path", "relpath", "rootfilesin"} and "*" in pat:
            warn(
                _(
                    "possible glob in non-glob pattern '{pat}', did you mean 'glob:{pat}'? "
                    "(see '@prog@ help patterns' for details).\n"
                ).format(pat=pat)
            )

        if kind in cwdrelativepatternkinds:
            pat = pathutil.canonpath(root, cwd, pat)
        elif kind in ("relglob", "path", "rootfilesin"):
            pat = util.normpath(pat)
        elif kind in ("listfile", "listfile0"):
            try:
                files = decodeutf8(util.readfile(pat))
                if kind == "listfile0":
                    files = files.split("\0")
                else:
                    files = files.splitlines()
                files = [f for f in files if f]
            except EnvironmentError:
                raise error.Abort(_("unable to read file list (%s)") % pat)

            if not files:
                if warn:
                    warn(_("empty %s %s matches nothing\n") % (kind, pat))

            for k, p, source in _donormalize(files, default, root, cwd, warn):
                kindpats.append((k, p, pat))
            continue

        # else: re or relre - which cannot be normalized
        kindpats.append((kind, pat, ""))
    return kindpats


def _testrefastpath(repat) -> bool:
    """Test if a re pattern can use fast path.

    That is, for every "$A/$B" path the pattern matches, "$A" must also be
    matched,

    Return True if we're sure it is. Return False otherwise.
    """
    # XXX: It's very hard to implement this. These are what need to be
    # supported in production and tests. Very hacky. But we plan to get rid
    # of re matchers eventually.

    # Rules like "(?!experimental/)"
    if repat.startswith("(?!") and repat.endswith(")") and repat.count(")") == 1:
        return True

    # Rules used in doctest
    if repat == "(i|j)$":
        return True

    return False


def _globpatsplit(pat) -> List[str]:
    """Split a glob pattern. Return a list.

    A naive version is "path.split("/")". This function handles more cases, like
    "{*,{a,b}*/*}".

    >>> _globpatsplit("*/**/x/{a,b/c}")
    ['*', '**', 'x', '{a,b/c}']
    """
    result = []
    buf = ""
    parentheses = 0
    for ch in pat:
        if ch == "{":
            parentheses += 1
        elif ch == "}":
            parentheses -= 1
        if parentheses == 0 and ch == "/":
            if buf:
                result.append(buf)
                buf = ""
        else:
            buf += ch
    if buf:
        result.append(buf)
    return result


class _tree(dict):
    """A tree intended to answer "visitdir" questions with more efficient
    answers (ex. return "all" or False if possible).
    """

    def __init__(self, *args, **kwargs):
        # If True, avoid entering subdirectories, and match everything recursively,
        # unconditionally.
        self.matchrecursive = False
        # If True, avoid entering subdirectories, and return "unsure" for
        # everything. This is set to True when complex re patterns (potentially
        # including "/") are used.
        self.unsurerecursive = False
        # Patterns for matching paths in this directory.
        self._kindpats = []
        # Glob patterns used to match parent directories of another glob
        # pattern.
        self._globdirpats = []
        super(_tree, self).__init__(*args, **kwargs)

    def insert(self, path, matchrecursive=True, globpats=None, repats=None):
        """Insert a directory path to this tree.

        If matchrecursive is True, mark the directory as unconditionally
        include files and subdirs recursively.

        If globpats or repats are specified, append them to the patterns being
        applied at this directory. The tricky part is those patterns can match
        "x/y/z" and visit("x"), visit("x/y") need to return True, while we
        still want visit("x/a") to return False.
        """
        if path == "":
            self.matchrecursive |= matchrecursive
            if globpats:
                # Need to match parent directories too.
                for pat in globpats:
                    components = _globpatsplit(pat)
                    parentpat = ""
                    for comp in components:
                        if parentpat:
                            parentpat += "/"
                        parentpat += comp
                        if "/" in comp:
                            # Giving up - fallback to slow paths.
                            self.unsurerecursive = True
                        self._globdirpats.append(parentpat)
                if any("**" in p for p in globpats):
                    # Giving up - "**" matches paths including "/"
                    self.unsurerecursive = True
                self._kindpats += [("glob", pat, "") for pat in globpats]
            if repats:
                if not all(map(_testrefastpath, repats)):
                    # Giving up - fallback to slow paths.
                    self.unsurerecursive = True
                self._kindpats += [("re", pat, "") for pat in repats]
            return

        subdir, rest = self._split(path)
        self.setdefault(subdir, _tree()).insert(rest, matchrecursive, globpats, repats)

    def visitdir(self, path):
        """Similar to matcher.visitdir"""
        path = normalizerootdir(path, "visitdir")
        if self.matchrecursive:
            return "all"
        elif self.unsurerecursive:
            return True
        elif path == "":
            return True

        if self._kindpats and self._compiledpats(path):
            # XXX: This is incorrect. But re patterns are already used in
            # production. We should kill them!
            # Need to test "if every string starting with 'path' matches".
            # Obviously it's impossible to test *every* string with the
            # standard regex API, therefore pick a random strange path to test
            # it approximately.
            if self._compiledpats("%s/*/_/-/0/*" % path):
                return "all"
            else:
                return True

        if self._globdirpats and self._compileddirpats(path):
            return True

        subdir, rest = self._split(path)
        subtree = self.get(subdir)
        if subtree is None:
            return False
        else:
            return subtree.visitdir(rest)

    @util.propertycache
    def _compiledpats(self):
        pat, matchfunc = _buildregexmatch(self._kindpats, "")
        return matchfunc

    @util.propertycache
    def _compileddirpats(self):
        pat, matchfunc = _buildregexmatch(
            [("glob", p, "") for p in self._globdirpats], "$"
        )
        return matchfunc

    def _split(self, path):
        if "/" in path:
            subdir, rest = path.split("/", 1)
        else:
            subdir, rest = path, ""
        if not subdir:
            raise error.ProgrammingError("path cannot be absolute")
        return subdir, rest


def _remainingpats(pat, prefix: Sized):
    """list of patterns with prefix stripped

    >>> _remainingpats("a/b/c", "")
    ['a/b/c']
    >>> _remainingpats("a/b/c", "a")
    ['b/c']
    >>> _remainingpats("a/b/c", "a/b")
    ['c']
    >>> _remainingpats("a/b/c", "a/b/c")
    []
    >>> _remainingpats("", "")
    []
    """
    if prefix:
        if prefix == pat:
            return []
        else:
            assert pat[len(prefix)] == "/"
            return [pat[len(prefix) + 1 :]]
    else:
        if pat:
            return [pat]
        else:
            return []


def _buildvisitdir(kindpats):
    """Try to build an efficient visitdir function

    Return a visitdir function if it's built. Otherwise return None
    if there are unsupported patterns.

    >>> _buildvisitdir([('include', 'foo', '')])
    >>> _buildvisitdir([('relglob', 'foo', '')])
    >>> t = _buildvisitdir([
    ...     ('glob', 'a/b', ''),
    ...     ('glob', 'c/*.d', ''),
    ...     ('glob', 'e/**/*.c', ''),
    ...     ('re', '^f/(?!g)', ''), # no "$", only match prefix
    ...     ('re', '^h/(i|j)$', ''),
    ...     ('glob', 'i/a*/b*/c*', ''),
    ...     ('glob', 'i/a5/b7/d', ''),
    ...     ('glob', 'j/**.c', ''),
    ... ])
    >>> t('a')
    True
    >>> t('a/b')
    'all'
    >>> t('a/b/c')
    'all'
    >>> t('c')
    True
    >>> t('c/d')
    False
    >>> t('c/rc.d')
    'all'
    >>> t('c/rc.d/foo')
    'all'
    >>> t('e')
    True
    >>> t('e/a')
    True
    >>> t('e/a/b.c')
    True
    >>> t('e/a/b.d')
    True
    >>> t('f')
    True
    >>> t('f/g')
    False
    >>> t('f/g2')
    False
    >>> t('f/g/a')
    False
    >>> t('f/h')
    'all'
    >>> t('f/h/i')
    'all'
    >>> t('h/i')
    True
    >>> t('h/i/k')
    False
    >>> t('h/k')
    False
    >>> t('i')
    True
    >>> t('i/a1')
    True
    >>> t('i/b2')
    False
    >>> t('i/a/b2/c3')
    'all'
    >>> t('i/a/b2/d4')
    False
    >>> t('i/a5/b7/d')
    'all'
    >>> t('j/x/y')
    True
    >>> t('z')
    False
    """
    tree = _tree()
    for kind, pat, _source in kindpats:
        if kind == "glob":
            components = []
            for p in pat.split("/"):
                if "[" in p or "{" in p or "*" in p or "?" in p:
                    break
                components.append(p)
            prefix = "/".join(components)
            matchrecursive = prefix == pat
            tree.insert(
                prefix,
                matchrecursive=matchrecursive,
                globpats=_remainingpats(pat, prefix),
            )
        elif kind == "re":
            # Still try to get a plain prefix from the regular expression so we
            # can still have fast paths.
            if pat.startswith("^"):
                # "re" already matches from the beginning, unlike "relre"
                pat = pat[1:]
            components = []
            for p in pat.split("/"):
                if re.escape(p) != p:
                    # contains special characters
                    break
                components.append(p)
            prefix = "/".join(components)
            tree.insert(
                prefix, matchrecursive=False, repats=_remainingpats(pat, prefix)
            )
        else:
            # Unsupported kind
            return None
    return tree.visitdir


class basematcher:
    def __init__(self, root, cwd, badfn=None, relativeuipath=True):
        self._root = root
        self._cwd = cwd
        if badfn is not None:
            self.bad = badfn
        self._relativeuipath = relativeuipath

    def __repr__(self):
        return "<%s>" % self.__class__.__name__

    def __call__(self, fn):
        return self.matchfn(fn)

    def __iter__(self):
        for f in self._files:
            yield f

    # Callbacks related to how the matcher is used by dirstate.walk.
    # Subscribers to these events must monkeypatch the matcher object.
    def bad(self, f, msg):
        """Callback from dirstate.walk for each explicit file that can't be
        found/accessed, with an error message."""

    # If an traversedir is set, it will be called when a directory discovered
    # by recursive traversal is visited.
    traversedir = None

    def abs(self, f):
        """Convert a repo path back to path that is relative to the root of the
        matcher."""
        return f

    def rel(self, f):
        """Convert repo path back to path that is relative to cwd of matcher."""
        return util.pathto(self._root, self._cwd, f)

    def uipath(self, f):
        """Convert repo path to a display path.  If patterns or -I/-X were used
        to create this matcher, the display path will be relative to cwd.
        Otherwise it is relative to the root of the repo."""
        return (self._relativeuipath and self.rel(f)) or self.abs(f)

    @propertycache
    def _files(self):
        return []

    def files(self):
        """Explicitly listed files or patterns or roots:
        if no patterns or .always(): empty list,
        if exact: list exact files,
        if not .anypats(): list all files and dirs,
        else: optimal roots"""
        return self._files

    @propertycache
    def _fileset(self):
        return set(self._files)

    def exact(self, f):
        """Returns True if f is in .files()."""
        return f in self._fileset

    def matchfn(self, f):
        return False

    def visitdir(self, dir):
        """Decides whether a directory should be visited based on whether it
        has potential matches in it or one of its subdirectories. This is
        based on the match's primary, included, and excluded patterns.

        Returns the string 'all' if the given directory and all subdirectories
        should be visited. Otherwise returns True or False indicating whether
        the given directory should be visited.
        """
        return True

    def always(self):
        """Matcher will match everything and .files() will be empty.
        Optimization might be possible."""
        return False

    def isexact(self):
        """Matcher matches exactly the list of files in .files(), and nothing else.
        Optimization might be possible."""
        return False

    def prefix(self):
        """Matcher matches the paths in .files() recursively, and nothing else.
        Optimization might be possible."""
        return False

    def anypats(self):
        """Matcher contains a non-trivial pattern (i.e. non-path and non-always).
        If this returns False, code assumes files() is all that matters.
        Optimizations will be difficult."""
        if self.always():
            # This is confusing since, conceptually, we are saying
            # there aren't patterns when we have a pattern like "**".
            # But since always() implies files() is empty, it is safe
            # for code to assume files() is all that's important.
            return False

        if self.isexact():
            # Only exacty files - no patterns.
            return False

        if self.prefix():
            # Only recursive paths - no patterns.
            return False

        return True


class alwaysmatcher(basematcher):
    """Matches everything."""

    def __init__(self, root, cwd, badfn=None, relativeuipath=False):
        super(alwaysmatcher, self).__init__(
            root, cwd, badfn, relativeuipath=relativeuipath
        )

    def always(self):
        return True

    def matchfn(self, f):
        return True

    def visitdir(self, dir):
        return "all"

    def __repr__(self):
        return "<alwaysmatcher>"


class nevermatcher(basematcher):
    """Matches nothing."""

    def __init__(self, root, cwd, badfn=None):
        super(nevermatcher, self).__init__(root, cwd, badfn)

    # It's a little weird to say that the nevermatcher is an exact matcher
    # or a prefix matcher, but it seems to make sense to let callers take
    # fast paths based on either. There will be no exact matches, nor any
    # prefixes (files() returns []), so fast paths iterating over them should
    # be efficient (and correct).
    def isexact(self):
        return True

    def prefix(self):
        return True

    def visitdir(self, dir):
        return False

    def __repr__(self):
        return "<nevermatcher>"


class gitignorematcher(basematcher):
    """Match files specified by ".gitignore"s"""

    def __init__(self, root, cwd, badfn=None, gitignorepaths=None):
        super(gitignorematcher, self).__init__(root, cwd, badfn)
        gitignorepaths = gitignorepaths or []
        self._matcher = pathmatcher.gitignorematcher(
            root, gitignorepaths, util.fscasesensitive(root)
        )

    def matchfn(self, f):
        return self._matcher.match_relative(f, False)

    def explain(self, f):
        return self._matcher.explain(f, True)

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        matched = self._matcher.match_relative(dir, True)
        if matched:
            # Everything in the directory is selected (ignored)
            return "all"
        else:
            # Not sure
            return True

    def __repr__(self):
        return "<gitignorematcher>"


def rulesmatch(root, cwd, rules, ruledetails=None) -> "treematcher":
    # Strip the exclude indicator from the rules, then reapply it later after
    # normalizing everything.
    excludeindexes = set()
    strippedrules = []
    for i, rule in enumerate(rules):
        if rule[0] == "!":
            excludeindexes.add(i)
            strippedrules.append(rule[1:])
        else:
            strippedrules.append(rule)

    kindpats = _donormalize(strippedrules, "glob", root, cwd, None)
    globs = _kindpatstoglobs(kindpats, recursive=True)
    if globs is None:
        raise error.Abort(
            _(
                "treematcher does not support regular expressions or relpath matchers: %s"
            )
            % rules
        )

    rules = []
    for i, glob in enumerate(globs):
        if i in excludeindexes:
            rules.append("!" + glob)
        else:
            rules.append(glob)

    return treematcher(root, "", rules=rules, ruledetails=ruledetails)


class treematcher(basematcher):
    """Match glob patterns with negative pattern support.
    Have a smarter 'visitdir' implementation.
    """

    def __init__(
        self, root, cwd, badfn=None, rules=[], ruledetails=None, casesensitive=True
    ):
        super(treematcher, self).__init__(root, cwd, badfn)
        rules = list(rules)

        self._matcher = pathmatcher.treematcher(rules, casesensitive)

        self._rules = rules
        self._ruledetails = ruledetails

    def matchfn(self, f):
        return self._matcher.matches(f)

    def visitdir(self, dir):
        matched = self._matcher.match_recursive(dir)
        if matched is None:
            return True
        elif matched is True:
            return "all"
        else:
            assert matched is False
            return False

    def explain(self, f):
        matchingidxs = self._matcher.matching_rule_indexes(f)
        if matchingidxs:
            # Use the final matching index (this follows the "last match wins"
            # logic within the tree matcher).
            rule = self._rules[matchingidxs[-1]]
            if self._ruledetails:
                rule = "{} ({})".format(rule, self._ruledetails[matchingidxs[-1]])

            return rule

        return None

    def __repr__(self):
        return "<treematcher rules=%r>" % self._rules


class regexmatcher(basematcher):
    """Match regex patterns."""

    def __init__(self, root, cwd, pattern, badfn=None):
        super(regexmatcher, self).__init__(root, cwd, badfn)
        self._matcher = pathmatcher.regexmatcher(pattern, util.fscasesensitive(root))
        self._pattern = pattern

    def matchfn(self, f):
        return self._matcher.matches(f)

    def visitdir(self, dir):
        matched = self._matcher.match_prefix(dir)
        if matched is None:
            return True
        elif matched is True:
            return "all"
        else:
            assert matched is False, f"expected False, but got {matched}"
            return False

    def explain(self, f):
        if self._matcher.matches(f):
            return self._pattern

    def __repr__(self):
        return f"<regexmatcher pattern={self._pattern!r}>"


class hintedmatcher(basematcher):
    """Rust matcher fully implementing Python API."""

    def __init__(
        self,
        root,
        cwd,
        patterns: List[str],
        include: List[str],
        exclude: List[str],
        default: str,
        ctx,
        casesensitive: bool,
        badfn=None,
    ):
        super(hintedmatcher, self).__init__(
            root, cwd, badfn, relativeuipath=bool(patterns or include or exclude)
        )

        def expandsets(pats, default):
            fset, nonsets = set(), []
            for pat in pats:
                k, p = _patsplit(pat, default)
                if k == "set":
                    if not ctx:
                        raise error.ProgrammingError(
                            "fileset expression with no " "context"
                        )
                    fset.update(ctx.getfileset(p))
                else:
                    nonsets.append(pat)

            if len(nonsets) == len(pats):
                return nonsets, None
            else:
                return nonsets, list(fset)

        self._matcher = pathmatcher.hintedmatcher(
            *expandsets(patterns, default),
            *expandsets(include, "glob"),
            *expandsets(exclude, "glob"),
            default,
            casesensitive,
            root,
            cwd,
        )
        self._files = self._matcher.exact_files()

    def matchfn(self, f):
        return self._matcher.matches_file(f)

    def visitdir(self, dir):
        matched = self._matcher.matches_directory(dir)
        if matched is None:
            return True
        elif matched is True:
            return "all"
        else:
            assert matched is False, f"expected False, but got {matched}"
            return False

    def always(self):
        return self._matcher.always_matches()

    def prefix(self):
        return self._matcher.all_recursive_paths()

    def isexact(self):
        # Similar to nevermatcher, let the knowledge that we never match
        # allow isexact() fast paths.
        return self._matcher.never_matches()

    def warnings(self):
        return self._matcher.warnings()


class dynmatcher(basematcher):
    """Rust dyn matcher created by the build matcher API."""

    def __init__(
        self,
        root,
        cwd,
        patterns: List[str],
        include: List[str],
        exclude: List[str],
        casesensitive: bool = True,
        badfn=None,
    ):
        super(dynmatcher, self).__init__(root, cwd, badfn)
        self._matcher = pathmatcher.dynmatcher(
            patterns, include, exclude, casesensitive
        )
        self.patterns = patterns
        self.include = include
        self.exclude = exclude
        self.casesensitive = casesensitive

    def matchfn(self, f):
        return self._matcher.matches_file(f)

    def visitdir(self, dir):
        matched = self._matcher.matches_directory(dir)
        if matched is None:
            return True
        elif matched is True:
            return "all"
        else:
            assert matched is False, f"expected False, but got {matched}"
            return False

    def __repr__(self):
        return "<dynmatcher patterns=%r include=%r exclude=%r casesensitive=%r>" % (
            self.patterns,
            self.include,
            self.exclude,
            self.casesensitive,
        )


def normalizerootdir(dir: str, funcname) -> str:
    if dir == ".":
        util.nouideprecwarn(
            "match.%s() no longer accepts '.', use '' instead." % funcname, "20190805"
        )
        return ""
    return dir


def _kindpatstoglobs(kindpats, recursive: bool = False) -> Optional[List[str]]:
    "Attempt to convert kindpats to globs that can be used in a treematcher."
    if _usetreematcher:
        res = _kindpatstoglobsregexs(kindpats, recursive)
        if res and not res[1]:
            return res[0]


def _kindpatstoglobsregexs(
    kindpats, recursive: bool = False
) -> Optional[Tuple[List, List]]:
    """Attempt to convert 'kindpats' to (glob patterns, regex patterns).

    kindpats should be already normalized to be relative to repo root.

    If recursive is True, `glob:a*` will match both `a1/b` and `a1`, otherwise
    `glob:a*` will only match `a1` but not `a1/b`.

    Return None if there are unsupported patterns (ex. set expressions).
    """
    globs, regexs = [], []
    for kindpat in kindpats:
        kind, pat = kindpat[0:2]
        subkindpats = [(kind, pat)]
        if kind == "set":
            # Attempt to rewrite fileset to non-fileset patterns
            from . import fileset

            maybekindpats = fileset.maybekindpats(pat)
            if maybekindpats is not None:
                subkindpats = maybekindpats
        for kind, pat in subkindpats:
            if kind == "re":
                # Attempt to convert the re pat to globs
                reglobs = _convertretoglobs(pat)
                if reglobs is not None:
                    globs += reglobs
                else:
                    regexs.append(pat)
            elif kind == "glob":
                # The treematcher (man gitignore) does not support csh-style
                # brackets (ex. "{a,b,c}"). Expand the brackets to patterns.
                for subpat in pathmatcher.expandcurlybrackets(pat):
                    normalized = pathmatcher.normalizeglob(subpat)
                    if recursive:
                        normalized = _makeglobrecursive(normalized)
                    globs.append(normalized)
            elif kind == "path":
                if pat == ".":
                    # Special case. Comes from `util.normpath`.
                    pat = ""
                else:
                    pat = pathmatcher.plaintoglob(pat)
                pat = _makeglobrecursive(pat)
                globs.append(pat)
            else:
                return None
    return globs, regexs


def _makeglobrecursive(pat):
    """Make a glob pattern recursive by appending "/**" to it"""
    if pat.endswith("/") or not pat:
        return pat + "**"
    else:
        return pat + "/**"


# re:x/(?!y/)
# meaning: include x, but not x/y.
_repat1: Pattern[str] = re.compile(r"^\^?([\w._/]+)/\(\?\!([\w._/]+)/?\)$")

# re:x/(?:.*/)?y
# meaning: glob:x/**/y
_repat2: Pattern[str] = re.compile(
    r"^\^?([\w._/]+)/\(\?:\.\*/\)\?([\w._]+)(?:\(\?\:\/\|\$\))?$"
)


def _convertretoglobs(repat) -> Optional[List[str]]:
    """Attempt to convert a regular expression pattern to glob patterns.

    A single regular expression pattern might be converted into multiple
    glob patterns.

    Return None if conversion is unsupported.

    >>> _convertretoglobs("abc*") is None
    True
    >>> _convertretoglobs("xx/yy/(?!zz/kk)")
    ['xx/yy/**', '!xx/yy/zz/kk/**']
    >>> _convertretoglobs("x/y/(?:.*/)?BUCK")
    ['x/y/**/BUCK']
    """
    m = _repat1.match(repat)
    if m:
        prefix, excluded = m.groups()
        return ["%s/**" % prefix, "!%s/%s/**" % (prefix, excluded)]
    m = _repat2.match(repat)
    if m:
        prefix, name = m.groups()
        return ["%s/**/%s" % (prefix, name)]
    return None


class patternmatcher(basematcher):
    def __init__(self, root, cwd, kindpats, ctx=None, badfn=None):
        super(patternmatcher, self).__init__(root, cwd, badfn)
        # kindpats are already normalized to be relative to repo-root.
        self._prefix = _prefix(kindpats)
        self._pats, self.matchfn = _buildmatch(ctx, kindpats, "$", root)
        self._files = _explicitfiles(kindpats)

    @propertycache
    def _dirs(self):
        return set(util.dirs(self._fileset))

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        if self._prefix and dir in self._fileset:
            return "all"
        if not self._prefix:
            return True
        return (
            dir in self._fileset
            or dir in self._dirs
            or any(parentdir in self._fileset for parentdir in util.finddirs(dir))
        )

    def prefix(self):
        return self._prefix

    def __repr__(self):
        return "<patternmatcher patterns=%r>" % self._pats


class includematcher(basematcher):
    def __init__(self, root, cwd, kindpats, ctx=None, badfn=None):
        super(includematcher, self).__init__(root, cwd, badfn)

        self._pats, self.matchfn = _buildmatch(ctx, kindpats, "(?:/|$)", root)
        # prefix is True if all patterns are recursive, so certain fast paths
        # can be enabled. Unfortunately, it's too easy to break it (ex. by
        # using "glob:*.c", "re:...", etc).
        self._prefix = _prefix(kindpats)
        roots, dirs = _rootsanddirs(kindpats)
        # roots are directories which are recursively included.
        # If self._prefix is True, then _roots can have a fast path for
        # visitdir to return "all", marking things included unconditionally.
        # If self._prefix is False, then that optimization is unsound because
        # "roots" might contain entries that is not recursive (ex. roots will
        # include "foo/bar" for pattern "glob:foo/bar/*.c").
        self._roots = set(roots)
        # dirs are directories which are non-recursively included.
        # That is, files under that directory are included. But not
        # subdirectories.
        self._dirs = set(dirs)
        # Try to use a more efficient visitdir implementation
        visitdir = _buildvisitdir(kindpats)
        if visitdir:
            self.visitdir = visitdir

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        if self._prefix and dir in self._roots:
            return "all"
        return (
            dir in self._roots
            or dir in self._dirs
            or any(parentdir in self._roots for parentdir in util.finddirs(dir))
        )

    def __repr__(self):
        return "<includematcher includes=%r>" % self._pats


def _buildpatternmatcher(
    root, cwd, kindpats, ctx=None, badfn=None, fallbackmatcher=patternmatcher
):
    """This is a factory function for creating different pattern matchers.

    1. If all patterns can be converted globs and regexs, we will try to
       use either treematcher, regexmatcher or union of them.
    2. Fallback to fallbackmatcher.

    >>> import os
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', 'fbcode/.*', '')])
    <regexmatcher pattern='(?:fbcode/.*)'>
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', 'fbcode/.*', ''), ('glob', 'fbandroid/**', '')])
    <unionmatcher matchers=[<treematcher rules=['fbandroid/**']>, <regexmatcher pattern='(?:fbcode/.*)'>]>
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', 'a/.*', '')])
    <regexmatcher pattern='(?:a/.*)'>
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', 'a/.*', ''), ('glob', 'b/**', '')])
    <unionmatcher matchers=[<treematcher rules=['b/**']>, <regexmatcher pattern='(?:a/.*)'>]>
    >>> _buildpatternmatcher(os.getcwd(), "", [('glob', 'b/**', '')])
    <treematcher rules=['b/**']>
    >>> _buildpatternmatcher(os.getcwd(), "", [])
    <nevermatcher>

    # includematcher
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', 'a/.*', ''), ('glob', 'b', '')], fallbackmatcher=includematcher)
    <unionmatcher matchers=[<treematcher rules=['b/**']>, <regexmatcher pattern='(?:a/.*)'>]>

    # regexmatcher supports '^'
    >>> _buildpatternmatcher(os.getcwd(), "", [('re', '^abc', '')])
    <regexmatcher pattern='(?:^abc)'>

    # treematcher doesn't support large glob patterns, fallback to patternmatcher
    >>> kindpats  = [("glob", f"a/b/*/c/d/e/f/g/{i}/**", "") for i in range(10000)]
    >>> p = _buildpatternmatcher(os.getcwd(), "", kindpats)
    >>> isinstance(p, patternmatcher)
    True
    """
    # kindpats are already normalized to be relative to repo-root.

    # 1
    if _usetreematcher or _useregexmatcher:
        isincludematcher = fallbackmatcher is includematcher
        res = _kindpatstoglobsregexs(kindpats, recursive=isincludematcher)
        if res:
            globs, regexs = res
            try:
                m1 = _buildtreematcher(root, cwd, globs, badfn=badfn)
                m2 = _buildregexmatcher(root, cwd, regexs, badfn=badfn)
            except (error.UncategorizedNativeError, ValueError):
                # just fallback to patternmatcher.
                # possible exceptions:
                #   * disable XXX matcher, but their coresponding pats are not empty
                #   * treematcher: Regex("Compiled regex exceeds size limit of 10485760 bytes.")
                #   * regexmatcher: doesn't support '\b' etc.
                pass
            else:
                m = union([m1, m2], root, cwd)
                # includematcher are only for filtering files, so we skip explicit files for it
                if not isincludematcher:
                    m._files = _explicitfiles(kindpats)
                return m

    # 2
    return fallbackmatcher(root, cwd, kindpats, ctx=ctx, badfn=badfn)


def _buildtreematcher(root, cwd, rules, badfn) -> Optional[treematcher]:
    """build treematcher.

    >>> import os
    >>> _buildtreematcher(os.getcwd(), '', ['a**'], '')
    <treematcher rules=['a**']>

    >>> rules = ['a/b/*/c/d/e/f/g/%s/**' % i for i in range(10000)]
    >>> try:
    ...     _buildtreematcher(os.getcwd(), '', rules, None)
    ... except error.UncategorizedNativeError as e:
    ...     print("got expected exception")
    got expected exception
    """
    if not _usetreematcher and rules:
        raise ValueError("disabled treematcher, but rules is not empty")
    return treematcher(root, cwd, rules=rules, badfn=badfn) if rules else None


def _buildregexmatcher(root, cwd, regexs, badfn) -> Optional[regexmatcher]:
    """build regexmatcher.

    >>> import os
    >>> _buildregexmatcher(os.getcwd(), '', ['a/.*'], '')
    <regexmatcher pattern='(?:a/.*)'>

    >>> _buildregexmatcher(os.getcwd(), '', ['^abc'], '')
    <regexmatcher pattern='(?:^abc)'>
    """
    if not _useregexmatcher and regexs:
        raise ValueError("disabled regexmatcher, but regexs is not empty")
    pattern = f"(?:{'|'.join(regexs)})"
    return regexmatcher(root, cwd, pattern, badfn) if regexs else None


class exactmatcher(basematcher):
    """Matches the input files exactly. They are interpreted as paths, not
    patterns (so no kind-prefixes).
    """

    def __init__(self, root, cwd, files, badfn=None):
        super(exactmatcher, self).__init__(root, cwd, badfn)

        if isinstance(files, list):
            self._files = files
        else:
            self._files = list(files)

    matchfn = basematcher.exact

    @propertycache
    def _dirs(self):
        return set(util.dirs(self._fileset))

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        return dir in self._dirs

    def isexact(self):
        return True

    def __repr__(self):
        return "<exactmatcher files=%r>" % self._files


class differencematcher(basematcher):
    """Composes two matchers by matching if the first matches and the second
    does not. Well, almost... If the user provides a pattern like "-X foo foo",
    Mercurial actually does match "foo" against that. That's because exact
    matches are treated specially. So, since this differencematcher is used for
    excludes, it needs to special-case exact matching.

    The second matcher's non-matching-attributes (root, cwd, bad, traversedir)
    are ignored.

    TODO: If we want to keep the behavior described above for exact matches, we
    should consider instead treating the above case something like this:
    union(exact(foo), difference(pattern(foo), include(foo)))
    """

    def __init__(self, m1, m2):
        super(differencematcher, self).__init__(m1._root, m1._cwd)
        self._m1 = m1
        self._m2 = m2
        self.bad = m1.bad
        self.traversedir = m1.traversedir

    def matchfn(self, f):
        return self._m1(f) and (not self._m2(f) or self._m1.exact(f))

    @propertycache
    def _files(self):
        if self.isexact():
            return [f for f in self._m1.files() if self(f)]
        # If m1 is not an exact matcher, we can't easily figure out the set of
        # files, because its files() are not always files. For example, if
        # m1 is "path:dir" and m2 is "rootfileins:.", we don't
        # want to remove "dir" from the set even though it would match m2,
        # because the "dir" in m1 may not be a file.
        return self._m1.files()

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        if not self._m2.visitdir(dir):
            return self._m1.visitdir(dir)

        if self._m2.visitdir(dir) == "all":
            # There's a bug here: If m1 matches file 'dir/file' and m2 excludes
            # 'dir' (recursively), we should still visit 'dir' due to the
            # exception we have for exact matches.
            return False
        return bool(self._m1.visitdir(dir))

    def isexact(self):
        return self._m1.isexact()

    def __repr__(self):
        return "<differencematcher m1=%r, m2=%r>" % (self._m1, self._m2)


def intersectmatchers(m1, m2):
    """Composes two matchers by matching if both of them match.

    The second matcher's non-matching-attributes (root, cwd, bad, traversedir)
    are ignored.
    """
    if m1 is None or m2 is None:
        return m1 or m2
    if m1.always():
        m = copy.copy(m2)
        # TODO: Consider encapsulating these things in a class so there's only
        # one thing to copy from m1.
        m.bad = m1.bad
        m.traversedir = m1.traversedir
        m.abs = m1.abs
        m.rel = m1.rel
        m._relativeuipath |= m1._relativeuipath
        return m
    if m2.always():
        m = copy.copy(m1)
        m._relativeuipath |= m2._relativeuipath
        return m
    return intersectionmatcher(m1, m2)


class intersectionmatcher(basematcher):
    def __init__(self, m1, m2):
        super(intersectionmatcher, self).__init__(m1._root, m1._cwd)
        self._m1 = m1
        self._m2 = m2
        self.bad = m1.bad
        self.traversedir = m1.traversedir

    @propertycache
    def _files(self):
        if self.isexact():
            m1, m2 = self._m1, self._m2
            if not m1.isexact():
                m1, m2 = m2, m1
            return [f for f in m1.files() if m2(f)]
        # It neither m1 nor m2 is an exact matcher, we can't easily intersect
        # the set of files, because their files() are not always files. For
        # example, if intersecting a matcher "-I glob:foo.txt" with matcher of
        # "path:dir2", we don't want to remove "dir2" from the set.
        return self._m1.files() + self._m2.files()

    def matchfn(self, f):
        return self._m1(f) and self._m2(f)

    def visitdir(self, dir):
        dir = normalizerootdir(dir, "visitdir")
        visit1 = self._m1.visitdir(dir)
        if visit1 == "all":
            return self._m2.visitdir(dir)
        # bool() because visit1=True + visit2='all' should not be 'all'
        return bool(visit1 and self._m2.visitdir(dir))

    def always(self):
        return self._m1.always() and self._m2.always()

    def isexact(self):
        return self._m1.isexact() or self._m2.isexact()

    def __repr__(self):
        return "<intersectionmatcher m1=%r, m2=%r>" % (self._m1, self._m2)


class unionmatcher(basematcher):
    """A matcher that is the union of several matchers.

    The non-matching-attributes (root, cwd, bad, traversedir) are
    taken from the first matcher.
    """

    def __init__(self, matchers):
        m1 = matchers[0]
        super(unionmatcher, self).__init__(m1._root, m1._cwd)
        self.traversedir = m1.traversedir
        self._matchers = matchers

    def matchfn(self, f):
        for match in self._matchers:
            if match(f):
                return True
        return False

    def visitdir(self, dir):
        r = False
        for m in self._matchers:
            v = m.visitdir(dir)
            if v == "all":
                return v
            r |= v
        return r

    def explain(self, f):
        include_explains = []
        exclude_explains = []
        for match in self._matchers:
            explanation = match.explain(f)
            if explanation:
                if match(f):
                    include_explains.append(explanation)
                else:
                    exclude_explains.append(explanation)
        if include_explains:
            summary = "\n".join(include_explains)
            if exclude_explains:
                exclude_summary = "\n".join(
                    f"{e} (overridden by rules above)" for e in exclude_explains
                )
                summary += "\n" + exclude_summary
            return summary
        elif exclude_explains:
            exclude_summary = "\n".join(exclude_explains)
            return exclude_summary
        else:
            return None

    def __repr__(self):
        return "<unionmatcher matchers=%r>" % self._matchers


class xormatcher(basematcher):
    """A matcher that is the xor of two matchers i.e. match returns true if there's at least
    one false and one true.

    The non-matching-attributes (root, cwd, bad, traversedir) are
    taken from the first matcher.
    """

    def __init__(self, m1, m2):
        super(xormatcher, self).__init__(m1._root, m1._cwd)
        self.traversedir = m1.traversedir
        self.m1 = m1
        self.m2 = m2

    def matchfn(self, f):
        return bool(self.m1(f)) ^ bool(self.m2(f))

    def visitdir(self, dir):
        m1dir = self.m1.visitdir(dir)
        m2dir = self.m2.visitdir(dir)

        # if both matchers return "all" then we know for sure we don't need
        # to visit this directory. Same if all matchers return False. In all
        # other case we have to visit a directory.
        if m1dir == "all" and m2dir == "all":
            return False
        if not m1dir and not m2dir:
            return False
        return True

    def __repr__(self):
        return "<xormatcher matchers=%r>" % self._matchers


def patkind(pattern, default=None):
    """If pattern is 'kind:pat' with a known kind, return kind."""
    return _patsplit(pattern, default)[0]


def _patsplit(pattern, default):
    """Split a string into the optional pattern kind prefix and the actual
    pattern."""
    if ":" in pattern:
        kind, pat = pattern.split(":", 1)
        if kind in allpatternkinds:
            return kind, pat
    return default, pattern


def _globre(pat: Sized) -> str:
    r"""Convert an extended glob string to a regexp string.

    >>> from . import pycompat
    >>> def bprint(s):
    ...     print(s)
    >>> bprint(_globre(r'?'))
    .
    >>> bprint(_globre(r'*'))
    [^/]*
    >>> bprint(_globre(r'**'))
    .*
    >>> bprint(_globre(r'**/a'))
    (?:.*/)?a
    >>> bprint(_globre(r'a/**/b'))
    a/(?:.*/)?b
    >>> bprint(_globre(r'[a*?!^][^b][!c]'))
    [a*?!^][\^b][^c]
    >>> bprint(_globre(r'{a,b}'))
    (?:a|b)
    >>> bprint(_globre(r'.\*\?'))
    \.\*\?
    """
    i, n = 0, len(pat)
    res = ""
    group = 0
    escape = util.re.escape

    def peek():
        return i < n and pat[i : i + 1]

    while i < n:
        # pyre-fixme[16]: `Sized` has no attribute `__getitem__`.
        c = pat[i : i + 1]
        i += 1
        if c not in "*?[{},\\":
            res += escape(c)
        elif c == "*":
            if peek() == "*":
                i += 1
                if peek() == "/":
                    i += 1
                    res += "(?:.*/)?"
                else:
                    res += ".*"
            else:
                res += "[^/]*"
        elif c == "?":
            res += "."
        elif c == "[":
            j = i
            if j < n and pat[j : j + 1] in "!]":
                j += 1
            while j < n and pat[j : j + 1] != "]":
                j += 1
            if j >= n:
                res += "\\["
            else:
                stuff = pat[i:j].replace("\\", "\\\\")
                i = j + 1
                if stuff[0:1] == "!":
                    stuff = "^" + stuff[1:]
                elif stuff[0:1] == "^":
                    stuff = "\\" + stuff
                res = "%s[%s]" % (res, stuff)
        elif c == "{":
            group += 1
            res += "(?:"
        elif c == "}" and group:
            res += ")"
            group -= 1
        elif c == "," and group:
            res += "|"
        elif c == "\\":
            p = peek()
            if p:
                i += 1
                res += escape(p)
            else:
                res += escape(c)
        else:
            res += escape(c)
    return res


def _regex(kind, pat, globsuffix):
    """Convert a (normalized) pattern of any kind into a regular expression.
    globsuffix is appended to the regexp of globs."""
    if not pat and kind in ("glob", "relpath"):
        return ""
    if kind == "re":
        return pat
    if kind in ("path", "relpath"):
        if pat == ".":
            return ""
        return util.re.escape(pat) + "(?:/|$)"
    if kind == "rootfilesin":
        if pat == ".":
            escaped = ""
        else:
            # Pattern is a directory name.
            escaped = util.re.escape(pat) + "/"
        # Anything after the pattern must be a non-directory.
        return escaped + "[^/]+$"
    if kind == "relglob":
        return "(?:|.*/)" + _globre(pat) + globsuffix
    if kind == "relre":
        if pat.startswith("^"):
            return pat
        return ".*" + pat
    return _globre(pat) + globsuffix


def _buildmatch(ctx, kindpats, globsuffix, root):
    """Return regexp string and a matcher function for kindpats.
    globsuffix is appended to the regexp of globs."""
    matchfuncs = []

    fset, kindpats = _expandsets(kindpats, ctx)
    if fset:
        matchfuncs.append(fset.__contains__)

    regex = ""
    if kindpats:
        regex, mf = _buildregexmatch(kindpats, globsuffix)
        matchfuncs.append(mf)

    if len(matchfuncs) == 1:
        return regex, matchfuncs[0]
    else:
        return regex, lambda f: any(mf(f) for mf in matchfuncs)


def _buildregexmatch(kindpats: List, globsuffix):
    """Build a match function from a list of kinds and kindpats,
    return regexp string and a matcher function."""
    regex = _buildregex(kindpats, globsuffix)
    try:
        if len(regex) > MAX_RE_SIZE:
            raise OverflowError
        return regex, _rematcher(regex)
    except OverflowError:
        # We're using a Python with a tiny regex engine and we
        # made it explode, so we'll divide the pattern list in two
        # until it works
        l = len(kindpats)
        if l < 2:
            raise
        regexa, a = _buildregexmatch(kindpats[: l // 2], globsuffix)
        regexb, b = _buildregexmatch(kindpats[l // 2 :], globsuffix)
        return regex, lambda s: a(s) or b(s)
    except re.error:
        for k, p, s in kindpats:
            try:
                _rematcher("(?:%s)" % _regex(k, p, globsuffix))
            except re.error:
                if s:
                    raise error.Abort(_("%s: invalid pattern (%s): %s") % (s, k, p))
                else:
                    raise error.Abort(_("invalid pattern (%s): %s") % (k, p))
        raise error.Abort(_("invalid pattern"))


def _buildregex(kindpats: List, globsuffix: str) -> str:
    """Convert a (normalized) patterns of any kind into a regular expression.

    globsuffix is appended to the regexp of globs.
    """
    return "(?:%s)" % "|".join([_regex(k, p, globsuffix) for (k, p, _) in kindpats])


def _patternrootsanddirs(kindpats):
    """Returns roots and directories corresponding to each pattern.

    This calculates the roots and directories exactly matching the patterns and
    returns a tuple of (roots, dirs) for each. It does not return other
    directories which may also need to be considered, like the parent
    directories.
    """
    r = []
    d = []
    for kind, pat, source in kindpats:
        if kind == "glob":  # find the non-glob prefix
            root = []
            for p in pat.split("/"):
                if "[" in p or "{" in p or "*" in p or "?" in p:
                    break
                root.append(p)
            r.append("/".join(root))
        elif kind in ("relpath", "path"):
            if pat == ".":
                pat = ""
            r.append(pat)
        elif kind in ("rootfilesin",):
            if pat == ".":
                pat = ""
            d.append(pat)
        else:  # relglob, re, relre
            r.append("")
    return r, d


def _roots(kindpats):
    """Returns root directories to match recursively from the given patterns."""
    roots, dirs = _patternrootsanddirs(kindpats)
    return roots


def _rootsanddirs(kindpats):
    """Returns roots and exact directories from patterns.

    roots are directories to match recursively, whereas exact directories should
    be matched non-recursively. The returned (roots, dirs) tuple will also
    include directories that need to be implicitly considered as either, such as
    parent directories.

    >>> _rootsanddirs(
    ...     [('glob', 'g/h/*', ''), ('glob', 'g/h', ''),
    ...      ('glob', 'g*', '')])
    (['g/h', 'g/h', ''], ['', 'g'])
    >>> _rootsanddirs(
    ...     [('rootfilesin', 'g/h', ''), ('rootfilesin', '', '')])
    ([], ['g/h', '', '', 'g'])
    >>> _rootsanddirs(
    ...     [('relpath', 'r', ''), ('path', 'p/p', ''),
    ...      ('path', '', '')])
    (['r', 'p/p', ''], ['', 'p'])
    >>> _rootsanddirs(
    ...     [('relglob', 'rg*', ''), ('re', 're/', ''),
    ...      ('relre', 'rr', '')])
    (['', '', ''], [''])
    """
    r, d = _patternrootsanddirs(kindpats)

    # Append the parents as non-recursive/exact directories, since they must be
    # scanned to get to either the roots or the other exact directories.
    d.extend(sorted(util.dirs(d)))
    d.extend(sorted(util.dirs(r)))

    return r, d


def _explicitfiles(kindpats):
    """Returns the potential explicit filenames from the patterns.

    >>> _explicitfiles([('path', 'foo/bar', '')])
    ['foo/bar']
    >>> _explicitfiles([('rootfilesin', 'foo/bar', '')])
    []
    """
    # Keep only the pattern kinds where one can specify filenames (vs only
    # directory names).
    filable = [kp for kp in kindpats if kp[0] not in ("rootfilesin",)]
    return _roots(filable)


def _prefix(kindpats) -> bool:
    """Whether all the patterns match a prefix (i.e. recursively)"""
    for kind, pat, source in kindpats:
        if kind not in ("path", "relpath"):
            return False
    return True


_usetreematcher = True
_useregexmatcher = True
_usedynmatcher = True
_emptyglobalwaysmatches = False
_userustmatcher = False


def init(ui) -> None:
    global _usetreematcher
    _usetreematcher = ui.configbool("experimental", "treematcher")
    global _useregexmatcher
    _useregexmatcher = ui.configbool("experimental", "regexmatcher")
    global _usedynmatcher
    _usedynmatcher = ui.configbool("experimental", "dynmatcher")
    global _emptyglobalwaysmatches
    _emptyglobalwaysmatches = ui.configbool("experimental", "empty-glob-always-matches")
    global _userustmatcher
    _userustmatcher = ui.configbool("experimental", "rustmatcher")
