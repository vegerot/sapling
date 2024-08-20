/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Place, UIChangedFile, VisualChangedFileType} from './UncommittedChanges';
import type {UseUncommittedSelection} from './partialSelection';
import type {ChangedFileType, GeneratedStatus} from './types';
import type {ReactNode} from 'react';
import type {Comparison} from 'shared/Comparison';

import {copyUrlForFile, supportsBrowseUrlForHash} from './BrowseRepo';
import {type ChangedFilesDisplayType} from './ChangedFileDisplayTypePicker';
import {generatedStatusToLabel, generatedStatusDescription} from './GeneratedFile';
import {PartialFileSelectionWithMode} from './PartialFileSelection';
import {SuspenseBoundary} from './SuspenseBoundary';
import {holdingAltAtom, holdingCtrlAtom} from './atoms/keyboardAtoms';
import {externalMergeToolAtom} from './externalMergeTool';
import {T, t} from './i18n';
import {readAtom} from './jotaiUtils';
import {CONFLICT_SIDE_LABELS} from './mergeConflicts/state';
import {AddOperation} from './operations/AddOperation';
import {ForgetOperation} from './operations/ForgetOperation';
import {PurgeOperation} from './operations/PurgeOperation';
import {ResolveInExternalMergeToolOperation} from './operations/ResolveInExternalMergeToolOperation';
import {ResolveOperation, ResolveTool} from './operations/ResolveOperation';
import {RevertOperation} from './operations/RevertOperation';
import {RmOperation} from './operations/RmOperation';
import {useRunOperation} from './operationsState';
import {useUncommittedSelection} from './partialSelection';
import platform from './platform';
import {optimisticMergeConflicts} from './previews';
import {copyAndShowToast} from './toast';
import {ConflictType, succeedableRevset} from './types';
import {usePromise} from './usePromise';
import {Button} from 'isl-components/Button';
import {Checkbox} from 'isl-components/Checkbox';
import {Icon} from 'isl-components/Icon';
import {Subtle} from 'isl-components/Subtle';
import {Tooltip} from 'isl-components/Tooltip';
import {useAtomValue} from 'jotai';
import React from 'react';
import {labelForComparison, revsetForComparison, ComparisonType} from 'shared/Comparison';
import {useContextMenu} from 'shared/ContextMenu';
import {isMac} from 'shared/OperatingSystem';
import {basename, notEmpty} from 'shared/utils';

/**
 * Is the alt key currently held down, used to show full file paths.
 * On windows, this actually uses the ctrl key instead to avoid conflicting with OS focus behaviors.
 */
const holdingModifiedKeyAtom = isMac ? holdingAltAtom : holdingCtrlAtom;

