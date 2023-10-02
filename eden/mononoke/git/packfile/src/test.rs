/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![cfg(test)]

use std::io::Write;
use std::sync::atomic::AtomicBool;

use bytes::Bytes;
use bytes::BytesMut;
use flate2::write::ZlibDecoder;
use futures::stream;
use futures::Future;
use gix_hash::ObjectId;
use gix_object::Object;
use gix_object::ObjectRef;
use gix_object::Tag;
use tempfile::NamedTempFile;

use crate::bundle::BundleWriter;
use crate::pack::PackfileWriter;
use crate::types::to_vec_bytes;
use crate::types::BaseObject;
use crate::types::PackfileItem;

fn get_objects_stream()
-> anyhow::Result<impl stream::Stream<Item = impl Future<Output = anyhow::Result<PackfileItem>>>> {
    // Create a few Git objects
    let tag_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tag(Tag {
        target: ObjectId::empty_tree(gix_hash::Kind::Sha1),
        target_kind: gix_object::Kind::Tree,
        name: "TreeTag".into(),
        tagger: None,
        message: "Tag pointing to a tree".into(),
        pgp_signature: None,
    }))?);
    let blob_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Blob(gix_object::Blob {
        data: "Some file content".as_bytes().to_vec(),
    }))?);
    let tree_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tree(gix_object::Tree {
        entries: vec![gix_object::tree::Entry {
            mode: gix_object::tree::EntryMode::Blob,
            filename: "JustAFile.txt".into(),
            oid: ObjectId::empty_blob(gix_hash::Kind::Sha1),
        }],
    }))?);
    let objects_stream = stream::iter(vec![
        futures::future::ready(PackfileItem::new_base(tag_bytes)),
        futures::future::ready(PackfileItem::new_base(blob_bytes)),
        futures::future::ready(PackfileItem::new_base(tree_bytes)),
    ]);
    Ok(objects_stream)
}

#[test]
fn validate_packitem_creation() -> anyhow::Result<()> {
    // Create a Git object
    let tag = Tag {
        target: ObjectId::empty_tree(gix_hash::Kind::Sha1),
        target_kind: gix_object::Kind::Tree,
        name: "TreeTag".into(),
        tagger: None,
        message: "Tag pointing to a tree".into(),
        pgp_signature: None,
    };
    // Get the bytes of the Git object
    let bytes =
        to_vec_bytes(&Object::Tag(tag)).expect("Expected successful Git object serialization");
    // Convert it into a packfile item
    BaseObject::new(Bytes::from(bytes)).expect("Expected successful PackfileItem creation");
    Ok(())
}

#[test]
fn validate_packfile_item_encoding() -> anyhow::Result<()> {
    // Create a Git object
    let tag = Tag {
        target: ObjectId::empty_tree(gix_hash::Kind::Sha1),
        target_kind: gix_object::Kind::Tree,
        name: "TreeTag".into(),
        tagger: None,
        message: "Tag pointing to a tree".into(),
        pgp_signature: None,
    };
    // Get the bytes of the Git object
    let bytes =
        to_vec_bytes(&Object::Tag(tag)).expect("Expected successful Git object serialization");
    // Convert it into a packfile item
    let item =
        BaseObject::new(Bytes::from(bytes)).expect("Expected successful PackfileItem creation");
    let mut encoded_bytes = BytesMut::new();
    item.write_encoded(&mut encoded_bytes, true)
        .expect("Expected successful encoding of packfile item");
    let encoded_bytes = encoded_bytes.freeze();
    // Decode the bytes and try to recreate the git object
    let mut decoded_bytes = Vec::new();
    let mut decoder = ZlibDecoder::new(decoded_bytes);
    decoder.write_all(encoded_bytes.as_ref())?;
    decoded_bytes = decoder.finish()?;
    // Validate the decoded bytes represent a valid Git object
    ObjectRef::from_loose(decoded_bytes.as_ref())
        .expect("Expected successful Git object creation from decoded bytes");
    Ok(())
}

#[fbinit::test]
async fn validate_basic_packfile_generation() -> anyhow::Result<()> {
    let objects_stream = get_objects_stream()?;
    let mut packfile_writer = PackfileWriter::new(Vec::new(), 3);
    // Validate we are able to write the objects to the packfile without errors
    packfile_writer
        .write(objects_stream)
        .await
        .expect("Expected successful write of objects to packfile");
    // Validate we are able to finish writing to the packfile and generate the final checksum
    let checksum = packfile_writer.finish().await;
    assert!(checksum.is_ok());
    Ok(())
}

