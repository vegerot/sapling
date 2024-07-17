/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {TokenizedDiffHunk, TokenizedHunk} from './syntaxHighlightingTypes';
import type {Context, OneIndexedLineNumber} from './types';
import type {ReactNode} from 'react';
import type {Hunk, ParsedDiff} from 'shared/patch/parse';

import {T, t} from '../../i18n';
import {useFetchLines} from '../ComparisonView';
import SplitDiffRow, {BlankLineNumber} from './SplitDiffRow';
import {useTableColumnSelection} from './copyFromSelectedColumn';
import {useTokenizedContents, useTokenizedHunks} from './syntaxHighlighting';
import {diffChars} from 'diff';
import {ErrorNotice} from 'isl-components/ErrorNotice';
import {Icon} from 'isl-components/Icon';
import React, {useCallback, useState} from 'react';
import organizeLinesIntoGroups from 'shared/SplitDiffView/organizeLinesIntoGroups';
import {
  applyTokenizationToLine,
  createTokenizedIntralineDiff,
} from 'shared/createTokenizedIntralineDiff';

const MAX_INPUT_LENGTH_FOR_INTRALINE_DIFF = 300;

export type SplitDiffTableProps = {
  ctx: Context;
  path: string;
  patch: ParsedDiff;
};

export const SplitDiffTable = React.memo(
  ({ctx, path, patch}: SplitDiffTableProps): React.ReactElement => {
    const [deletedFileExpanded, setDeletedFileExpanded] = useState<boolean>(false);
    const [expandedSeparators, setExpandedSeparators] = useState<Readonly<Set<string>>>(
      () => new Set(),
    );
    const onExpand = useCallback(
      (key: string) => {
        const amendedSet = new Set(expandedSeparators);
        amendedSet.add(key);
        setExpandedSeparators(amendedSet);
      },
      [expandedSeparators, setExpandedSeparators],
    );

    const tokenization = useTokenizedHunks(patch.newFileName ?? '', patch.hunks);

    const {className: tableSelectionClassName, ...tableSelectionProps} = useTableColumnSelection();

    const isDeleted = patch.newFileName === '/dev/null';
    const isAdded = patch.type === 'Added';

    const unified = ctx.display === 'unified';

    const {hunks} = patch;
    const lastHunkIndex = hunks.length - 1;
    const rows: React.ReactElement[] = [];
    if (!isDeleted || deletedFileExpanded) {
      hunks.forEach((hunk, index) => {
        // Show a separator before the first hunk if the file starts with a
        // section of unmodified lines that is hidden by default.
        if (index === 0 && (hunk.oldStart !== 1 || hunk.newStart !== 1)) {
          // TODO: test empty file that went from 644 to 755?
          const key = 's0';
          if (expandedSeparators.has(key)) {
            rows.push(
              <ExpandingSeparator
                key={key}
                ctx={ctx}
                path={path}
                start={1}
                numLines={hunk.oldStart - 1}
                beforeLineStart={1}
                afterLineStart={1}
              />,
            );
          } else if (ctx.supportsExpandingContext) {
            const numLines = Math.max(hunk.oldStart, hunk.newStart) - 1;
            rows.push(
              <HunkSeparator key={key} numLines={numLines} onExpand={() => onExpand(key)} t={t} />,
            );
          }
        }

        addRowsForHunk(unified, hunk, path, rows, tokenization?.[index], ctx.openFileToLine);

        const isLast = index === lastHunkIndex;
        const nextHunk = hunks[index + 1];
        const key = `s${hunk.oldStart}`;
        const canExpand = !isLast || !(isAdded || isDeleted || isHunkProbablyAtEndOfFile(hunk)); // added and deleted files are already expanded
        if (canExpand) {
          if (expandedSeparators.has(key)) {
            const start = hunk.oldStart + hunk.oldLines;
            const MAX_LINES_FETCH = 10000; // We don't know the total number of lines, so for the last hunk we just request a lot of lines.
            const numLines = isLast ? MAX_LINES_FETCH : nextHunk.oldStart - start;
            rows.push(
              <ExpandingSeparator
                key={key}
                ctx={ctx}
                start={start}
                numLines={numLines}
                path={path}
                beforeLineStart={hunk.oldStart + hunk.oldLines}
                afterLineStart={hunk.newStart + hunk.newLines}
              />,
            );
          } else if (ctx.supportsExpandingContext) {
            const numLines = isLast ? null : nextHunk.oldStart - hunk.oldLines - hunk.oldStart;
            rows.push(
              <HunkSeparator key={key} numLines={numLines} onExpand={() => onExpand(key)} t={t} />,
            );
          }
        }
      });
    } else {
      rows.push(
        <SeparatorRow>
          <InlineRowButton
            key={'show-deleted'}
            label={t('Show deleted file')}
            onClick={() => setDeletedFileExpanded(true)}
          />
        </SeparatorRow>,
      );
    }

    if (unified) {
      return (
        <table
          className={
            'split-diff-view-hunk-table display-unified ' + (tableSelectionClassName ?? '')
          }
          {...tableSelectionProps}>
          <colgroup>
            <col width={50} />
            <col width={50} />
            <col width={'100%'} />
          </colgroup>
          <tbody>{rows}</tbody>
        </table>
      );
    }
    return (
      <table
        className={'split-diff-view-hunk-table display-split ' + (tableSelectionClassName ?? '')}
        {...tableSelectionProps}>
        <colgroup>
          <col width={50} />
          <col width={'50%'} />
          <col width={50} />
          <col width={'50%'} />
        </colgroup>
        <tbody>{rows}</tbody>
      </table>
    );
  },
);