export function File({
  file,
  displayType,
  comparison,
  selection,
  place,
  generatedStatus,
}: {
  file: UIChangedFile;
  displayType: ChangedFilesDisplayType;
  comparison: Comparison;
  selection?: UseUncommittedSelection;
  place?: Place;
  generatedStatus?: GeneratedStatus;
}) {
  const clipboardCopy = (text: string) => copyAndShowToast(text);

  // Renamed files are files which have a copy field, where that path was also removed.

  // Visually show renamed files as if they were modified, even though sl treats them as added.
  const [statusName, icon] = nameAndIconForFileStatus[file.visualStatus];

  const generated = generatedStatusToLabel(generatedStatus);

  const contextMenu = useContextMenu(() => {
    const options = [
      {label: t('Copy File Path'), onClick: () => clipboardCopy(file.path)},
      {label: t('Copy Filename'), onClick: () => clipboardCopy(basename(file.path))},
      {label: t('Open File'), onClick: () => platform.openFile(file.path)},
    ];

    if (platform.openContainingFolder != null) {
      options.push({
        label: t('Open Containing Folder'),
        onClick: () => platform.openContainingFolder?.(file.path),
      });
    }
    if (platform.openDiff != null) {
      options.push({
        label: t('Open Diff View ($comparison)', {
          replace: {$comparison: labelForComparison(comparison)},
        }),
        onClick: () => platform.openDiff?.(file.path, comparison),
      });
    }

    if (readAtom(supportsBrowseUrlForHash)) {
      options.push({
        label: t('Copy file URL'),
        onClick: () => {
          copyUrlForFile(file.path, comparison);
        },
      });
    }
    return options;
  });

  const runOperation = useRunOperation();

  // Hold "alt" key to show full file paths instead of short form.
  // This is a quick way to see where a file comes from without
  // needing to go through the menu to change the rendering type.
  const isHoldingAlt = useAtomValue(holdingModifiedKeyAtom);

  const tooltip = [file.tooltip, generatedStatusDescription(generatedStatus)]
    .filter(notEmpty)
    .join('\n\n');

  const openFile = () => {
    if (file.visualStatus === 'U') {
      const tool = readAtom(externalMergeToolAtom);
      if (tool != null) {
        runOperation(new ResolveInExternalMergeToolOperation(tool, file.path));
        return;
      }
    }
    platform.openFile(file.path);
  };

  return (
    <>
      <div
        className={`changed-file file-${statusName} file-${generated}`}
        data-testid={`changed-file-${file.path}`}
        onContextMenu={contextMenu}
        key={file.path}
        tabIndex={0}
        onKeyUp={e => {
          if (e.key === 'Enter') {
            openFile();
          }
        }}>
        <FileSelectionCheckbox file={file} selection={selection} />
        <span className="changed-file-path" onClick={openFile}>
          <Icon icon={icon} />
          <Tooltip title={tooltip} delayMs={2_000} placement="right">
            <span
              className="changed-file-path-text"
              onCopy={e => {
                const selection = document.getSelection();
                if (selection) {
                  // we inserted LTR markers, remove them again on copy
                  e.clipboardData.setData(
                    'text/plain',
                    selection.toString().replace(/\u200E/g, ''),
                  );
                  e.preventDefault();
                }
              }}>
              {escapeForRTL(
                displayType === 'tree'
                  ? file.path.slice(file.path.lastIndexOf('/') + 1)
                  : // Holding alt takes precedence over fish/short styles, but not tree.
                  displayType === 'fullPaths' || isHoldingAlt
                  ? file.path
                  : displayType === 'fish'
                  ? file.path
                      .split('/')
                      .map((a, i, arr) => (i === arr.length - 1 ? a : a[0]))
                      .join('/')
                  : file.label,
              )}
            </span>
          </Tooltip>
        </span>
        <FileActions file={file} comparison={comparison} place={place} />
      </div>
      {place === 'main' && selection?.isExpanded(file.path) && (
        <MaybePartialSelection file={file} />
      )}
    </>
  );
}

