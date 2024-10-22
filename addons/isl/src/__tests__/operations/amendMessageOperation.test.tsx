/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import App from '../../App';
import {CommitInfoTestUtils, CommitTreeListTestUtils} from '../../testQueries';
import {
  resetTestMessages,
  expectMessageSentToServer,
  simulateCommits,
  COMMIT,
  simulateUncommittedChangedFiles,
  simulateMessageFromServer,
  openCommitInfoSidebar,
} from '../../testUtils';
import {render, waitFor, act} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as utils from 'shared/utils';

describe('AmendMessageOperation', () => {
  beforeEach(() => {
    resetTestMessages();
    render(<App />);
    act(() => {
      simulateMessageFromServer({
        type: 'repoInfo',
        info: {
          type: 'success',
          command: 'sl',
          repoRoot: '/path/to/testrepo',
          dotdir: '/path/to/testrepo/.sl',
          codeReviewSystem: {type: 'unknown'},
          pullRequestDomain: undefined,
          preferredSubmitCommand: undefined,
        },
      });
      expectMessageSentToServer({
        type: 'subscribe',
        kind: 'smartlogCommits',
        subscriptionID: expect.anything(),
      });
      simulateUncommittedChangedFiles({
        value: [
          {path: 'file1.txt', status: 'M'},
          {path: 'file2.txt', status: 'A'},
          {path: 'file3.txt', status: 'R'},
        ],
      });
      simulateCommits({
        value: [
          COMMIT('111111111111', 'Commit 1', '0', {phase: 'public'}),
          COMMIT('aaaaaaaaaaaa', 'Commit A', '1'),
          COMMIT('bbbbbbbbbbbb', 'Commit B', 'a', {isDot: true}),
          COMMIT('cccccccccccc', 'Commit C', 'b'),
        ],
      });
    });
  });

  it('on error, restores edited commit message to try again', () => {
    act(() => CommitInfoTestUtils.clickToSelectCommit('aaaaaaaaaaaa'));
    act(() => openCommitInfoSidebar());
    act(() => {
      CommitInfoTestUtils.clickToEditTitle();
      CommitInfoTestUtils.clickToEditDescription();
    });
    act(() => {
      const title = CommitInfoTestUtils.getTitleEditor();
      userEvent.type(title, 'My Commit');
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      userEvent.type(desc, 'My description');
    });

    jest.spyOn(utils, 'randomId').mockImplementationOnce(() => '1111');
    act(() => {
      CommitInfoTestUtils.clickAmendMessageButton();
    });

    CommitInfoTestUtils.expectIsNOTEditingTitle();

    act(() => CommitInfoTestUtils.clickToSelectCommit('bbbbbbbbbbbb'));
    expect(CommitInfoTestUtils.withinCommitInfo().getByText('You are here')).toBeInTheDocument();

    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'exit',
        exitCode: 1,
        id: '1111',
        timestamp: 0,
      });
    });

    waitFor(() => {
      expect(
        CommitInfoTestUtils.withinCommitInfo().getByText('You are here'),
      ).not.toBeInTheDocument();
      CommitInfoTestUtils.expectIsEditingTitle();
      const title = CommitInfoTestUtils.getTitleEditor();
      expect(title).toHaveValue('My Commit');
      CommitInfoTestUtils.expectIsEditingDescription();
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      expect(desc).toHaveValue('My description');
    });
  });

  it('if a previous command errors and metaedit is queued, the message is recovered', async () => {
    // run a goto
    jest.spyOn(utils, 'randomId').mockImplementationOnce(() => '3333');
    await CommitTreeListTestUtils.clickGoto('cccccccccccc');
    expectMessageSentToServer({
      type: 'runOperation',
      operation: expect.objectContaining({
        args: expect.arrayContaining(['goto']),
      }),
    });

    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'spawn',
        id: '3333',
        queue: [],
      });
    });

    // then queue a metaedit
    act(() => CommitInfoTestUtils.clickToSelectCommit('aaaaaaaaaaaa'));
    act(() => openCommitInfoSidebar());
    act(() => {
      CommitInfoTestUtils.clickToEditTitle();
      CommitInfoTestUtils.clickToEditDescription();
    });
    act(() => {
      const title = CommitInfoTestUtils.getTitleEditor();
      userEvent.type(title, 'My Commit');
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      userEvent.type(desc, 'My description');
    });

    jest.spyOn(utils, 'randomId').mockImplementationOnce(() => '4444');
    act(() => {
      CommitInfoTestUtils.clickAmendMessageButton();
    });
    await waitFor(() => {
      expectMessageSentToServer({
        type: 'runOperation',
        operation: expect.objectContaining({
          args: expect.arrayContaining(['metaedit']),
        }),
      });
    });

    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'queue',
        id: '4444',
        queue: ['4444'],
      });
    });

    CommitInfoTestUtils.expectIsNOTEditingTitle();

    act(() => CommitInfoTestUtils.clickToSelectCommit('bbbbbbbbbbbb'));

    // the goto fails
    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'exit',
        exitCode: 1,
        id: '3333',
        timestamp: 0,
      });
    });

    // we recover the message
    await waitFor(() => {
      CommitInfoTestUtils.expectIsEditingTitle();
      const title = CommitInfoTestUtils.getTitleEditor();
      expect(title).toHaveValue('Commit AMy Commit');
      CommitInfoTestUtils.expectIsEditingDescription();
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      expect(desc.value).toContain('My description');
    });
  });
});
