/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! A hash of a node (changeset, manifest or file).

use std::fmt;
use std::fmt::Display;
use std::result;
use std::str::FromStr;

use abomonation_derive::Abomonation;
use anyhow::Result;
use ascii::AsciiStr;
use ascii::AsciiString;
use edenapi_types::CommitId as EdenapiCommitId;
use mononoke_types::sha1_hash;
use mononoke_types::sha1_hash::Sha1;
use mononoke_types::sha1_hash::Sha1Prefix;
use mononoke_types::FileType;
use quickcheck_arbitrary_derive::Arbitrary;
use sql::mysql;
/// Type used to represent a node hash in the Mercurial client's Rust code.
/// Equivalent to HgNodeHash;
use types::HgId;

use crate::manifest::Type;
use crate::thrift;
use crate::RepoPath;

pub const NULL_HASH: HgNodeHash = HgNodeHash(sha1_hash::NULL);
pub const NULL_CSID: HgChangesetId = HgChangesetId(NULL_HASH);

/// This structure represents Sha1 based hashes that are used in Mercurial, but the Sha1
/// structure is private outside this crate to keep it an implementation detail.
/// This is why the main constructors to create this structure are from_bytes and from_ascii_str
/// which parses raw bytes or hex string to create HgNodeHash.
#[derive(Arbitrary, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
#[derive(Abomonation)]
pub struct HgNodeHash(pub(crate) Sha1);

impl HgNodeHash {
    pub const fn new(sha1: Sha1) -> Self {
        HgNodeHash(sha1)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Sha1::from_bytes(bytes).map(HgNodeHash)
    }

    pub fn from_thrift(thrift_hash: thrift::HgNodeHash) -> Result<Self> {
        Ok(HgNodeHash(Sha1::from_thrift(thrift_hash.0)?))
    }

    pub fn from_thrift_opt(thrift_hash_opt: Option<thrift::HgNodeHash>) -> Result<Option<Self>> {
        match thrift_hash_opt {
            Some(h) => Ok(Some(Self::from_thrift(h)?)),
            None => Ok(None),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    pub fn from_static_str(hash: &'static str) -> Result<Self> {
        Sha1::from_str(hash).map(HgNodeHash)
    }

    pub fn sha1(&self) -> &Sha1 {
        &self.0
    }

    #[inline]
    pub fn from_ascii_str(s: &AsciiStr) -> Result<Self> {
        Sha1::from_ascii_str(s).map(HgNodeHash)
    }

    /// Returns a 40 hex digits representation of the sha1 hash
    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        self.0.to_hex()
    }

    #[inline]
    pub fn into_option(self) -> Option<Self> {
        if self == NULL_HASH { None } else { Some(self) }
    }

    pub fn into_thrift(self) -> thrift::HgNodeHash {
        thrift::HgNodeHash(self.0.into_thrift())
    }

    #[inline]
    pub fn display_opt<'a>(opt_hash: Option<&'a HgNodeHash>) -> OptDisplay<'a, Self> {
        OptDisplay { inner: opt_hash }
    }

    /// Return a stable hash fingerprint that can be used for sampling
    #[inline]
    pub fn sampling_fingerprint(&self) -> u64 {
        let byte_slice = &self.0.as_ref();
        let mut bytes: [u8; 8] = [0; 8];
        bytes.copy_from_slice(&byte_slice[0..8]);
        u64::from_le_bytes(bytes)
    }
}

pub struct OptDisplay<'a, T> {
    inner: Option<&'a T>,
}

impl<'a, T: Display> Display for OptDisplay<'a, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self.inner {
            Some(inner) => inner.fmt(fmt),
            None => write!(fmt, "(none)"),
        }
    }
}

impl From<Option<HgNodeHash>> for HgNodeHash {
    fn from(h: Option<HgNodeHash>) -> Self {
        match h {
            None => NULL_HASH,
            Some(h) => h,
        }
    }
}

impl From<HgNodeHash> for HgId {
    fn from(node: HgNodeHash) -> Self {
        HgId::from_byte_array(node.0.into_byte_array())
    }
}

impl From<HgId> for HgNodeHash {
    fn from(hgid: HgId) -> Self {
        Self::from_bytes(hgid.as_ref()).unwrap()
    }
}

struct StringVisitor;

