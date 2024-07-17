# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

from __future__ import absolute_import

# Standard Library
import gzip
import socket
import ssl
import time

from bindings import clientinfo as clientinfomod

from sapling import error, httpclient, httpconnection, json, perftrace, pycompat, util
from sapling.i18n import _

from . import baseservice, error as ccerror, util as ccutil


# pyre-fixme[11]: Annotation `client` is not defined as a type.
httplib = util.httplib

try:
    range
except NameError:
    range = range

# clean up helper (to use with json.dumps)
# filter out the fields with None and empty arrays / maps


def cleandict(d):
    if not isinstance(d, dict):
        return d
    return dict(
        (k, cleandict(v))
        for k, v in pycompat.iteritems(d)
        if (v is not None and not (hasattr(v, "__len__") and len(v) == 0))
    )


DEFAULT_TIMEOUT = 180
MAX_CONNECT_RETRIES = 2


class _HttpsCommitCloudService(baseservice.BaseService):
    """Commit Cloud Client uses http endpoint to communicate with
    the commit cloud service
    """

    def __init__(self, ui):
        self.ui = ui
        self.debugrequests = ui.config("commitcloud", "debugrequests")
        self.url = ui.config("commitcloud", "url")
        self._sockettimeout = DEFAULT_TIMEOUT
        self.user_agent = "Sapling {}".format(util.version())
        self._unix_socket_proxy = (
            ui.config("auth_proxy", "unix_socket_path")
            if ui.config("auth_proxy", "commitcloud_use_uds")
            else None
        )

        if self._unix_socket_proxy:
            self.user_agent += "+x2pagentd"

        if not self.url:
            raise ccerror.ConfigurationError(
                self.ui, _("'commitcloud.url' is required")
            )

        self._setupconnection()

    def _setupconnection(self):
        # setting up HTTP(S) connection

        # enable client side compression
        # data in the response is also requested compressed
        self.headers = {
            "Connection": "keep-alive",
            "Content-Type": "application/json",
            "Accept-Encoding": "none, gzip",
            "Content-Encoding": "gzip",
            "User-Agent": self.user_agent,
            "X-Client-Info": clientinfomod.clientinfo().to_json().decode(),
        }

        u = util.url(self.url, parsequery=False, parsefragment=False)

        if u.scheme != "https" or not u.host or u.passwd is not None:
            raise ccerror.ConfigurationError(
                self.ui, _("'commitcloud.url' is invalid or unsupported")
            )

        if self._unix_socket_proxy:
            u.scheme = "http"

        remotehost = u.host
        remoteport = int(u.port) if u.port else 443

        sslcontext = ssl.create_default_context()

        # use the same TLS auth to connect to the Commit Cloud service
        # as it is used to connect to the default path
        if not self._unix_socket_proxy:
            path = ccutil.getremotepath(self.ui)
            authdata = httpconnection.readauthforuri(self.ui, path, u.user)
            if authdata:
                (authname, auth) = authdata
                cert = auth.get("cert")
                key = auth.get("key")
                cacerts = auth.get("cacerts")
                sslcontext.load_cert_chain(cert, keyfile=key)
                if cacerts:
                    sslcontext.load_verify_locations(cacerts)
            else:
                raise ccerror.TLSConfigurationError(
                    self.ui,
                    _(
                        "no certificates have been found to connect to the Commit Cloud Service"
                    ),
                )

        # Use UNIX SOCKECT connection to x2pagentd if available
        if self._unix_socket_proxy:
            self.connection = httpclient.HTTPConnection(
                remotehost,
                unix_socket_path=self._unix_socket_proxy,
                timeout=self._sockettimeout,
            )
            self.ui.debug(
                "will be connecting to %s using x2pagentd\n" % (remotehost),
                component="commitcloud",
            )
        else:
            self.connection = httpclient.HTTPConnection(
                remotehost,
                remoteport,
                timeout=DEFAULT_TIMEOUT,
                use_ssl=True,
                ssl_wrap_socket=sslcontext.wrap_socket,
            )

            self.ui.debug(
                "will be connecting to %s:%d\n" % (remotehost, remoteport),
                component="commitcloud",
            )

    def _getheader(self, s):
        return self.headers.get(s)

    def _send(self, path, data):
        lastretriableex = None
        rdata = None
        # print request if debugrequests and debug are both on
        if self.debugrequests:
            self.ui.debug("%s\n" % json.dumps(cleandict(data)))
        if self._getheader("Content-Encoding") == "gzip":
            buffer = util.stringio()
            with gzip.GzipFile(fileobj=buffer, mode="w") as compressed:
                compressed.write(pycompat.encodeutf8(json.dumps(data)))
                compressed.flush()
            rdata = buffer.getvalue()
        else:
            rdata = pycompat.encodeutf8(json.dumps(data))

        # exponential backoff here on failure, 1s, 2s, 4s, 8s, 16s etc
        sl = 1

        for attempt in range(MAX_CONNECT_RETRIES):
            try:
                self.connection.request("POST", path, rdata, self.headers)
                resp = self.connection.getresponse()

                if resp.status == int(httplib.UNAUTHORIZED):
                    raise ccerror.RegistrationError(self.ui, _("unauthorized client"))
                if resp.status == int(httplib.FORBIDDEN):
                    raise ccerror.RegistrationError(self.ui, _("forbidden client"))
                if resp.status == int(httplib.BAD_REQUEST):
                    raise ccerror.BadRequestError(self.ui, resp.reason.decode("utf-8"))
                if resp.status != int(httplib.OK):
                    raise ccerror.ServiceError(
                        self.ui, "%d %s" % (resp.status, resp.reason.decode("utf-8"))
                    )
                if resp.getheader("Content-Encoding") == "gzip":
                    resp = gzip.GzipFile(fileobj=util.stringio(resp.read()))
                data = json.load(resp)
                # print response if debugrequests and debug are both on
                if self.debugrequests:
                    self.ui.debug("%s\n" % json.dumps(cleandict(data)))
                if "error" in data:
                    raise ccerror.ServiceError(self.ui, data["error"])
                return data
            except httplib.HTTPException as e:
                lastretriableex = e
                self.connection.close()
            except (socket.timeout, socket.gaierror) as e:
                raise error.Abort(
                    _("network error: %s") % e, hint=_("check your network connection")
                )
            except socket.error as e:
                if "SSL" in str(e):
                    raise ccerror.TLSAccessError(self.ui, str(e))
                raise ccerror.ServiceError(self.ui, str(e))
            except ssl.CertificateError as e:
                raise ccerror.TLSAccessError(self.ui, str(e))
            time.sleep(sl)
            sl *= 2

        # Control flow can only end up here if we have failed all retries.
        raise ccerror.ServiceError(
            self.ui,
            "Failed after {} tries. {}".format(
                MAX_CONNECT_RETRIES, str(lastretriableex)
            ),
        )

    def _timedsend(self, path, data):
        start = util.timer()
        response = self._send(path, data)
        elapsed = util.timer() - start
        self.ui.debug(
            "response received in %0.2f sec\n" % elapsed, component="commitcloud"
        )
        return response

    def _addheads(self, refs):
        """Create list of heads from head_dates data

        Server may omit heads to reduce data transmission.
        """
        if not refs.get("heads") and refs.get("head_dates"):
            refs["heads"] = refs.get("head_dates", {}).keys()
        return refs

    @perftrace.tracefunc("Check Commit Cloud Authentication")
    def check(self):
        # send a check request.  Currently this is an empty 'get_references'
        # request, which asks for the latest version of workspace '' for repo
        # ''.  That always returns a valid response indicating there is no
        # workspace with that name for that repo.
        # TODO: Make this a dedicated request

        self.ui.debug(
            "sending empty 'get_references' request to check authentication\n",
            component="commitcloud",
        )
        path = "/commit_cloud/get_references"
        self._send(path, {})

    @perftrace.tracefunc("Get Commit Cloud References")
    def getreferences(self, reponame, workspace, baseversion, clientinfo=None):
        self.ui.debug("sending 'get_references' request\n", component="commitcloud")

        # send request
        path = "/commit_cloud/get_references"
        data = {
            "base_version": baseversion,
            "repo_name": reponame,
            "workspace": workspace,
        }
        if clientinfo is not None:
            data["client_info"] = clientinfo
        response = self._timedsend(path, data)
        version = response["ref"]["version"]

        if version == 0:
            self.ui.debug(
                "'get_references' returns that workspace '%s' is not known by server\n"
                % workspace,
                component="commitcloud",
            )
            return self._makeemptyreferences(version)

        if version == baseversion:
            self.ui.debug(
                "'get_references' confirms the current version %s is the latest\n"
                % version,
                component="commitcloud",
            )
            return self._makeemptyreferences(version)

        self.ui.debug(
            "'get_references' returns version %s, current version %s\n"
            % (version, baseversion),
            component="commitcloud",
        )
        return self._makereferences(self._addheads(response["ref"]))

    @perftrace.tracefunc("Update Commit Cloud References")
    def updatereferences(
        self,
        reponame,
        workspace,
        version,
        oldheads=None,
        newheads=None,
        oldbookmarks=None,
        newbookmarks=None,
        oldremotebookmarks=None,
        newremotebookmarks=None,
        clientinfo=None,
        logopts={},
    ):
        self.ui.debug("sending 'update_references' request\n", component="commitcloud")
        oldheads = oldheads or []
        newheads = newheads or []
        oldbookmarks = oldbookmarks or []
        newbookmarks = newbookmarks or {}
        oldremotebookmarks = oldremotebookmarks or []
        newremotebookmarks = newremotebookmarks or {}

        # remove duplicates, must preserve order in the newheads list
        newheadsset = set(newheads)
        commonset = set([item for item in oldheads if item in newheadsset])

        newheads = [h for h in newheads if h not in commonset]
        oldheads = [h for h in oldheads if h not in commonset]

        client_correlator = clientinfomod.get_client_correlator().decode()
        client_entry_point = clientinfomod.get_client_entry_point().decode()
        self.ui.log(
            "commitcloud_updates",
            version=version,
            repo=reponame,
            workspace=workspace,
            oldheadcount=len(oldheads),
            newheadcount=len(newheads),
            oldbookmarkcount=len(oldbookmarks),
            newbookmarkcount=len(newbookmarks),
            oldremotebookmarkcount=len(oldremotebookmarks),
            newremotebookmarkcount=len(newremotebookmarks),
            client_correlator=client_correlator,
            client_entry_point=client_entry_point,
            **logopts,
        )

        # send request
        path = "/commit_cloud/update_references"
        data = {
            "version": version,
            "repo_name": reponame,
            "workspace": workspace,
            "removed_heads": oldheads,
            "new_heads": newheads,
            "removed_bookmarks": oldbookmarks,
            "updated_bookmarks": newbookmarks,
            "removed_remote_bookmarks": self._makeremotebookmarks(oldremotebookmarks),
            "updated_remote_bookmarks": self._makeremotebookmarks(newremotebookmarks),
            "removed_snapshots": [],
            "new_snapshots": [],
        }
        if clientinfo is not None:
            data["client_info"] = clientinfo

        response = self._timedsend(path, data)
        data = response["ref"]
        rc = response["rc"]
        newversion = data["version"]

        if rc != 0:
            self.ui.debug(
                "'update_references' rejected update, current version %d is old, "
                "client needs to sync to version %d first\n" % (version, newversion),
                component="commitcloud",
            )
            return False, self._makereferences(self._addheads(data))

        self.ui.debug(
            "'update_references' accepted update, old version is %d, new version is %d\n"
            % (version, newversion),
            component="commitcloud",
        )

        return (
            True,
            self._makeemptyreferences(newversion),
        )

    @perftrace.tracefunc("Get Commit Cloud Smartlog")
    def getsmartlog(self, reponame, workspace, repo, limit, flags=[]):
        self.ui.debug("sending 'get_smartlog' request\n", component="commitcloud")
        path = "/commit_cloud/get_smartlog"
        data = {"repo_name": reponame, "workspace": workspace, "flags": flags}
        response = self._timedsend(path, data)

        # if 200 OK response format is:
        # {
        #   "rc":0,
        #   "smartlog": <thrift structure SmartlogData serialized to json using Thrift JSON serialization>
        # }
        smartlog = response["smartlog"]
        if limit != 0:
            cutoff = int(time.time()) - limit
            smartlog["nodes"] = list(
                filter(lambda x: x["date"] >= cutoff, smartlog["nodes"])
            )
        self.ui.debug(
            "'get_smartlog' returns %d entries\n" % len(smartlog["nodes"]),
            component="commitcloud",
        )

        try:
            return self._makesmartloginfo(smartlog)
        except Exception as e:
            raise ccerror.UnexpectedError(self.ui, e)

    @perftrace.tracefunc("Get Commit Cloud Smartlog By Version")
    def getsmartlogbyversion(
        self, reponame, workspace, repo, date, version, limit, flags=[]
    ):
        self.ui.debug("sending 'get_old_smartlog' request\n", component="commitcloud")
        path = "/commit_cloud/get_smartlog_by_version"
        if date:
            data = {
                "repo_name": reponame,
                "workspace": workspace,
                "timestamp": date[0],
                "flags": flags,
            }
        else:
            data = {
                "repo_name": reponame,
                "workspace": workspace,
                "version": version,
                "flags": flags,
            }

        response = self._timedsend(path, data)

        # if 200 OK response format is:
        # {
        #   "rc":0,
        #   "smartlog": <thrift structure SmartlogData serialized to json using Thrift JSON serialization>
        # }
        smartlog = response["smartlog"]
        if limit != 0:
            cutoff = smartlog["timestamp"] - limit
            smartlog["nodes"] = list(
                filter(lambda x: x["date"] >= cutoff, smartlog["nodes"])
            )

        self.ui.debug(
            "'get_smartlog' returns %d entries\n" % len(smartlog["nodes"]),
            component="commitcloud",
        )

        try:
            return self._makesmartloginfo(smartlog)
        except Exception as e:
            raise ccerror.UnexpectedError(self.ui, e)

    @perftrace.tracefunc("Get list of historical versions")
    def gethistoricalversions(self, reponame, workspace):
        self.ui.debug(
            "sending 'get_historical_versions' request\n", component="commitcloud"
        )
        path = "/commit_cloud/get_historical_versions"
        data = {"repo_name": reponame, "workspace": workspace}

        response = self._timedsend(path, data)
        versions = response["versions"]["versions"]

        self.ui.debug(
            "'get_historical_versions' returns %d entries\n" % len(versions),
            component="commitcloud",
        )

        try:
            return versions
        except Exception as e:
            raise ccerror.UnexpectedError(self.ui, e)

    @perftrace.tracefunc("update checkout locations")
    def updatecheckoutlocations(
        self, reponame, workspace, hostname, commit, checkoutpath, sharedpath, unixname
    ):
        self.ui.debug(
            "sending 'update_checkout_locations' request\n", component="commitcloud"
        )
        path = "/commit_cloud/update_checkout_locations"
        data = {
            "repo_name": reponame,
            "workspace": workspace,
            "hostname": hostname,
            "commit": commit,
            "checkout_path": checkoutpath,
            "shared_path": sharedpath,
            "unixname": unixname,
        }
        self._timedsend(path, data)
        self.ui.debug("'update_checkout_locations' successful", component="commitcloud")

    @perftrace.tracefunc("Get Commit Cloud Workspaces By Prefix")
    def getworkspaces(self, reponame, prefix):
        """Fetch Commit Cloud workspaces for the given prefix"""
        self.ui.debug("sending 'get_workspaces' request\n", component="commitcloud")
        path = "/commit_cloud/get_workspaces"
        data = {"repo_name": reponame, "prefix": prefix}
        response = self._timedsend(path, data)
        workspaces = response["workspaces_data"]
        return self._makeworkspacesinfo(workspaces)

    @perftrace.tracefunc("Get Commit Cloud Workspace By Name")
    def getworkspace(self, reponame, workspacename):
        """Fetch Commit Cloud workspace by its full name"""
        # Use the existing prefix lookup.
        # TODO(liubovd): change to use "/commit_cloud/get_workspace" API once it is fixed
        winfos = self.getworkspaces(reponame, workspacename)
        match = [winfo for winfo in winfos if winfo.name == workspacename]
        if not match:
            return None
        return match[0]

    @perftrace.tracefunc("Archive/Restore Workspace")
    def updateworkspacearchive(self, reponame, workspace, archived):
        """Archive or Restore the given workspace"""
        self.ui.debug(
            "sending 'update_workspace_archive' request\n", component="commitcloud"
        )
        path = "/commit_cloud/update_workspace_archive"
        data = {"repo_name": reponame, "workspace": workspace, "archived": archived}
        self._timedsend(path, data)

    @perftrace.tracefunc("Rename Workspace")
    def renameworkspace(self, reponame, workspace, new_workspace):
        """Rename the given workspace"""
        self.ui.debug("sending 'rename_workspace' request\n", component="commitcloud")
        path = "/commit_cloud/rename_workspace"
        data = {
            "repo_name": reponame,
            "workspace": workspace,
            "new_workspace": new_workspace,
        }
        self._timedsend(path, data)

    @perftrace.tracefunc("Share Workspace")
    def shareworkspace(self, reponame, workspace):
        """Enable sharing for the given workspace"""
        self.ui.debug("sending 'share_workspace' request\n", component="commitcloud")
        path = "/commit_cloud/share_workspace"
        data = {
            "repo_name": reponame,
            "workspace": workspace,
        }
        response = self._timedsend(path, data)
        return response["sharing_data"]

    @perftrace.tracefunc("Rollback Workspace")
    def rollbackworkspace(self, reponame, workspace, version):
        """Rollback the given workspace to a specific version"""
        self.ui.debug("sending 'rollback_workspace' request\n", component="commitcloud")
        path = "/commit_cloud/rollback_workspace"
        data = {
            "repo_name": reponame,
            "workspace": workspace,
            "version": version,
        }
        self._timedsend(path, data)

    @perftrace.tracefunc("Cleanup Workspace")
    def cleanupworkspace(self, reponame, workspace):
        """Cleanup unnecessary remote bookmarks from the given workspace"""
        self.ui.debug("sending 'cleanup_workspace' request\n", component="commitcloud")
        path = "/commit_cloud/cleanup_workspace"
        important_remote_bookmarks = self.ui.configlist(
            "remotenames", "selectivepulldefault"
        )
        expensive_remote_bookmarks = self.ui.configlist(
            "commitcloud", "expensive_bookmarks", []
        )
        data = {
            "repo_name": reponame,
            "workspace": workspace,
            "expensive_bookmarks": expensive_remote_bookmarks,
            "important_bookmarks": important_remote_bookmarks,
            "main_bookmark": important_remote_bookmarks[0],
        }
        self._timedsend(path, data)


# Make sure that the HttpsCommitCloudService is a singleton
HttpsCommitCloudService = baseservice.SingletonDecorator(_HttpsCommitCloudService)
