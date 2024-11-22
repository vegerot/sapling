/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use storemodel::FileAuxData as ScmStoreFileAuxData;
use storemodel::TreeAuxData as ScmStoreTreeAuxData;

use crate::ffi::ffi::FileAuxData;
use crate::ffi::ffi::TreeAuxData;

impl From<ScmStoreFileAuxData> for FileAuxData {
    fn from(v: ScmStoreFileAuxData) -> Self {
        FileAuxData {
            total_size: v.total_size,
            content_sha1: v.sha1.into(),
            content_blake3: v.blake3.into_byte_array(),
        }
    }
}

impl From<ScmStoreTreeAuxData> for TreeAuxData {
    fn from(v: ScmStoreTreeAuxData) -> Self {
        TreeAuxData {
            digest_size: v.augmented_manifest_size,
            digest_hash: v.augmented_manifest_id.into_byte_array(),
        }
    }
}
