/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {FieldsBeingEdited} from '../../CommitInfoView/types';
import type {CommitInfo} from '../../types';
import type {MutableRefObject} from 'react';

import {Commit} from '../../Commit';
import {
  editedCommitMessages,
  getDefaultEditedCommitMessage,
  unsavedFieldsBeingEdited,
} from '../../CommitInfoView/CommitInfoState';
import {commitMessageFieldsSchema} from '../../CommitInfoView/CommitMessageFields';
import {FlexSpacer} from '../../ComponentUtils';
import {T, t} from '../../i18n';
import {readAtom, writeAtom} from '../../jotaiUtils';
import {CommitPreview} from '../../previews';
import {useModal} from '../../useModal';
import {Button} from 'isl-components/Button';
import {Divider} from 'isl-components/Divider';
import {Icon} from 'isl-components/Icon';
import {useAtomValue} from 'jotai';
import {useCallback} from 'react';
import {useAutofocusRef} from 'shared/hooks';

import './ConfirmUnsavedEditsBeforeSplit.css';

type UnsavedEditConfirmKind = 'split' | 'edit_stack';

export function useConfirmUnsavedEditsBeforeSplit(): (
  commits: Array<CommitInfo>,
  kind: UnsavedEditConfirmKind,
) => Promise<boolean> {
  const showModal = useModal();
  const showConfirmation = useCallback(
    async (commits: Array<CommitInfo>, kind: UnsavedEditConfirmKind): Promise<boolean> => {
      const editedCommits = commits
        .map(commit => [commit, readAtom(unsavedFieldsBeingEdited(commit.hash))])
        .filter(([_, f]) => f != null) as Array<[CommitInfo, FieldsBeingEdited]>;
      if (editedCommits.some(([_, f]) => Object.values(f).some(Boolean))) {
        const continueWithSplit = await showModal<boolean>({
          type: 'custom',
          component: ({returnResultAndDismiss}) => (
            <PreSplitUnsavedEditsConfirmationModal
              kind={kind}
              editedCommits={editedCommits}
              returnResultAndDismiss={returnResultAndDismiss}
            />
          ),
          title:
            kind === 'split'
              ? t('Save edits before splitting?')
              : t('Save edits before editing stack?'),
        });
        return continueWithSplit === true;
      }
      return true;
    },
    [showModal],
  );

  return (commits: Array<CommitInfo>, kind: UnsavedEditConfirmKind) => {
    return showConfirmation(commits, kind);
  };
}

function PreSplitUnsavedEditsConfirmationModal({
  kind,
  editedCommits,
  returnResultAndDismiss,
}: {
  kind: UnsavedEditConfirmKind;
  editedCommits: Array<[CommitInfo, FieldsBeingEdited]>;
  returnResultAndDismiss: (continueWithSplit: boolean) => unknown;
}) {
  const schema = useAtomValue(commitMessageFieldsSchema);

  const resetEditedCommitMessage = useCallback((commit: CommitInfo) => {
    writeAtom(editedCommitMessages(commit.hash), getDefaultEditedCommitMessage());
  }, []);

  const commitsWithUnsavedEdits = editedCommits.filter(([_, fields]) =>
    Object.values(fields).some(Boolean),
  );

  const saveButtonRef = useAutofocusRef();

  return (
    <div className="confirm-unsaved-edits-pre-split" data-testid="confirm-unsaved-edits-pre-split">
      <>
        <div>
          <T count={commitsWithUnsavedEdits.length}>
            {kind === 'split'
              ? 'confirmUnsavedEditsBeforeSplit'
              : 'confirmUnsavedEditsBeforeEditStack'}
          </T>
        </div>
        <div className="commits-with-unsaved-changes">
          {commitsWithUnsavedEdits.map(([commit, fields]) => (
            <div className="commit-row" key={commit.hash}>
              <Commit
                commit={commit}
                hasChildren={false}
                previewType={CommitPreview.NON_ACTIONABLE_COMMIT}
              />
              <span key={`${commit.hash}-fields`} className="byline">
                <T
                  replace={{
                    $commitTitle: commit.title,
                    $fields: (
                      <>
                        {Object.entries(fields)
                          .filter(([, value]) => value)
                          .map(([field]) => {
                            const icon = schema.find(f => f.key === field)?.icon;
                            return (
                              <span key={field} className="field-name">
                                {icon && <Icon icon={icon} />}
                                {field}
                              </span>
                            );
                          })}
                      </>
                    ),
                  }}>
                  unsaved changes to $fields
                </T>
              </span>
            </div>
          ))}
        </div>
        <Divider />
        <div className="use-modal-buttons">
          <FlexSpacer />
          <Button onClick={() => returnResultAndDismiss(false)}>
            <T>Cancel</T>
          </Button>
          <Button
            onClick={() => {
              for (const [commit] of editedCommits) {
                resetEditedCommitMessage(commit);
              }
              returnResultAndDismiss(true); // continue with split
            }}>
            <T>Discard Edits</T>
          </Button>
          <Button
            ref={saveButtonRef as MutableRefObject<null>}
            primary
            onClick={() => {
              // Unsaved edits will be automatically loaded by the split as the commits' text
              returnResultAndDismiss(true); // continue with split
            }}>
            <T>Save Edits</T>
          </Button>
        </div>
      </>
    </div>
  );
}