/**
 * If the last hunk of a file doesn't have as many context lines as it should,
 * it's because it's at the end of the file. This is a clue we can skip showing
 * the expander.
 * This util should only be called on the last hunk in the file.
 */
function isHunkProbablyAtEndOfFile(hunk: Hunk): boolean {
  // we could conceivably check if the initial context length matches the end length, but that's not true in short files.
  const CONTEXT_LENGTH = 4;
  return !hunk.lines.slice(-CONTEXT_LENGTH).every(line => line.startsWith(' '));
}

/**
 * Adds new rows to the supplied `rows` array.
 */
function addRowsForHunk(
  unified: boolean,
  hunk: Hunk,
  path: string,
  rows: React.ReactElement[],
  tokenization: TokenizedDiffHunk | undefined,
  openFileToLine?: (line: OneIndexedLineNumber) => unknown,
): void {
  const {oldStart, newStart, lines} = hunk;
  const groups = organizeLinesIntoGroups(lines);
  let beforeLineNumber = oldStart;
  let afterLineNumber = newStart;

  let beforeTokenizedIndex = 0;
  let afterTokenizedIndex = 0;

  groups.forEach(group => {
    const {common, removed, added} = group;
    addUnmodifiedRows(
      unified,
      common,
      path,
      'common',
      beforeLineNumber,
      afterLineNumber,
      rows,
      tokenization?.[0].slice(beforeTokenizedIndex),
      tokenization?.[1].slice(afterTokenizedIndex),
      openFileToLine,
    );
    beforeLineNumber += common.length;
    afterLineNumber += common.length;
    beforeTokenizedIndex += common.length;
    afterTokenizedIndex += common.length;

    // split content, or before lines when unified
    const linesA = [];
    // after lines when unified, or empty when using "split"
    const linesB = [];

    const maxIndex = Math.max(removed.length, added.length);
    for (let index = 0; index < maxIndex; ++index) {
      const removedLine = removed[index];
      const addedLine = added[index];
      if (removedLine != null && addedLine != null) {
        let beforeAndAfter;

        if (tokenization != null) {
          beforeAndAfter = createTokenizedIntralineDiff(
            removedLine,
            tokenization[0][beforeTokenizedIndex],
            addedLine,
            tokenization[1][afterTokenizedIndex],
          );
        } else {
          beforeAndAfter = createIntralineDiff(removedLine, addedLine);
        }

        const [before, after] = beforeAndAfter;
        const [beforeLine, beforeChange, afterLine, afterChange] = SplitDiffRow({
          beforeLineNumber,
          before,
          afterLineNumber,
          after,
          rowType: 'modify',
          path,
          unified,
          openFileToLine,
        });

        if (unified) {
          linesA.push(
            <tr key={`${beforeLineNumber}/${afterLineNumber}:b`}>
              {beforeLine}
              <BlankLineNumber before />
              {beforeChange}
            </tr>,
          );
          linesB.push(
            <tr key={`${beforeLineNumber}/${afterLineNumber}:a`}>
              <BlankLineNumber after />
              {afterLine}
              {afterChange}
            </tr>,
          );
        } else {
          linesA.push(
            <tr key={`${beforeLineNumber}/${afterLineNumber}`}>
              {beforeLine}
              {beforeChange}
              {afterLine}
              {afterChange}
            </tr>,
          );
        }
        ++beforeLineNumber;
        ++afterLineNumber;
        ++beforeTokenizedIndex;
        ++afterTokenizedIndex;
      } else if (removedLine != null) {
        const [beforeLine, beforeChange, afterLine, afterChange] = SplitDiffRow({
          beforeLineNumber,
          before:
            tokenization?.[0] == null
              ? removedLine
              : applyTokenizationToLine(removedLine, tokenization[0][beforeTokenizedIndex]),
          afterLineNumber: null,
          after: null,
          rowType: 'remove',
          path,
          unified,
          openFileToLine,
        });

        if (unified) {
          linesA.push(
            <tr key={`${beforeLineNumber}/`}>
              {beforeLine}
              <BlankLineNumber before />
              {beforeChange}
            </tr>,
          );
        } else {
          linesA.push(
            <tr key={`${beforeLineNumber}/`}>
              {beforeLine}
              {beforeChange}
              {afterLine}
              {afterChange}
            </tr>,
          );
        }
        ++beforeLineNumber;
        ++beforeTokenizedIndex;
      } else {
        const [beforeLine, beforeChange, afterLine, afterChange] = SplitDiffRow({
          beforeLineNumber: null,
          before: null,
          afterLineNumber,
          after:
            tokenization?.[1] == null
              ? addedLine
              : applyTokenizationToLine(addedLine, tokenization[1][afterTokenizedIndex]),
          rowType: 'add',
          path,
          unified,
          openFileToLine,
        });

        if (unified) {
          linesB.push(
            <tr key={`/${afterLineNumber}`}>
              <BlankLineNumber after />
              {afterLine}
              {afterChange}
            </tr>,
          );
        } else {
          linesA.push(
            <tr key={`/${afterLineNumber}`}>
              {beforeLine}
              {beforeChange}
              {afterLine}
              {afterChange}
            </tr>,
          );
        }
        ++afterLineNumber;
        ++afterTokenizedIndex;
      }
    }

    rows.push(...linesA, ...linesB);
  });
}

