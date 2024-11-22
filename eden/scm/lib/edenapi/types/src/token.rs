/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::num::NonZeroU64;

#[cfg(any(test, feature = "for-tests"))]
use quickcheck::Arbitrary;
#[cfg(any(test, feature = "for-tests"))]
use quickcheck_arbitrary_derive::Arbitrary;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use type_macros::auto_wire;

use crate::AnyId;

#[auto_wire]
/// Token metadata for file content token type.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub struct FileContentTokenMetadata {
    #[id(1)]
    pub content_size: u64,
}

/// Token metadata. Could be different for different token types.
/// A signed token guarantee the metadata has been verified.
#[auto_wire]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub enum UploadTokenMetadata {
    #[id(1)]
    FileContentTokenMetadata(FileContentTokenMetadata),
}

impl Default for UploadTokenMetadata {
    fn default() -> Self {
        Self::FileContentTokenMetadata(Default::default())
    }
}

impl From<FileContentTokenMetadata> for UploadTokenMetadata {
    fn from(fctm: FileContentTokenMetadata) -> Self {
        Self::FileContentTokenMetadata(fctm)
    }
}

#[auto_wire]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct UploadTokenData {
    #[id(1)]
    pub id: AnyId,
    #[id(3)]
    pub bubble_id: Option<NonZeroU64>,
    #[id(2)]
    pub metadata: Option<UploadTokenMetadata>,
    // TODO: add other data (like expiration time).
}

#[auto_wire]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub struct UploadTokenSignature {
    #[id(1)]
    pub signature: Vec<u8>,
}

/// Uniquely identifies an id an upload token can refer to.
/// Can be used as a key in maps/sets.
#[auto_wire]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub struct IndexableId {
    #[id(1)]
    pub id: AnyId,
    #[id(2)]
    pub bubble_id: Option<NonZeroU64>,
}

#[auto_wire]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[cfg_attr(any(test, feature = "for-tests"), derive(Arbitrary))]
pub struct UploadToken {
    #[id(1)]
    pub data: UploadTokenData,
    #[id(2)]
    pub signature: UploadTokenSignature,
}

impl UploadToken {
    pub fn new_fake_token(id: AnyId, bubble_id: Option<NonZeroU64>) -> Self {
        Self {
            data: UploadTokenData {
                id,
                bubble_id,
                metadata: None,
            },
            signature: UploadTokenSignature {
                signature: "faketokensignature".into(),
            },
        }
    }

    pub fn new_fake_token_with_metadata(
        id: AnyId,
        bubble_id: Option<NonZeroU64>,
        metadata: UploadTokenMetadata,
    ) -> Self {
        Self {
            data: UploadTokenData {
                id,
                bubble_id,
                metadata: Some(metadata),
            },
            signature: UploadTokenSignature {
                signature: "faketokensignature".into(),
            },
        }
    }
    // TODO: implement secure signed tokens

    pub fn indexable_id(&self) -> IndexableId {
        IndexableId {
            id: self.data.id.clone(),
            bubble_id: self.data.bubble_id,
        }
    }
}

#[cfg(any(test, feature = "for-tests"))]
impl Arbitrary for UploadTokenData {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            id: Arbitrary::arbitrary(g),
            bubble_id: Arbitrary::arbitrary(g),
            metadata: None,
        }
    }
}