impl<'de> serde::de::Visitor<'de> for StringVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("40 hex digits")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value.to_string())
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(value)
    }
}

impl serde::ser::Serialize for HgNodeHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_hex().as_str())
    }
}

impl<'de> serde::de::Deserialize<'de> for HgNodeHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let hex = deserializer.deserialize_string(StringVisitor)?;
        match Sha1::from_str(hex.as_str()) {
            Ok(sha1) => Ok(HgNodeHash(sha1)),
            Err(error) => Err(serde::de::Error::custom(error)),
        }
    }
}

impl AsRef<[u8]> for HgNodeHash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl FromStr for HgNodeHash {
    type Err = <Sha1 as FromStr>::Err;

    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        Sha1::from_str(s).map(HgNodeHash)
    }
}

impl Display for HgNodeHash {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

#[derive(Arbitrary, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
#[derive(Abomonation, mysql::OptTryFromRowField)]
pub struct HgChangesetId(HgNodeHash);

impl HgChangesetId {
    #[inline]
    pub fn from_ascii_str(s: &AsciiStr) -> Result<HgChangesetId> {
        HgNodeHash::from_ascii_str(s).map(HgChangesetId)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        HgNodeHash::from_bytes(bytes).map(HgChangesetId)
    }

    pub fn from_thrift(thrift_hash: thrift::HgNodeHash) -> Result<Self> {
        HgNodeHash::from_thrift(thrift_hash).map(HgChangesetId)
    }

    pub fn from_thrift_opt(thrift_hash_opt: Option<thrift::HgNodeHash>) -> Result<Option<Self>> {
        match thrift_hash_opt {
            Some(h) => Ok(Some(Self::from_thrift(h)?)),
            None => Ok(None),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn into_nodehash(self) -> HgNodeHash {
        self.0
    }

    pub fn into_thrift(self) -> thrift::HgNodeHash {
        self.into_nodehash().into_thrift()
    }

    pub const fn new(hash: HgNodeHash) -> Self {
        HgChangesetId(hash)
    }

    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        self.0.to_hex()
    }

    /// Produce a key suitable for using in a blobstore.
    #[inline]
    pub fn blobstore_key(&self) -> String {
        format!("hgchangeset.sha1.{}", self.0)
    }

    #[inline]
    pub fn display_opt<'a>(opt_changeset_id: Option<&'a HgChangesetId>) -> OptDisplay<'a, Self> {
        OptDisplay {
            inner: opt_changeset_id,
        }
    }

    #[inline]
    pub fn sampling_fingerprint(&self) -> u64 {
        self.0.sampling_fingerprint()
    }
}

impl AsRef<[u8]> for HgChangesetId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl FromStr for HgChangesetId {
    type Err = <HgNodeHash as FromStr>::Err;

    fn from_str(s: &str) -> result::Result<HgChangesetId, Self::Err> {
        HgNodeHash::from_str(s).map(HgChangesetId)
    }
}

impl Display for HgChangesetId {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

impl From<HgChangesetId> for EdenapiCommitId {
    fn from(value: HgChangesetId) -> Self {
        EdenapiCommitId::Hg(value.into())
    }
}

impl serde::ser::Serialize for HgChangesetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> serde::de::Deserialize<'de> for HgChangesetId {
    fn deserialize<D>(deserializer: D) -> Result<HgChangesetId, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let hex = deserializer.deserialize_string(StringVisitor)?;
        match HgNodeHash::from_str(hex.as_str()) {
            Ok(hash) => Ok(HgChangesetId::new(hash)),
            Err(error) => Err(serde::de::Error::custom(error)),
        }
    }
}

impl From<HgId> for HgChangesetId {
    fn from(hgid: HgId) -> Self {
        HgChangesetId::new(HgNodeHash::from(hgid))
    }
}

impl From<HgChangesetId> for HgId {
    fn from(hg_cs_id: HgChangesetId) -> HgId {
        hg_cs_id.into_nodehash().into()
    }
}

/// An identifier for a changeset hash prefix in Nercurial.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
#[derive(Abomonation)]
pub struct HgChangesetIdPrefix(Sha1Prefix);

impl HgChangesetIdPrefix {
    pub const fn new(sha1prefix: Sha1Prefix) -> Self {
        HgChangesetIdPrefix(sha1prefix)
    }