#[fbinit::test]
async fn validate_packfile_generation_format() -> anyhow::Result<()> {
    // Create a few Git objects
    let objects_stream = get_objects_stream()?;
    let mut packfile_writer = PackfileWriter::new(Vec::new(), 3);
    // Validate we are able to write the objects to the packfile without errors
    packfile_writer
        .write(objects_stream)
        .await
        .expect("Expected successful write of objects to packfile");
    // Validate we are able to finish writing to the packfile and generate the final checksum
    let checksum = packfile_writer
        .finish()
        .await
        .expect("Expected successful checksum computation for packfile");
    // Capture the packfile size and number of objects
    let (num_entries, size) = (packfile_writer.num_entries, packfile_writer.size);
    // Retrieve the raw_writer (in this case Vec) back from the PackfileWriter
    let written_content = packfile_writer.into_write();
    // Write the packfile to disk
    let mut created_file = NamedTempFile::new()?;
    created_file.write_all(written_content.as_ref())?;
    // Open the written packfile
    let opened_packfile = gix_pack::data::File::at(created_file.path(), gix_hash::Kind::Sha1);
    // Validate that the packfile gets opened without error
    assert!(opened_packfile.is_ok());
    let opened_packfile = opened_packfile.expect("Expected successful opening of packfile");
    // Validate the number of objects in the packfile
    assert_eq!(opened_packfile.num_objects(), num_entries);
    // Validate the size of the packfile
    assert_eq!(opened_packfile.data_len(), size as usize);
    // Verify the checksum of the packfile
    let checksum_from_file = opened_packfile
        .verify_checksum(gix_features::progress::Discard, &AtomicBool::new(false))
        .expect("Expected successful checksum computation");
    // Verify the checksum matches the hash generated when computing the packfile
    assert_eq!(checksum, checksum_from_file);
    Ok(())
}

#[fbinit::test]
async fn validate_staggered_packfile_generation() -> anyhow::Result<()> {
    let mut packfile_writer = PackfileWriter::new(Vec::new(), 3);
    // Create Git objects and write them to a packfile one at a time
    let tag_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tag(Tag {
        target: ObjectId::empty_tree(gix_hash::Kind::Sha1),
        target_kind: gix_object::Kind::Tree,
        name: "TreeTag".into(),
        tagger: None,
        message: "Tag pointing to a tree".into(),
        pgp_signature: None,
    }))?);
    // Validate we are able to write the object to the packfile without errors
    packfile_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(tag_bytes),
        )]))
        .await
        .expect("Expected successful write of object to packfile");
    let blob_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Blob(gix_object::Blob {
        data: "Some file content".as_bytes().to_vec(),
    }))?);
    // Validate we are able to write the object to the packfile without errors
    packfile_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(blob_bytes),
        )]))
        .await
        .expect("Expected successful write of object to packfile");
    let tree_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tree(gix_object::Tree {
        entries: vec![gix_object::tree::Entry {
            mode: gix_object::tree::EntryMode::Blob,
            filename: "JustAFile.txt".into(),
            oid: ObjectId::empty_blob(gix_hash::Kind::Sha1),
        }],
    }))?);
    // Validate we are able to write the object to the packfile without errors
    packfile_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(tree_bytes),
        )]))
        .await
        .expect("Expected successful write of object to packfile");

    // Validate we are able to finish writing to the packfile and generate the final checksum
    let checksum = packfile_writer
        .finish()
        .await
        .expect("Expected successful checksum computation for packfile");
    // Capture the packfile size and number of objects
    let (num_entries, size) = (packfile_writer.num_entries, packfile_writer.size);
    // Retrieve the raw_writer (in this case Vec) back from the PackfileWriter
    let written_content = packfile_writer.into_write();
    // Write the packfile to disk
    let mut created_file = NamedTempFile::new()?;
    created_file.write_all(written_content.as_ref())?;
    // Open the written packfile
    let opened_packfile = gix_pack::data::File::at(created_file.path(), gix_hash::Kind::Sha1);
    // Validate that the packfile gets opened without error
    assert!(opened_packfile.is_ok());
    let opened_packfile = opened_packfile.expect("Expected successful opening of packfile");
    // Validate the number of objects in the packfile
    assert_eq!(opened_packfile.num_objects(), num_entries);
    // Validate the size of the packfile
    assert_eq!(opened_packfile.data_len(), size as usize);
    // Verify the checksum of the packfile
    let checksum_from_file = opened_packfile
        .verify_checksum(gix_features::progress::Discard, &AtomicBool::new(false))
        .expect("Expected successful checksum computation");
    // Verify the checksum matches the hash generated when computing the packfile
    assert_eq!(checksum, checksum_from_file);
    Ok(())
}

