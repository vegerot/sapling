#debugruntest-compatible
#inprocess-hg-incompatible

  $ eagerepo
Setup

  $ enable phabstatus smartlog
  $ setconfig extensions.arcconfig="$TESTDIR/../sapling/ext/extlib/phabricator/arcconfig.py"
  $ hg init repo
  $ cd repo
  $ touch foo
  $ hg ci -qAm 'Differential Revision: https://phabricator.fb.com/D1'

With an invalid arc configuration

  $ hg log -T '{phabstatus}\n' -r .
  arcconfig configuration problem. No diff information can be provided.
  Error info: no .arcconfig found
  Error

Configure arc...

  $ echo '{}' > .arcrc
  $ echo '{"config" : {"default" : "https://a.com/api"}, "hosts" : {"https://a.com/api/" : { "user" : "testuser", "oauth" : "garbage_cert"}}}' > .arcconfig

And now with bad responses:

  $ cat > $TESTTMP/mockduit << EOF
  > [{}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Error talking to phabricator. No diff information can be provided.
  Error info: Unexpected graphql response format
  Error

  $ cat > $TESTTMP/mockduit << EOF
  > [{"errors": [{"message": "failed, yo"}]}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Error talking to phabricator. No diff information can be provided.
  Error info: failed, yo
  Error

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": null}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Error talking to phabricator. No diff information can be provided.
  Error info: Unexpected graphql response format
  Error

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": null}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Error talking to phabricator. No diff information can be provided.
  Error info: Unexpected graphql response format
  Error

Missing status field is treated as an error
  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "created_time": 0, "updated_time": 2}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Error talking to phabricator. No diff information can be provided.
  Error info: Unexpected graphql response format
  Error

If the diff is landing, show "Landing" in place of the status name

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Accepted",
  >    "created_time": 0, "updated_time": 2, "is_landing": true,
  >    "land_job_status": "LAND_JOB_RUNNING",
  >    "needs_final_review_status": "NOT_NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Landing

If the diff has landed, but Phabricator hasn't parsed it yet, show "Committing"
in place of the status name

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Accepted",
  >    "created_time": 0, "updated_time": 2, "is_landing": true,
  >    "land_job_status": "LAND_RECENTLY_SUCCEEDED",
  >    "needs_final_review_status": "NOT_NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Committing

If the diff recently failed to land, show "Recently Failed to Land"

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Accepted",
  >    "created_time": 0, "updated_time": 2, "is_landing": true,
  >    "land_job_status": "LAND_RECENTLY_FAILED",
  >    "needs_final_review_status": "NOT_NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Recently Failed to Land

If the diff needs a final review, show "Needs Final Review"

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Accepted",
  >    "created_time": 0, "updated_time": 2, "is_landing": true,
  >    "land_job_status": "NO_LAND_RUNNING",
  >    "needs_final_review_status": "NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Needs Final Review

And finally, the success case

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Needs Review",
  >    "created_time": 0, "updated_time": 2, "is_landing": false,
  >    "land_job_status": "NO_LAND_RUNNING",
  >    "needs_final_review_status": "NOT_NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg log -T '{phabstatus}\n' -r .
  Needs Review

Make sure the code works without the smartlog extensions

  $ cat > $TESTTMP/mockduit << EOF
  > [{"data": {"query": [{"results": {"nodes": [
  >   {"number": 1, "diff_status_name": "Needs Review",
  >    "created_time": 0, "updated_time": 2, "is_landing": false,
  >    "land_job_status": "NO_LAND_RUNNING",
  >    "needs_final_review_status": "NOT_NEEDED"}
  > ]}}]}}]
  > EOF
  $ HG_ARC_CONDUIT_MOCK=$TESTTMP/mockduit hg --config 'extensions.smartlog=!' log -T '{phabstatus}\n' -r .
  Needs Review

Make sure the template keywords are documented correctly

  $ hg help templates | egrep 'phabstatus|syncstatus'
      phabstatus    String. Return the diff approval status for a given hg rev
      syncstatus    String. Return whether the local revision is in sync with

Make sure we get decent error messages when .arcrc is missing credential
information.  We intentionally do not use HG_ARC_CONDUIT_MOCK for this test,
so it tries to parse the (empty) arc config files.

  $ echo '{}' > .arcrc
  $ echo '{}' > .arcconfig
  $ hg log -T '{phabstatus}\n' -r .
  arcconfig configuration problem. No diff information can be provided.
  Error info: arcrc is missing user credentials. Use "jf authenticate" to fix, or ensure you are prepping your arcrc properly.
  Error

Make sure we get an error message if .arcrc is not proper JSON (for example
due to trailing commas). We do not use HG_ARC_CONDUIT_MOCK for this test,
in order for it to parse the badly formatted arc config file.

  $ echo '{,}' > ../.arcrc
  $ hg log -T '{phabstatus}\n' -r .
  arcconfig configuration problem. No diff information can be provided.
  Error info: Configuration file $TESTTMP/.arcrc is not a proper JSON file.
  Error
