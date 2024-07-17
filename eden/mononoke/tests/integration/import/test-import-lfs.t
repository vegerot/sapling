# Copyright (c) Meta Platforms, Inc. and affiliates.
#
# This software may be used and distributed according to the terms of the
# GNU General Public License found in the LICENSE file in the root
# directory of this source tree.

  $ . "${TEST_FIXTURES}/library.sh"
  $ LFS_HELPER="$(realpath "${TESTTMP}/lfs")"

# Setup Mononoke
  $ setup_common_config

# Create a mock LFS helper
  $ cat > "$LFS_HELPER" <<EOF
  > #!/bin/bash
  > echo "lfs: \$*" >&2
  > yes 2>/dev/null | head -c 128
  > EOF
  $ chmod +x "$LFS_HELPER"

# Test importing blobs
  $ cd "$TESTTMP"

  $ cat > bad_hash << EOF
  > version https://git-lfs.github.com/spec/v1
  > oid sha256:d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38
  > size 128
  > EOF
  $ mononoke_import lfs "$LFS_HELPER" "$(cat bad_hash)" --repo-id "$REPOID"
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  Error: Invalid Sha256: InvalidHash { expected: Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38), effective: Sha256(14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33) }
  [1]

  $ cat > bad_size << EOF
  > version https://git-lfs.github.com/spec/v1
  > oid sha256:14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33
  > size 128
  > EOF
  $ mononoke_import lfs "$LFS_HELPER" "$(cat bad_hash)" --repo-id "$REPOID"
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  * lfs_upload: importing blob Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38) (glob)
  lfs: d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38 128
  Error: Invalid Sha256: InvalidHash { expected: Sha256(d6c9160e8ac378413dd55fba213970bbf55afdddaf85999dc3cf8d941f08fb38), effective: Sha256(14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33) }
  [1]

  $ cat > ok << EOF
  > version https://git-lfs.github.com/spec/v1
  > oid sha256:14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33
  > size 128
  > EOF
  $ mononoke_import lfs "$LFS_HELPER" "$(cat ok)"  --repo-id "$REPOID"
  * lfs_upload: importing blob Sha256(14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33) (glob)
  lfs: 14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33 128
  * lfs_upload: imported blob Sha256(14217d6d598954662767fb151ff41cc10261f233d60d92aba9fdaa8534c2db33) (glob)