    pub fn from_bytes<B: AsRef<[u8]> + ?Sized>(bytes: &B) -> Result<Self> {
        Sha1Prefix::from_bytes(bytes).map(Self::new)
    }

    #[inline]
    pub fn min_cs(&self) -> HgChangesetId {
        HgChangesetId::new(
            HgNodeHash::from_bytes(self.0.min_as_ref()).expect("Min sha1 is a valid sha1"),
        )
    }

    #[inline]
    pub fn max_cs(&self) -> HgChangesetId {
        HgChangesetId::new(
            HgNodeHash::from_bytes(self.0.max_as_ref()).expect("Max sha1 is a valid sha1"),
        )
    }

    #[inline]
    pub fn min_as_ref(&self) -> &[u8] {
        self.0.min_as_ref()
    }

    #[inline]
    pub fn max_as_ref(&self) -> &[u8] {
        self.0.max_as_ref()
    }

    #[inline]
    pub fn into_hg_changeset_id(self) -> Option<HgChangesetId> {
        self.0.into_sha1().map(HgNodeHash).map(HgChangesetId)
    }
}

impl FromStr for HgChangesetIdPrefix {
    type Err = <Sha1Prefix as FromStr>::Err;
    fn from_str(s: &str) -> result::Result<HgChangesetIdPrefix, Self::Err> {
        Sha1Prefix::from_str(s).map(HgChangesetIdPrefix)
    }
}

impl Display for HgChangesetIdPrefix {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
/// The type for resolving changesets by prefix of the hash
pub enum HgChangesetIdsResolvedFromPrefix {
    /// Found single changeset
    Single(HgChangesetId),
    /// Found several changesets within the limit provided
    Multiple(Vec<HgChangesetId>),
    /// Found too many changesets exceeding the limit provided
    TooMany(Vec<HgChangesetId>),
    /// Changeset was not found
    NoMatch,
}

#[derive(Arbitrary, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct HgManifestId(HgNodeHash);

impl HgManifestId {
    pub fn into_nodehash(self) -> HgNodeHash {
        self.0
    }

    pub const fn new(hash: HgNodeHash) -> Self {
        HgManifestId(hash)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        HgNodeHash::from_bytes(bytes).map(HgManifestId)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        self.0.to_hex()
    }

    /// Produce a key suitable for using in a blobstore.
    #[inline]
    pub fn blobstore_key(&self) -> String {
        format!("hgmanifest.sha1.{}", self.0)
    }

    #[inline]
    pub fn sampling_fingerprint(&self) -> u64 {
        self.0.sampling_fingerprint()
    }
}

impl FromStr for HgManifestId {
    type Err = <HgNodeHash as FromStr>::Err;

    fn from_str(s: &str) -> result::Result<HgManifestId, Self::Err> {
        HgNodeHash::from_str(s).map(HgManifestId)
    }
}

impl Display for HgManifestId {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

#[derive(Arbitrary, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub struct HgAugmentedManifestId(HgNodeHash);

impl HgAugmentedManifestId {
    pub const fn new(hash: HgNodeHash) -> Self {
        HgAugmentedManifestId(hash)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        HgNodeHash::from_bytes(bytes).map(HgAugmentedManifestId)
    }

    pub fn from_thrift(thrift_hash: thrift::HgNodeHash) -> Result<Self> {
        HgNodeHash::from_thrift(thrift_hash).map(HgAugmentedManifestId)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn into_nodehash(self) -> HgNodeHash {
        self.0
    }

    pub fn into_thrift(self) -> thrift::HgNodeHash {
        self.into_nodehash().into_thrift()
    }

    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        self.0.to_hex()
    }

    /// Produce a key suitable for using in a blobstore.
    #[inline]
    pub fn blobstore_key(&self) -> String {
        format!("hgaugmentedmanifest.sha1.{}", self.0)
    }

    #[inline]
    pub fn sampling_fingerprint(&self) -> u64 {
        self.0.sampling_fingerprint()
    }
}

impl From<HgManifestId> for HgAugmentedManifestId {
    fn from(manifest_id: HgManifestId) -> Self {
        HgAugmentedManifestId::new(manifest_id.into_nodehash())
    }
}

impl FromStr for HgAugmentedManifestId {
    type Err = <HgNodeHash as FromStr>::Err;

