# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License version 2.

CACHEDIR="$TESTTMP/hgcache"
export DUMMYSSH_STABLE_ORDER=1
cat >> $HGRCPATH <<EOF
[remotefilelog]
cachepath=$CACHEDIR
debug=True
[extensions]
remotefilelog=
rebase=
[ui]
ssh=$(dummysshcmd)
[server]
preferuncompressed=True
[experimental]
changegroup3=True
[rebase]
singletransaction=True
EOF

_cachecount=0

newcachedir() {
  _cachecount=$((_cachecount+1))
  CACHEDIR="$TESTTMP/hgcache$_cachecount"
  setconfig remotefilelog.cachepath="$CACHEDIR"
}

hgcloneshallow() {
  local name
  local dest
  orig=$1
  shift
  dest=$1
  shift
  hg clone --shallow --config remotefilelog.reponame=master $orig $dest $@
  cat >> $dest/.hg/hgrc <<EOF
[remotefilelog]
reponame=master
[phases]
publish=False
EOF
}

hgcloneshallowlfs() {
  local name
  local dest
  local lfsdir
  orig=$1
  shift
  dest=$1
  shift
  lfsdir=$1
  shift
  hg clone --shallow --config "extensions.lfs=" --config "lfs.url=$lfsdir" --config remotefilelog.reponame=master $orig $dest $@
  cat >> $dest/.hg/hgrc <<EOF
[extensions]
lfs=
[lfs]
url=$lfsdir
[remotefilelog]
reponame=master
[phases]
publish=False
EOF
}

hginit() {
  local name
  name=$1
  shift
  hg init $name $@ --config remotefilelog.reponame=master
}

clearcache() {
  rm -rf $CACHEDIR/*
}

mkcommit() {
  echo "$1" > "$1"
  hg add "$1"
  hg ci -m "$1"
}

ls_l() {
  $PYTHON $TESTDIR/ls-l.py "$@"
}

findfilessorted() {
  find "$1" -type f | sort
}

getmysqldb() {
  source "$TESTDIR/hgsql/library.sh"
}

createpushrebaserecordingdb() {
mysql -h $DBHOST -P $DBPORT -u $DBUSER $DBPASSOPT -e "CREATE DATABASE IF NOT EXISTS $DBNAME;" 2>/dev/null
mysql -h $DBHOST -P $DBPORT -D $DBNAME -u $DBUSER $DBPASSOPT <<EOF
DROP TABLE IF EXISTS pushrebaserecording;
$(cat $TESTDIR/pushrebase_replay_schema.sql)
EOF
}