const revertableStatues = new Set(['M', 'R', '!']);
const conflictStatuses = new Set<ChangedFileType>(['U', 'Resolved']);
function FileActions({
  comparison,
  file,
  place,
}: {
  comparison: Comparison;
  file: UIChangedFile;
  place?: Place;
}) {
  const runOperation = useRunOperation();
  const conflicts = useAtomValue(optimisticMergeConflicts);

  const conflictData = conflicts?.files?.find(f => f.path === file.path);
  const label = labelForConflictType(conflictData?.conflictType);
  let conflictLabel = null;
  if (label) {
    conflictLabel = <Subtle>{label}</Subtle>;
  }

  const actions: Array<React.ReactNode> = [];

  if (platform.openDiff != null && !conflictStatuses.has(file.status)) {
    actions.push(
      <Tooltip title={t('Open diff view')} key="open-diff-view" delayMs={1000}>
        <Button
          className="file-show-on-hover"
          icon
          data-testid="file-open-diff-button"
          onClick={() => {
            platform.openDiff?.(file.path, comparison);
          }}>
          <Icon icon="request-changes" />
        </Button>
      </Tooltip>,
    );
  }

  if (
    (revertableStatues.has(file.status) && comparison.type !== ComparisonType.Committed) ||
    // special case: reverting does actually work for added files in the head commit
    (comparison.type === ComparisonType.HeadChanges && file.status === 'A')
  ) {
    actions.push(
      <Tooltip
        title={
          comparison.type === ComparisonType.UncommittedChanges
            ? t('Revert back to last commit')
            : t('Revert changes made by this commit')
        }
        key="revert"
        delayMs={1000}>
        <Button
          className="file-show-on-hover"
          key={file.path}
          icon
          data-testid="file-revert-button"
          onClick={() => {
            platform
              .confirm(
                comparison.type === ComparisonType.UncommittedChanges
                  ? t('Are you sure you want to revert $file?', {replace: {$file: file.path}})
                  : t(
                      'Are you sure you want to revert $file back to how it was just before the last commit? Uncommitted changes to this file will be lost.',
                      {replace: {$file: file.path}},
                    ),
              )
              .then(ok => {
                if (!ok) {
                  return;
                }
                runOperation(
                  new RevertOperation(
                    [file.path],
                    comparison.type === ComparisonType.UncommittedChanges
                      ? undefined
                      : succeedableRevset(revsetForComparison(comparison)),
                  ),
                );
              });
          }}>
          <Icon icon="discard" />
        </Button>
      </Tooltip>,
    );
  }

  if (comparison.type === ComparisonType.UncommittedChanges) {
    if (file.status === 'A') {
      actions.push(
        <Tooltip
          title={t('Stop tracking this file, without removing from the filesystem')}
          key="forget"
          delayMs={1000}>
          <Button
            className="file-show-on-hover"
            key={file.path}
            icon
            onClick={() => {
              runOperation(new ForgetOperation(file.path));
            }}>
            <Icon icon="circle-slash" />
          </Button>
        </Tooltip>,
      );
    } else if (file.status === '?') {
      actions.push(
        <Tooltip title={t('Start tracking this file')} key="add" delayMs={1000}>
          <Button
            className="file-show-on-hover"
            key={file.path}
            icon
            onClick={() => runOperation(new AddOperation(file.path))}>
            <Icon icon="add" />
          </Button>
        </Tooltip>,
        <Tooltip title={t('Remove this file from the filesystem')} key="remove" delayMs={1000}>
          <Button
            className="file-show-on-hover"
            key={file.path}
            icon
            data-testid="file-action-delete"
            onClick={async () => {
              const ok = await platform.confirm(
                t('Are you sure you want to delete $file?', {replace: {$file: file.path}}),
              );
              if (!ok) {
                return;
              }
              runOperation(new PurgeOperation([file.path]));
            }}>
            <Icon icon="trash" />
          </Button>
        </Tooltip>,
      );
    } else if (file.status === 'Resolved') {
      actions.push(
        <Tooltip title={t('Mark as unresolved')} key="unresolve-mark">
          <Button
            key={file.path}
            icon
            onClick={() => runOperation(new ResolveOperation(file.path, ResolveTool.unmark))}>
            <Icon icon="circle-slash" />
          </Button>
        </Tooltip>,
      );
    } else if (file.status === 'U') {
      actions.push(
        <Tooltip title={t('Mark as resolved')} key="resolve-mark">
          <Button
            className="file-show-on-hover"
            data-testid="file-action-resolve"
            key={file.path}
            icon
            onClick={() => runOperation(new ResolveOperation(file.path, ResolveTool.mark))}>
            <Icon icon="check" />
          </Button>
        </Tooltip>,
      );
      if (
        conflictData?.conflictType &&
        [ConflictType.DeletedInSource, ConflictType.DeletedInDest].includes(
          conflictData.conflictType,
        )
      ) {
        actions.push(
          <Tooltip title={t('Delete file')} key="resolve-delete">
            <Button
              className="file-show-on-hover"
              data-testid="file-action-resolve-delete"
              icon
              onClick={() => {
                runOperation(new RmOperation(file.path, /* force */ true));
                // then explicitly mark the file as resolved
                runOperation(new ResolveOperation(file.path, ResolveTool.mark));
              }}>
              <Icon icon="trash" />
            </Button>
          </Tooltip>,
        );
      } else {
        actions.push(
          <Tooltip
            title={t('Take $local', {
              replace: {$local: CONFLICT_SIDE_LABELS.local},
            })}
            key="resolve-local">
            <Button
              className="file-show-on-hover"
              key={file.path}
              icon
              onClick={() => runOperation(new ResolveOperation(file.path, ResolveTool.local))}>
              <Icon icon="fold-up" />
            </Button>
          </Tooltip>,
          <Tooltip
            title={t('Take $incoming', {
              replace: {$incoming: CONFLICT_SIDE_LABELS.incoming},
            })}
            key="resolve-other">
            <Button
              className="file-show-on-hover"
              key={file.path}
              icon
              onClick={() => runOperation(new ResolveOperation(file.path, ResolveTool.other))}>
              <Icon icon="fold-down" />
            </Button>
          </Tooltip>,
          <Tooltip
            title={t('Combine both $incoming and $local', {
              replace: {
                $local: CONFLICT_SIDE_LABELS.local,
                $incoming: CONFLICT_SIDE_LABELS.incoming,
              },
            })}
            key="resolve-both">
            <Button
              className="file-show-on-hover"
              key={file.path}
              icon
              onClick={() => runOperation(new ResolveOperation(file.path, ResolveTool.both))}>
              <Icon icon="fold" />
            </Button>
          </Tooltip>,
        );
      }
    }

    if (place === 'main' && conflicts == null) {
      actions.push(<PartialSelectionAction file={file} key="partial-selection" />);
    }
  }
  return (
    <div className="file-actions" data-testid="file-actions">
      {conflictLabel}
      {actions}
    </div>
  );
}