    fn from_str(s: &str) -> result::Result<HgAugmentedManifestId, Self::Err> {
        HgNodeHash::from_str(s).map(HgAugmentedManifestId)
    }
}

impl Display for HgAugmentedManifestId {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

#[derive(Arbitrary, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
#[derive(Abomonation, mysql::OptTryFromRowField)]
pub struct HgFileNodeId(HgNodeHash);

impl HgFileNodeId {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        HgNodeHash::from_bytes(bytes).map(HgFileNodeId)
    }

    pub fn from_thrift(thrift_hash: thrift::HgNodeHash) -> Result<Self> {
        HgNodeHash::from_thrift(thrift_hash).map(HgFileNodeId)
    }

    pub fn from_thrift_opt(thrift_hash_opt: Option<thrift::HgNodeHash>) -> Result<Option<Self>> {
        match thrift_hash_opt {
            Some(h) => Ok(Some(Self::from_thrift(h)?)),
            None => Ok(None),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    pub fn into_nodehash(self) -> HgNodeHash {
        self.0
    }

    pub const fn new(hash: HgNodeHash) -> Self {
        HgFileNodeId(hash)
    }

    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        self.0.to_hex()
    }

    /// Produce a key suitable for using in a blobstore.
    #[inline]
    pub fn blobstore_key(&self) -> String {
        format!("hgfilenode.sha1.{}", self.0)
    }

    #[inline]
    pub fn sampling_fingerprint(&self) -> u64 {
        self.0.sampling_fingerprint()
    }
}

impl FromStr for HgFileNodeId {
    type Err = <HgNodeHash as FromStr>::Err;

    fn from_str(s: &str) -> result::Result<HgFileNodeId, Self::Err> {
        HgNodeHash::from_str(s).map(HgFileNodeId)
    }
}

impl Display for HgFileNodeId {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub enum HgEntryId {
    File(FileType, HgFileNodeId),
    Manifest(HgManifestId),
}

impl HgEntryId {
    pub fn into_nodehash(self) -> HgNodeHash {
        match self {
            HgEntryId::File(_, file_hash) => file_hash.into_nodehash(),
            HgEntryId::Manifest(manifest_hash) => manifest_hash.into_nodehash(),
        }
    }

    #[inline]
    pub fn to_hex(&self) -> AsciiString {
        match self {
            HgEntryId::File(_, filenode_id) => filenode_id.to_hex(),
            HgEntryId::Manifest(manifest_id) => manifest_id.to_hex(),
        }
    }

    #[inline]
    pub fn to_filenode(&self) -> Option<(FileType, HgFileNodeId)> {
        match self {
            HgEntryId::File(file_type, filenode_id) => Some((*file_type, *filenode_id)),
            _ => None,
        }
    }

    #[inline]
    pub fn to_manifest(&self) -> Option<HgManifestId> {
        match self {
            HgEntryId::Manifest(manifest_id) => Some(*manifest_id),
            _ => None,
        }
    }

    #[inline]
    pub fn get_type(&self) -> Type {
        match self {
            HgEntryId::File(file_type, _) => Type::File(*file_type),
            HgEntryId::Manifest(_) => Type::Tree,
        }
    }
}

impl From<HgManifestId> for HgEntryId {
    fn from(manifest_id: HgManifestId) -> Self {
        HgEntryId::Manifest(manifest_id)
    }
}

impl Display for HgEntryId {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        (*self).into_nodehash().fmt(fmt)
    }
}

/// A (path, hash) combination. This is the key used throughout Mercurial for manifest and file
/// nodes.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HgNodeKey {
    pub path: RepoPath,
    pub hash: HgNodeHash,
}

impl Display for HgNodeKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "path: {}, hash: {}", self.path, self.hash)
    }
}

macro_rules! impl_hash {
    ($hash_type: ident) => {
        impl slog::Value for $hash_type {
            fn serialize(
                &self,
                _record: &slog::Record,
                key: slog::Key,
                serializer: &mut dyn slog::Serializer,
            ) -> slog::Result {
                let hex = self.to_hex();
                serializer.emit_str(key, hex.as_str())
            }
        }
    };
}

impl_hash!(HgNodeHash);
impl_hash!(HgChangesetId);
impl_hash!(HgManifestId);
impl_hash!(HgAugmentedManifestId);
impl_hash!(HgFileNodeId);
impl_hash!(HgEntryId);