function InlineRowButton({onClick, label}: {onClick: () => unknown; label: ReactNode}) {
  return (
    // TODO: tabindex or make this a button for accessibility
    <div className="split-diff-view-inline-row-button" onClick={onClick}>
      <Icon icon="unfold" />
      <span className="inline-row-button-label">{label}</span>
      <Icon icon="unfold" />
    </div>
  );
}

/**
 * Adds new rows to the supplied `rows` array.
 */
function addUnmodifiedRows(
  unified: boolean,
  lines: string[],
  path: string,
  rowType: 'common' | 'expanded',
  initialBeforeLineNumber: number,
  initialAfterLineNumber: number,
  rows: React.ReactElement[],
  tokenizationBefore?: TokenizedHunk | undefined,
  tokenizationAfter?: TokenizedHunk | undefined,
  openFileToLine?: (line: OneIndexedLineNumber) => unknown,
): void {
  let beforeLineNumber = initialBeforeLineNumber;
  let afterLineNumber = initialAfterLineNumber;
  lines.forEach((lineContent, i) => {
    const [beforeLine, beforeChange, afterLine, afterChange] = SplitDiffRow({
      beforeLineNumber,
      before:
        tokenizationBefore?.[i] == null
          ? lineContent
          : applyTokenizationToLine(lineContent, tokenizationBefore[i]),
      afterLineNumber,
      after:
        tokenizationAfter?.[i] == null
          ? lineContent
          : applyTokenizationToLine(lineContent, tokenizationAfter[i]),
      rowType,
      path,
      unified,
      openFileToLine,
    });
    if (unified) {
      rows.push(
        <tr key={`${beforeLineNumber}/${afterLineNumber}`}>
          {beforeLine}
          {afterLine}
          {beforeChange}
        </tr>,
      );
    } else {
      rows.push(
        <tr key={`${beforeLineNumber}/${afterLineNumber}`}>
          {beforeLine}
          {beforeChange}
          {afterLine}
          {afterChange}
        </tr>,
      );
    }
    ++beforeLineNumber;
    ++afterLineNumber;
  });
}