function labelForConflictType(type?: ConflictType) {
  switch (type) {
    case ConflictType.DeletedInSource:
      return t('(Deleted in $incoming)', {
        replace: {$incoming: CONFLICT_SIDE_LABELS.incoming},
      });

    case ConflictType.DeletedInDest:
      return t('(Deleted in $local)', {replace: {$local: CONFLICT_SIDE_LABELS.local}});
    default:
      return null;
  }
}

/**
 * We render file paths with CSS text-direction: rtl,
 * which allows the ellipsis overflow to appear on the left.
 * However, rtl can have weird effects, such as moving leading '.' to the end.
 * To fix this, it's enough to add a left-to-right marker at the start of the path
 */
function escapeForRTL(s: string): ReactNode {
  return '\u200E' + s + '\u200E';
}

function FileSelectionCheckbox({
  file,
  selection,
}: {
  file: UIChangedFile;
  selection?: UseUncommittedSelection;
}) {
  const checked = selection?.isFullyOrPartiallySelected(file.path) ?? false;
  return selection == null ? null : (
    <Checkbox
      aria-label={t('$label $file', {
        replace: {$label: checked ? 'unselect' : 'select', $file: file.path},
      })}
      checked={checked}
      indeterminate={selection.isPartiallySelected(file.path)}
      data-testid={'file-selection-checkbox'}
      onChange={checked => {
        if (checked) {
          if (file.renamedFrom != null) {
            // Selecting a renamed file also selects the original, so they are committed/amended together
            // the UI merges them visually anyway.
            selection.select(file.renamedFrom, file.path);
          } else {
            selection.select(file.path);
          }
        } else {
          if (file.renamedFrom != null) {
            selection.deselect(file.renamedFrom, file.path);
          } else {
            selection.deselect(file.path);
          }
        }
      }}
    />
  );
}

function PartialSelectionAction({file}: {file: UIChangedFile}) {
  const selection = useUncommittedSelection();

  const handleClick = () => {
    selection.toggleExpand(file.path);
  };

  return (
    <Tooltip
      component={() => (
        <div style={{maxWidth: '300px'}}>
          <div>
            <T>Toggle chunk selection</T>
          </div>
          <div>
            <Subtle>
              <T>
                Shows changed files in your commit and lets you select individual chunks or lines to
                include.
              </T>
            </Subtle>
          </div>
        </div>
      )}>
      <Button className="file-show-on-hover" icon onClick={handleClick}>
        <Icon icon="diff" />
      </Button>
    </Tooltip>
  );
}

// Left margin to "indendent" by roughly a checkbox width.
const leftMarginStyle: React.CSSProperties = {marginLeft: 'calc(2.5 * var(--pad))'};

function MaybePartialSelection({file}: {file: UIChangedFile}) {
  const fallback = (
    <div style={leftMarginStyle}>
      <Icon icon="loading" />
    </div>
  );
  return (
    <SuspenseBoundary fallback={fallback}>
      <PartialSelectionPanel file={file} />
    </SuspenseBoundary>
  );
}

function PartialSelectionPanel({file}: {file: UIChangedFile}) {
  const path = file.path;
  const selection = useUncommittedSelection();
  const chunkSelect = usePromise(selection.getChunkSelect(path));

  return (
    <div style={leftMarginStyle}>
      <PartialFileSelectionWithMode
        chunkSelection={chunkSelect}
        setChunkSelection={state => selection.editChunkSelect(path, state)}
        mode="unified"
      />
    </div>
  );
}

/**
 * Map for changed files statuses into classNames (for color & styles) and icon names.
 */
const nameAndIconForFileStatus: Record<VisualChangedFileType, [string, string]> = {
  A: ['added', 'diff-added'],
  M: ['modified', 'diff-modified'],
  R: ['removed', 'diff-removed'],
  '?': ['ignored', 'question'],
  '!': ['missing', 'warning'],
  U: ['unresolved', 'diff-ignored'],
  Resolved: ['resolved', 'pass'],
  Renamed: ['modified', 'diff-renamed'],
  Copied: ['added', 'diff-added'],
};
