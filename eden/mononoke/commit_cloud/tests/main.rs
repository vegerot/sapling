/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::path::PathBuf;
use std::str::FromStr;

use commit_cloud::references::heads::WorkspaceHead;
use commit_cloud::references::local_bookmarks::WorkspaceLocalBookmark;
use commit_cloud::references::remote_bookmarks::WorkspaceRemoteBookmark;
use commit_cloud::references::snapshots::WorkspaceSnapshot;
use commit_cloud::sql::builder::SqlCommitCloudBuilder;
use commit_cloud::sql::checkout_locations_ops::WorkspaceCheckoutLocation;
use commit_cloud::sql::ops::Delete;
use commit_cloud::sql::ops::Insert;
use fbinit::FacebookInit;
use mercurial_types::HgChangesetId;
use mononoke_types::Timestamp;
use sql_construct::SqlConstruct;

#[fbinit::test]
async fn test_checkout_locations(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::sql::ops::Get;
    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();

    let args = WorkspaceCheckoutLocation {
        hostname: "testhost".to_owned(),
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
        checkout_path: PathBuf::from("checkout/path"),
        shared_path: PathBuf::from("shared/path"),
        timestamp: Timestamp::now(),
        unixname: "testuser".to_owned(),
    };
    let expected = args.clone();
    let mut txn = sql.connections.write_connection.start_transaction().await?;

    txn = sql
        .insert(txn, None, reponame.clone(), workspace.clone(), args)
        .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceCheckoutLocation> = sql.get(reponame, workspace).await?;

    assert_eq!(vec![expected], res);
    Ok(())
}

#[fbinit::test]
async fn test_snapshots(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::sql::ops::Get;
    use commit_cloud::sql::snapshots_ops::DeleteArgs;

    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);

    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();

    let snapshot1 = WorkspaceSnapshot {
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
    };

    let snapshot2 = WorkspaceSnapshot {
        commit: HgChangesetId::from_str("3e0e761030db6e479a7fb58b12881883f9f8c63f").unwrap(),
    };
    let mut txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            snapshot1.clone(),
        )
        .await?;

    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            snapshot2.clone(),
        )
        .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceSnapshot> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert_eq!(res.len(), 2);

    let removed_commits = vec![snapshot1.commit];
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = Delete::<WorkspaceSnapshot>::delete(
        &sql,
        txn,
        None,
        reponame.clone(),
        workspace.clone(),
        DeleteArgs { removed_commits },
    )
    .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceSnapshot> = sql.get(reponame.clone(), workspace.clone()).await?;

    assert_eq!(res, vec![snapshot2]);

    Ok(())
}

#[fbinit::test]
async fn test_heads(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::sql::heads_ops::DeleteArgs;
    use commit_cloud::sql::ops::Get;
    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();

    let head1 = WorkspaceHead {
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
    };

    let head2 = WorkspaceHead {
        commit: HgChangesetId::from_str("3e0e761030db6e479a7fb58b12881883f9f8c63f").unwrap(),
    };
    let mut txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            head1.clone(),
        )
        .await?;

    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            head2.clone(),
        )
        .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceHead> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert_eq!(res.len(), 2);
    let removed_commits = vec![head1.commit];
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = Delete::<WorkspaceHead>::delete(
        &sql,
        txn,
        None,
        reponame.clone(),
        workspace.clone(),
        DeleteArgs { removed_commits },
    )
    .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceHead> = sql.get(reponame.clone(), workspace.clone()).await?;

    assert_eq!(res, vec![head2]);

    Ok(())
}

#[fbinit::test]
async fn test_local_bookmarks(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::sql::local_bookmarks_ops::DeleteArgs;
    use commit_cloud::sql::ops::Get;

    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();

    let bookmark1 = WorkspaceLocalBookmark {
        name: "my_bookmark1".to_owned(),
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
    };

    let bookmark2 = WorkspaceLocalBookmark {
        name: "my_bookmark2".to_owned(),
        commit: HgChangesetId::from_str("3e0e761030db6e479a7fb58b12881883f9f8c63f").unwrap(),
    };

    let mut txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            bookmark1.clone(),
        )
        .await?;

    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            bookmark2.clone(),
        )
        .await?;
    txn.commit().await?;
    let res: Vec<WorkspaceLocalBookmark> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert_eq!(res.len(), 2);

    let removed_bookmarks = vec![bookmark1.name.clone()];
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = Delete::<WorkspaceLocalBookmark>::delete(
        &sql,
        txn,
        None,
        reponame.clone(),
        workspace.clone(),
        DeleteArgs { removed_bookmarks },
    )
    .await?;
    txn.commit().await?;
    let res: Vec<WorkspaceLocalBookmark> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert_eq!(res, vec![bookmark2]);

    Ok(())
}