function createIntralineDiff(
  before: string,
  after: string,
): [React.ReactFragment, React.ReactFragment] {
  // For lines longer than this, diffChars() can get very expensive to compute
  // and is likely of little value to the user.
  if (before.length + after.length > MAX_INPUT_LENGTH_FOR_INTRALINE_DIFF) {
    return [before, after];
  }

  const changes = diffChars(before, after);
  const beforeElements: React.ReactNode[] = [];
  const afterElements: React.ReactNode[] = [];
  changes.forEach((change, index) => {
    const {added, removed, value} = change;
    if (added) {
      afterElements.push(
        <span key={index} className="patch-add-word">
          {value}
        </span>,
      );
    } else if (removed) {
      beforeElements.push(
        <span key={index} className="patch-remove-word">
          {value}
        </span>,
      );
    } else {
      beforeElements.push(value);
      afterElements.push(value);
    }
  });

  return [beforeElements, afterElements];
}

/**
 * Visual element to delimit the discontinuity in a SplitDiffView.
 */
function HunkSeparator({
  numLines,
  onExpand,
  t,
}: {
  numLines: number | null;
  onExpand: () => unknown;
  t: (s: string) => string;
}): React.ReactElement | null {
  if (numLines === 0) {
    return null;
  }
  // TODO: Ensure numLines is never below a certain threshold: it takes up more
  // space to display the separator than it does to display the text (though
  // admittedly fetching the collapsed text is an async operation).
  const label =
    numLines == null
      ? // to expand the remaining lines at the end of the file, we don't know the size ahead of time,
        // just omit the amount to be expanded
        t('Expand lines')
      : numLines === 1
      ? t('Expand 1 line')
      : t(`Expand ${numLines} lines`);
  return (
    <SeparatorRow>
      <InlineRowButton label={label} onClick={onExpand} />
    </SeparatorRow>
  );
}

/**
 * This replaces a <HunkSeparator> when the user clicks on it to expand the
 * hidden file contents.
 * By rendering this, additional lines are automatically fetched.
 */
function ExpandingSeparator({
  ctx,
  path,
  start,
  numLines,
  beforeLineStart,
  afterLineStart,
}: {
  ctx: Context;
  path: string;
  numLines: number;
  start: number;
  beforeLineStart: number;
  afterLineStart: number;
}): React.ReactElement {
  const result = useFetchLines(ctx, numLines, start);

  const tokenization = useTokenizedContents(path, result?.value);
  if (result == null) {
    return (
      <SeparatorRow>
        <div className="split-diff-view-loading-row">
          <Icon icon="loading" />
          <span>{t('Loading...')}</span>
        </div>
      </SeparatorRow>
    );
  }
  if (result.error) {
    return (
      <SeparatorRow>
        <div className="split-diff-view-error-row">
          <ErrorNotice error={result.error} title={<T>Unable to fetch additional lines</T>} />
        </div>
      </SeparatorRow>
    );
  }

  const rows: React.ReactElement[] = [];
  addUnmodifiedRows(
    ctx.display === 'unified',
    result.value,
    path,
    'expanded',
    beforeLineStart,
    afterLineStart,
    rows,
    tokenization,
    tokenization,
    ctx.openFileToLine,
  );
  return <>{rows}</>;
}

function SeparatorRow({children}: {children: React.ReactNode}): React.ReactElement {
  return (
    <tr className="separator-row">
      <td colSpan={4} className="separator">
        {children}
      </td>
    </tr>
  );
}