#[fbinit::test]
async fn validate_roundtrip_packfile_generation() -> anyhow::Result<()> {
    // Create a few Git objects
    let objects_stream = get_objects_stream()?;
    let mut packfile_writer = PackfileWriter::new(Vec::new(), 3);
    // Validate we are able to write the objects to the packfile without errors
    packfile_writer
        .write(objects_stream)
        .await
        .expect("Expected successful write of objects to packfile");
    // Validate we are able to finish writing to the packfile and generate the final checksum
    packfile_writer
        .finish()
        .await
        .expect("Expected successful checksum computation for packfile");
    // Retrieve the raw_writer (in this case Vec) back from the PackfileWriter
    let written_content = packfile_writer.into_write();
    // Write the packfile to disk
    let mut created_file = NamedTempFile::new()?;
    created_file.write_all(written_content.as_ref())?;
    // Open the written packfile
    let opened_packfile = gix_pack::data::File::at(created_file.path(), gix_hash::Kind::Sha1);
    // Validate that the packfile gets opened without error
    assert!(opened_packfile.is_ok());
    let opened_packfile = opened_packfile.expect("Expected successful opening of packfile");
    // Validate that we are able to iterate over the entries in the packfile
    for entry in opened_packfile
        .streaming_iter()
        .expect("Expected successful iteration of packfile entries")
    {
        // Validate the entry is a valid Git object
        let entry = entry.expect("Expected valid Git object in packfile entry");
        // Since we used only base objects, the packfile entries should all have is_base() set to true
        assert!(entry.header.is_base());
    }
    Ok(())
}

#[fbinit::test]
async fn validate_basic_bundle_generation() -> anyhow::Result<()> {
    // Create a few Git objects
    let objects_stream = get_objects_stream()?;
    let refs = vec![(
        "HEAD".to_owned(),
        ObjectId::empty_tree(gix_hash::Kind::Sha1),
    )];
    // Validate we are able to successfully create BundleWriter
    let mut bundle_writer = BundleWriter::new_with_header(Vec::new(), refs, None, 3)
        .await
        .expect("Expected successful creation of BundleWriter");
    // Validate we are able to successfully write objects to the bundle
    bundle_writer
        .write(objects_stream)
        .await
        .expect("Expected successful write of objects to bundle.");
    // Validate we are able to finish writing to the bundle
    bundle_writer
        .finish()
        .await
        .expect("Expected successful finish of bundle creation");
    Ok(())
}

#[fbinit::test]
async fn validate_staggered_bundle_generation() -> anyhow::Result<()> {
    let refs = vec![(
        "HEAD".to_owned(),
        ObjectId::empty_tree(gix_hash::Kind::Sha1),
    )];
    // Validate we are able to successfully create BundleWriter
    let mut bundle_writer = BundleWriter::new_with_header(Vec::new(), refs, None, 3)
        .await
        .expect("Expected successful creation of BundleWriter");
    // Create a few Git objects
    let tag_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tag(Tag {
        target: ObjectId::empty_tree(gix_hash::Kind::Sha1),
        target_kind: gix_object::Kind::Tree,
        name: "TreeTag".into(),
        tagger: None,
        message: "Tag pointing to a tree".into(),
        pgp_signature: None,
    }))?);
    // Validate we are able to write the object to the bundle without errors
    bundle_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(tag_bytes),
        )]))
        .await
        .expect("Expected successful write of object to bundle");
    let blob_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Blob(gix_object::Blob {
        data: "Some file content".as_bytes().to_vec(),
    }))?);
    // Validate we are able to write the object to the bundle without errors
    bundle_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(blob_bytes),
        )]))
        .await
        .expect("Expected successful write of object to bundle");
    let tree_bytes = Bytes::from(to_vec_bytes(&gix_object::Object::Tree(gix_object::Tree {
        entries: vec![],
    }))?);
    // Validate we are able to write the object to the bundle without errors
    bundle_writer
        .write(stream::iter(vec![futures::future::ready(
            PackfileItem::new_base(tree_bytes),
        )]))
        .await
        .expect("Expected successful write of object to bundle");
    // Validate we are able to finish writing to the bundle
    bundle_writer
        .finish()
        .await
        .expect("Expected successful finish of bundle creation");
    Ok(())
}