#[fbinit::test]
async fn test_remote_bookmarks(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::sql::ops::Get;
    use commit_cloud::sql::remote_bookmarks_ops::DeleteArgs;
    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();

    let bookmark1 = WorkspaceRemoteBookmark {
        name: "my_bookmark1".to_owned(),
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
        remote: "remote".to_owned(),
    };

    let bookmark2 = WorkspaceRemoteBookmark {
        name: "my_bookmark2".to_owned(),
        commit: HgChangesetId::from_str("3e0e761030db6e479a7fb58b12881883f9f8c63f").unwrap(),
        remote: "remote".to_owned(),
    };

    let mut txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            bookmark1.clone(),
        )
        .await?;

    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            bookmark2.clone(),
        )
        .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceRemoteBookmark> = sql.get(reponame.clone(), workspace.clone()).await?;

    assert_eq!(res.len(), 2);

    let removed_bookmarks = vec!["remote/my_bookmark1".to_owned()];
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = Delete::<WorkspaceRemoteBookmark>::delete(
        &sql,
        txn,
        None,
        reponame.clone(),
        workspace.clone(),
        DeleteArgs { removed_bookmarks },
    )
    .await?;
    txn.commit().await?;

    let res: Vec<WorkspaceRemoteBookmark> = sql.get(reponame.clone(), workspace.clone()).await?;

    assert_eq!(res, vec![bookmark2]);

    Ok(())
}

#[fbinit::test]
async fn test_versions(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::references::versions::WorkspaceVersion;
    use commit_cloud::sql::ops::Get;
    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();
    let initial_timestamp = Timestamp::now();
    let args = WorkspaceVersion {
        workspace: workspace.clone(),
        version: 1,
        timestamp: initial_timestamp,
        archived: false,
    };

    let mut txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(txn, None, reponame.clone(), workspace.clone(), args.clone())
        .await?;
    txn.commit().await?;
    let res: Vec<WorkspaceVersion> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert_eq!(vec![args], res);

    // Test version conflict
    let args2 = WorkspaceVersion {
        workspace: workspace.clone(),
        version: 2,
        timestamp: Timestamp::now(),
        archived: false,
    };

    txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            args2.clone(),
        )
        .await?;
    txn.commit().await?;
    let res2: Vec<WorkspaceVersion> = sql.get(reponame.clone(), workspace.clone()).await?;
    assert!(res2[0].timestamp > initial_timestamp);

    Ok(())
}

#[fbinit::test]
async fn test_history(_fb: FacebookInit) -> anyhow::Result<()> {
    use commit_cloud::references::history::WorkspaceHistory;
    use commit_cloud::sql::history_ops::DeleteArgs;
    use commit_cloud::sql::history_ops::GetOutput;
    use commit_cloud::sql::history_ops::GetType;
    use commit_cloud::sql::ops::GenericGet;

    // Create a workspace with heads and bookmarks
    let sql = SqlCommitCloudBuilder::with_sqlite_in_memory()?.new(false);
    let reponame = "test_repo".to_owned();
    let workspace = "user_testuser_default".to_owned();
    let timestamp = Timestamp::now();

    let head1 = WorkspaceHead {
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
    };

    let remote_bookmark1 = WorkspaceRemoteBookmark {
        name: "my_bookmark1".to_owned(),
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
        remote: "remote".to_owned(),
    };

    let local_bookmark1 = WorkspaceLocalBookmark {
        name: "my_bookmark1".to_owned(),
        commit: HgChangesetId::from_str("2d7d4ba9ce0a6ffd222de7785b249ead9c51c536").unwrap(),
    };

    let args1 = WorkspaceHistory {
        version: 1,
        timestamp: Some(Timestamp::now()),
        heads: vec![head1.clone()],
        local_bookmarks: vec![local_bookmark1.clone()],
        remote_bookmarks: vec![remote_bookmark1.clone()],
    };

    let mut txn = sql.connections.write_connection.start_transaction().await?;
    // Insert a history entry, retrieve it and cast it to Rust struct
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            args1.clone(),
        )
        .await?;
    txn.commit().await?;

    let res: Vec<GetOutput> = sql
        .get(
            reponame.clone(),
            workspace.clone(),
            GetType::GetHistoryVersion { version: 1 },
        )
        .await?;

    let res_as_history: Vec<WorkspaceHistory> = res
        .into_iter()
        .map(|output| match output {
            GetOutput::WorkspaceHistory(history) => history,
            _ => panic!("Output doesn't match query type"),
        })
        .collect::<Vec<WorkspaceHistory>>();

    assert_eq!(vec![args1], res_as_history);

    // Insert a new history entry
    let args2 = WorkspaceHistory {
        version: 2,
        timestamp: Some(Timestamp::now()),
        heads: vec![head1],
        local_bookmarks: vec![local_bookmark1],
        remote_bookmarks: vec![remote_bookmark1],
    };
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = sql
        .insert(
            txn,
            None,
            reponame.clone(),
            workspace.clone(),
            args2.clone(),
        )
        .await?;
    txn.commit().await?;

    // Delete first history entry, validate only second entry is left
    txn = sql.connections.write_connection.start_transaction().await?;
    txn = Delete::<WorkspaceHistory>::delete(
        &sql,
        txn,
        None,
        reponame.clone(),
        workspace.clone(),
        DeleteArgs {
            keep_days: 0,
            keep_version: 1,
            delete_limit: 1,
        },
    )
    .await?;
    txn.commit().await?;

    let res: Vec<GetOutput> = sql
        .get(
            reponame.clone(),
            workspace.clone(),
            GetType::GetHistoryDate {
                timestamp,
                limit: 2,
            },
        )
        .await?;

    let res_as_history: Vec<WorkspaceHistory> = res
        .into_iter()
        .map(|output| match output {
            GetOutput::WorkspaceHistory(history) => history,
            _ => panic!("Output doesn't match query type"),
        })
        .collect::<Vec<WorkspaceHistory>>();

    assert_eq!(vec![args2], res_as_history);

    Ok(())
}
