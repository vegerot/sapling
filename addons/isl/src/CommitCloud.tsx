/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {CommitCloudSyncState, Hash, Result} from './types';

import serverAPI from './ClientToServerAPI';
import {Commit} from './Commit';
import {FlexSpacer} from './ComponentUtils';
import {EducationInfoTip} from './Education';
import {T, t} from './i18n';
import {writeAtom} from './jotaiUtils';
import {CommitCloudChangeWorkspaceOperation} from './operations/CommitCloudChangeWorkspaceOperation';
import {CommitCloudCreateWorkspaceOperation} from './operations/CommitCloudCreateWorkspaceOperation';
import {CommitCloudSyncOperation} from './operations/CommitCloudSyncOperation';
import {useRunOperation} from './operationsState';
import {CommitPreview, dagWithPreviews, useMostRecentPendingOperation} from './previews';
import {RelativeDate} from './relativeDate';
import {CommitCloudBackupStatus} from './types';
import {registerDisposable} from './utils';
import {Button} from 'isl-components/Button';
import {Dropdown} from 'isl-components/Dropdown';
import {ErrorNotice, InlineErrorBadge} from 'isl-components/ErrorNotice';
import {Icon} from 'isl-components/Icon';
import {Subtle} from 'isl-components/Subtle';
import {TextField} from 'isl-components/TextField';
import {Tooltip} from 'isl-components/Tooltip';
import {atom, useAtom, useAtomValue} from 'jotai';
import {useCallback, useEffect, useRef, useState} from 'react';
import {notEmpty} from 'shared/utils';

import './CommitCloud.css';

const cloudSyncStateAtom = atom<Result<CommitCloudSyncState> | null>(null);

registerDisposable(
  cloudSyncStateAtom,
  serverAPI.onMessageOfType('fetchedCommitCloudState', event => {
    writeAtom(cloudSyncStateAtom, event.state);
  }),
  import.meta.hot,
);

const REFRESH_INTERVAL = 30 * 1000;

