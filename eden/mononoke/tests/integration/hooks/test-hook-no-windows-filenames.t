# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ export LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8 LANGUAGE=en_US.UTF-8

  $ hook_test_setup no_windows_filenames <( \
  >   cat <<CONF
  > bypass_pushvar="ALLOW_BAD_WINDOWS_FILENAMES=true"
  > config_json='''{
  >   "allowed_paths": "^fbcode/videoinfra|^fbcode/transient_analysis|^fbcode/tupperware|^fbcode/realtime|^fbcode/npe|^fbcode/axon|^fbcode/ame|^third-party/rpms|^opsfiles/|^fbobjc/Libraries/Lexical/",
  >   "illegal_filename_message": "ABORT: Illegal windows filename: \${filename}. Name and path of file in windows should not match regex \${illegal_pattern}"  
  > }'''
  > CONF
  > ) 

  $ hg up -q "min(all())"
  $ echo "ok"  > "com"
  $ hg ci -Aqm success
  $ hgmn push -r . --to master_bookmark
  pushing rev 2bdf0e02c487 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark

  $ hg up -q "min(all())"
  $ echo "bad" > "COM5"
  $ hg ci -Aqm failure
  warning: filename contains 'COM5', which is reserved on Windows: COM5
  $ hgmn push -r . --to master_bookmark
  pushing rev 0a31cb8056d1 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     hooks failed:
  remote:     no_windows_filenames for 0a31cb8056d10d69d6652e754aeee9ecdd5f9e7b: ABORT: Illegal windows filename: COM5. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Root cause:
  remote:     hooks failed:
  remote:     no_windows_filenames for 0a31cb8056d10d69d6652e754aeee9ecdd5f9e7b: ABORT: Illegal windows filename: COM5. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Debug context:
  remote:     "hooks failed:\nno_windows_filenames for 0a31cb8056d10d69d6652e754aeee9ecdd5f9e7b: ABORT: Illegal windows filename: COM5. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\\d)|con|prn|aux|nul))($|\\.))|<|>|:|\"|/|\\\\|\\||\\?|\\*|[\\x00-\\x1F]|(\\.| )$"
  abort: unexpected EOL, expected netstring digit
  [255]

  $ hg up -q "min(all())"
  $ echo "bad" > "nul.txt"
  $ hg ci -Aqm failure
  warning: filename contains 'nul', which is reserved on Windows: nul.txt
  $ hgmn push -r . --to master_bookmark
  pushing rev 7e7f8fb54a0b to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     hooks failed:
  remote:     no_windows_filenames for 7e7f8fb54a0b8f692fbf224a33476b864f11dfe9: ABORT: Illegal windows filename: nul.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Root cause:
  remote:     hooks failed:
  remote:     no_windows_filenames for 7e7f8fb54a0b8f692fbf224a33476b864f11dfe9: ABORT: Illegal windows filename: nul.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Debug context:
  remote:     "hooks failed:\nno_windows_filenames for 7e7f8fb54a0b8f692fbf224a33476b864f11dfe9: ABORT: Illegal windows filename: nul.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\\d)|con|prn|aux|nul))($|\\.))|<|>|:|\"|/|\\\\|\\||\\?|\\*|[\\x00-\\x1F]|(\\.| )$"
  abort: unexpected EOL, expected netstring digit
  [255]

  $ hg up -q "min(all())"
  $ mkdir dir
  $ echo "bad" > dir/CoN.txt
  $ hg ci -Aqm failure
  warning: filename contains 'CoN', which is reserved on Windows: dir/CoN.txt
  $ hgmn push -r . --to master_bookmark
  pushing rev 49604693a23c to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     hooks failed:
  remote:     no_windows_filenames for 49604693a23c85a9ee0f6036d330b535842610dc: ABORT: Illegal windows filename: dir/CoN.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Root cause:
  remote:     hooks failed:
  remote:     no_windows_filenames for 49604693a23c85a9ee0f6036d330b535842610dc: ABORT: Illegal windows filename: dir/CoN.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Debug context:
  remote:     "hooks failed:\nno_windows_filenames for 49604693a23c85a9ee0f6036d330b535842610dc: ABORT: Illegal windows filename: dir/CoN.txt. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\\d)|con|prn|aux|nul))($|\\.))|<|>|:|\"|/|\\\\|\\||\\?|\\*|[\\x00-\\x1F]|(\\.| )$"
  abort: unexpected EOL, expected netstring digit
  [255]

  $ hg up -q "min(all())"
  $ mkdir dir
  $ echo "ok" > dir/Icon.txt
  $ hg ci -Aqm success
  $ hgmn push -r . --to master_bookmark
  pushing rev 74f01fef9e70 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark

  $ hg up -q "min(all())"
  $ mkdir dir
  $ echo "ok" > dir/Icom5
  $ hg ci -Aqm success
  $ hgmn push -r . --to master_bookmark
  pushing rev 47222c857e63 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  adding changesets
  adding manifests
  adding file changes
  updating bookmark master_bookmark

  $ hg up -q "min(all())"
  $ mkdir con
  $ echo "bad" > con/foo
  $ hg ci -Aqm failure
  warning: filename contains 'con', which is reserved on Windows: con/foo
  $ hgmn push -r . --to master_bookmark
  pushing rev 115c8cee8249 to destination mononoke://$LOCALIP:$LOCAL_PORT/repo bookmark master_bookmark
  searching for changes
  remote: Command failed
  remote:   Error:
  remote:     hooks failed:
  remote:     no_windows_filenames for 115c8cee824903baec5607a5b5d731f4a92e5859: ABORT: Illegal windows filename: con/foo. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Root cause:
  remote:     hooks failed:
  remote:     no_windows_filenames for 115c8cee824903baec5607a5b5d731f4a92e5859: ABORT: Illegal windows filename: con/foo. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\d)|con|prn|aux|nul))($|\.))|<|>|:|"|/|\\|\||\?|\*|[\x00-\x1F]|(\.| )$
  remote: 
  remote:   Debug context:
  remote:     "hooks failed:\nno_windows_filenames for 115c8cee824903baec5607a5b5d731f4a92e5859: ABORT: Illegal windows filename: con/foo. Name and path of file in windows should not match regex (^(?i)((((com|lpt)\\d)|con|prn|aux|nul))($|\\.))|<|>|:|\"|/|\\\\|\\||\\?|\\*|[\\x00-\\x1F]|(\\.| )$"
  abort: unexpected EOL, expected netstring digit
  [255]