export function CommitCloudInfo() {
  const [cloudSyncState, setCloudSyncState] = useAtom(cloudSyncStateAtom);
  const runOperation = useRunOperation();
  const pendingOperation = useMostRecentPendingOperation();
  const isRunningSync = pendingOperation?.trackEventName === 'CommitCloudSyncOperation';
  const isLoading = cloudSyncState?.value?.isFetching === true;
  const [enteredWorkspaceName, setEnteredWorkspaceName] = useState<null | string>(null);

  const refreshCommitCloudStatus = useCallback(() => {
    setCloudSyncState(old =>
      old?.value != null ? {value: {...old.value, isFetching: true}} : old,
    );
    serverAPI.postMessage({
      type: 'fetchCommitCloudState',
    });
  }, [setCloudSyncState]);

  useEffect(() => {
    const interval = setInterval(refreshCommitCloudStatus, REFRESH_INTERVAL);
    // also call immediately on mount
    refreshCommitCloudStatus();
    return () => clearInterval(interval);
  }, [refreshCommitCloudStatus]);

  const isMakingWorkspace = enteredWorkspaceName != null;
  const newWorkspaceNameRef = useRef(null);
  useEffect(() => {
    if (isMakingWorkspace && newWorkspaceNameRef.current != null) {
      (newWorkspaceNameRef.current as HTMLInputElement).focus();
    }
  }, [newWorkspaceNameRef, isMakingWorkspace]);

  return (
    <div className="commit-cloud-info">
      <div className="dropdown-fields-header commit-cloud-header">
        <Icon icon="cloud" size="M" />
        <strong role="heading">{<T>Commit Cloud</T>}</strong>
        <EducationInfoTip>
          <T>Commit Cloud backs up your draft commits automatically across all your devices.</T>
        </EducationInfoTip>
        {isLoading && <Icon icon="loading" />}
      </div>

      {cloudSyncState?.value?.isDisabled !== true ? null : (
        <div className="commit-cloud-row">
          <Subtle>
            <T>Commit Cloud is disabled in this repository</T>
          </Subtle>
        </div>
      )}

      {cloudSyncState?.value?.syncError == null ? null : (
        <div className="commit-cloud-row">
          <InlineErrorBadge error={cloudSyncState?.value?.syncError}>
            <T>Failed to fetch commit cloud backup statuses</T>
          </InlineErrorBadge>
        </div>
      )}
      {cloudSyncState?.value?.workspaceError == null ? null : (
        <div className="commit-cloud-row">
          <InlineErrorBadge error={cloudSyncState?.value?.workspaceError}>
            <T>Failed to fetch commit cloud status</T>
          </InlineErrorBadge>
        </div>
      )}
      <div className="commit-cloud-row">
        {cloudSyncState == null ? (
          <Icon icon="loading" />
        ) : cloudSyncState.error != null ? (
          <ErrorNotice
            error={cloudSyncState.error}
            title={t('Failed to check Commit Cloud state')}
          />
        ) : cloudSyncState.value.lastBackup == null ? null : (
          <>
            <Subtle>
              <T
                replace={{
                  $relTimeAgo: (
                    <Tooltip title={cloudSyncState.value.lastBackup.toLocaleString()}>
                      <RelativeDate date={cloudSyncState.value.lastBackup} />
                    </Tooltip>
                  ),
                }}>
                Last meaningful sync: $relTimeAgo
              </T>
            </Subtle>
            <FlexSpacer />
            <Button
              onClick={() => {
                runOperation(new CommitCloudSyncOperation()).then(() => {
                  refreshCommitCloudStatus();
                });
              }}
              disabled={isRunningSync}
              icon>
              {isRunningSync ? (
                <Icon icon="loading" slot="start" />
              ) : (
                <Icon icon="sync" slot="start" />
              )}
              <T>Sync now</T>
            </Button>
          </>
        )}
      </div>

      {cloudSyncState?.value?.commitStatuses == null ? null : (
        <CommitCloudSyncStatusBadge statuses={cloudSyncState?.value?.commitStatuses} />
      )}

      <div className="commit-cloud-row">
        {cloudSyncState?.value?.currentWorkspace == null ? null : (
          <div className="commit-cloud-dropdown-container">
            <label htmlFor="stack-file-dropdown">
              <T>Commit Cloud Workspace</T>
            </label>
            <div className="commit-cloud-workspace-actions">
              <Dropdown
                value={cloudSyncState?.value.currentWorkspace}
                disabled={
                  pendingOperation?.trackEventName === 'CommitCloudChangeWorkspaceOperation' ||
                  pendingOperation?.trackEventName === 'CommitCloudCreateWorkspaceOperation'
                }
                onChange={event => {
                  const newChoice = event.currentTarget.value;
                  runOperation(new CommitCloudChangeWorkspaceOperation(newChoice)).then(() => {
                    refreshCommitCloudStatus();
                  });
                  if (cloudSyncState?.value) {
                    // optimistically set the workspace choice
                    setCloudSyncState({
                      value: {...cloudSyncState?.value, currentWorkspace: newChoice},
                    });
                  }
                }}
                options={cloudSyncState?.value.workspaceChoices ?? []}
              />
              {enteredWorkspaceName == null ? (
                <Button
                  icon
                  onClick={e => {
                    setEnteredWorkspaceName('');
                    e.preventDefault();
                    e.stopPropagation();
                  }}>
                  <Icon icon="plus" slot="start" />
                  <T>Add Workspace</T>
                </Button>
              ) : (
                <div className="commit-cloud-new-workspace-input">
                  <TextField
                    ref={newWorkspaceNameRef as React.MutableRefObject<null>}
                    onInput={e => setEnteredWorkspaceName((e.target as HTMLInputElement).value)}>
                    <T>New Workspace Name</T>
                  </TextField>
                  <Button
                    onClick={e => {
                      setEnteredWorkspaceName(null);
                      e.preventDefault();
                      e.stopPropagation();
                    }}>
                    <T>Cancel</T>
                  </Button>
                  <Button
                    primary
                    disabled={!enteredWorkspaceName}
                    onClick={e => {
                      if (!enteredWorkspaceName) {
                        return;
                      }
                      const name = enteredWorkspaceName.trim().replace(' ', '_');
                      // optimistically update the dropdown
                      setCloudSyncState(old =>
                        old?.value != null
                          ? {
                              value: {
                                ...old.value,
                                workspaceChoices: [...(old.value.workspaceChoices ?? []), name],
                                currentWorkspace: name,
                              },
                            }
                          : old,
                      );
                      runOperation(new CommitCloudCreateWorkspaceOperation(name)).then(() => {
                        refreshCommitCloudStatus();
                      });
                      setEnteredWorkspaceName(null);
                      e.preventDefault();
                      e.stopPropagation();
                    }}>
                    <T>Create</T>
                  </Button>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function CommitCloudSyncStatusBadge({statuses}: {statuses: Map<Hash, CommitCloudBackupStatus>}) {
  const statusValues = [...statuses.entries()];
  const pending = statusValues.filter(
    ([_hash, status]) =>
      status === CommitCloudBackupStatus.Pending || status === CommitCloudBackupStatus.InProgress,
  );
  const failed = statusValues.filter(
    ([_hash, status]) => status === CommitCloudBackupStatus.Failed,
  );

  let icon;
  let content;
  let renderTooltip;
  if (pending.length > 0) {
    icon = 'sync';
    content = <T count={pending.length}>commitsBeingBackedUp</T>;
    renderTooltip = () => <BackupList commits={pending.map(([hash]) => hash)} />;
  } else if (failed.length > 0) {
    icon = 'sync';
    content = (
      <div className="inline-error-badge">
        <span>
          <Icon icon="error" slot="start" />
          <T count={failed.length}>commitsFailedBackingUp</T>
        </span>
      </div>
    );
    renderTooltip = () => <BackupList commits={failed.map(([hash]) => hash)} />;
  } else {
    // Empty means all commits were backed up, since we don't fetch successfully backed up hashes.
    // Note: this does mean we can't tell the difference between a commit we don't know about and a commit that is backed up.
    icon = 'check';
    content = <T>All commits backed up</T>;
  }

  return (
    <div className="commit-cloud-row commit-cloud-sync-status-badge">
      {renderTooltip == null ? (
        <div>
          <Icon icon={icon} />
          {content}
        </div>
      ) : (
        <Tooltip component={renderTooltip}>
          <Icon icon={icon} />
          {content}
        </Tooltip>
      )}
    </div>
  );
}

function BackupList({commits}: {commits: Array<Hash>}) {
  const dag = useAtomValue(dagWithPreviews);
  const infos = commits.map(hash => dag.get(hash)).filter(notEmpty);
  return (
    <div className="commit-cloud-backup-list">
      {infos.map(commit =>
        typeof commit === 'string' ? (
          <div>{commit}</div>
        ) : (
          <Commit
            commit={commit}
            key={commit.hash}
            hasChildren={false}
            previewType={CommitPreview.NON_ACTIONABLE_COMMIT}
          />
        ),
      )}
    </div>
  );
}
