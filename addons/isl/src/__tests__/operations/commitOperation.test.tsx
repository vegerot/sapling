/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import App from '../../App';
import {CommitInfoTestUtils} from '../../testQueries';
import {
  resetTestMessages,
  expectMessageSentToServer,
  simulateCommits,
  COMMIT,
  simulateUncommittedChangedFiles,
  simulateMessageFromServer,
  openCommitInfoSidebar,
} from '../../testUtils';
import {CommandRunner} from '../../types';
import {fireEvent, render, screen, waitFor, within, act} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import * as utils from 'shared/utils';

describe('CommitOperation', () => {
  beforeEach(() => {
    resetTestMessages();
    render(<App />);
    act(() => {
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
          COMMIT('2', 'master', '00', {phase: 'public', remoteBookmarks: ['remote/master']}),
          COMMIT('1', 'Commit 1', '0', {phase: 'public'}),
          COMMIT('a', 'Commit A', '1'),
          COMMIT('b', 'Commit B', 'a', {isDot: true}),
        ],
      });
    });
  });

  const clickQuickCommit = async () => {
    const quickCommitButton = screen.getByTestId('quick-commit-button');
    act(() => {
      fireEvent.click(quickCommitButton);
    });
    await waitFor(() =>
      expectMessageSentToServer({
        type: 'runOperation',
        operation: expect.objectContaining({
          args: expect.arrayContaining(['commit']),
        }),
      }),
    );
  };

  const clickCheckboxForFile = (inside: HTMLElement, fileName: string) => {
    act(() => {
      const checkbox = within(within(inside).getByTestId(`changed-file-${fileName}`)).getByTestId(
        'file-selection-checkbox',
      );
      expect(checkbox).toBeInTheDocument();
      fireEvent.click(checkbox);
    });
  };

  it('runs commit', async () => {
    await clickQuickCommit();

    expectMessageSentToServer({
      type: 'runOperation',
      operation: {
        args: [
          'commit',
          '--addremove',
          '--message',
          expect.stringContaining(`Temporary Commit at`),
        ],
        id: expect.anything(),
        runner: CommandRunner.Sapling,
        trackEventName: 'CommitOperation',
      },
    });
  });

  it('runs commit with subset of files selected', async () => {
    const commitTree = screen.getByTestId('commit-tree-root');
    clickCheckboxForFile(commitTree, 'file2.txt');

    await clickQuickCommit();

    expectMessageSentToServer({
      type: 'runOperation',
      operation: {
        args: [
          'commit',
          '--addremove',
          '--message',
          expect.stringContaining(`Temporary Commit at`),
          {type: 'repo-relative-file', path: 'file1.txt'},
          {type: 'repo-relative-file', path: 'file3.txt'},
        ],
        id: expect.anything(),
        runner: CommandRunner.Sapling,
        trackEventName: 'CommitFileSubsetOperation',
      },
    });
  });

  it('changed files are shown in commit info view', async () => {
    const commitTree = screen.getByTestId('commit-tree-root');
    clickCheckboxForFile(commitTree, 'file2.txt');

    const quickInput = screen.getByTestId('quick-commit-title');

    act(() => {
      userEvent.type(quickInput, 'My Commit');
    });

    await clickQuickCommit();

    expect(
      within(screen.getByTestId('changes-to-amend')).queryByText(/file1.txt/),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId('changes-to-amend')).getByText(/file2.txt/),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId('changes-to-amend')).queryByText(/file3.txt/),
    ).not.toBeInTheDocument();

    expect(
      within(screen.getByTestId('committed-changes')).getByText(/file1.txt/),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId('committed-changes')).queryByText(/file2.txt/),
    ).not.toBeInTheDocument();
    expect(
      within(screen.getByTestId('committed-changes')).getByText(/file3.txt/),
    ).toBeInTheDocument();
  });

  it('uses commit template if provided', async () => {
    await waitFor(() => {
      expectMessageSentToServer({type: 'fetchCommitMessageTemplate'});
    });
    act(() => {
      simulateMessageFromServer({
        type: 'fetchedCommitMessageTemplate',
        template: 'Template Title\n\nSummary: my template',
      });
    });

    await clickQuickCommit();

    expectMessageSentToServer({
      type: 'runOperation',
      operation: {
        args: ['commit', '--addremove', '--message', expect.stringContaining('Template Title')],
        id: expect.anything(),
        runner: CommandRunner.Sapling,
        trackEventName: 'CommitOperation',
      },
    });
  });

  it('clears quick commit title after committing', async () => {
    const commitTree = screen.getByTestId('commit-tree-root');
    clickCheckboxForFile(commitTree, 'file2.txt'); // partial commit, so the quick input box isn't unmounted

    const quickInput = screen.getByTestId('quick-commit-title');
    act(() => {
      userEvent.type(quickInput, 'My Commit');
    });

    await clickQuickCommit();

    expect(quickInput).toHaveValue('');
  });

  it('on error, restores edited commit message to try again', async () => {
    act(() => openCommitInfoSidebar());
    act(() => CommitInfoTestUtils.clickCommitMode());

    act(() => {
      const title = CommitInfoTestUtils.getTitleEditor();
      userEvent.type(title, 'My Commit');
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      userEvent.type(desc, 'My description');
    });

    jest.spyOn(utils, 'randomId').mockImplementationOnce(() => '1111');
    await CommitInfoTestUtils.clickCommitButton();

    CommitInfoTestUtils.expectIsNOTEditingTitle();

    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'exit',
        exitCode: 1,
        id: '1111',
        timestamp: 0,
      });
    });

    await waitFor(() => {
      CommitInfoTestUtils.expectIsEditingTitle();
      const title = CommitInfoTestUtils.getTitleEditor();
      expect(title).toHaveValue('My Commit');
      CommitInfoTestUtils.expectIsEditingDescription();
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      expect(desc.value).toContain('My description');
    });
  });

  it('on error, merges messages when restoring edited commit message to try again', async () => {
    act(() => openCommitInfoSidebar());
    act(() => CommitInfoTestUtils.clickCommitMode());

    act(() => {
      const title = CommitInfoTestUtils.getTitleEditor();
      userEvent.type(title, 'My Commit');
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      userEvent.type(desc, 'My description');
    });

    jest.spyOn(utils, 'randomId').mockImplementationOnce(() => '2222');
    await CommitInfoTestUtils.clickCommitButton();
    CommitInfoTestUtils.expectIsNOTEditingTitle();

    act(() => {
      openCommitInfoSidebar();
      CommitInfoTestUtils.clickCommitMode();
    });
    act(() => {
      const title = CommitInfoTestUtils.getTitleEditor();
      userEvent.type(title, 'other title');
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      userEvent.type(desc, 'other description');
    });

    act(() => {
      simulateMessageFromServer({
        type: 'operationProgress',
        kind: 'exit',
        exitCode: 1,
        id: '2222',
        timestamp: 0,
      });
    });

    await waitFor(() => {
      CommitInfoTestUtils.expectIsEditingTitle();
      const title = CommitInfoTestUtils.getTitleEditor();
      expect(title).toHaveValue('other title, My Commit');
      CommitInfoTestUtils.expectIsEditingDescription();
      const desc = CommitInfoTestUtils.getDescriptionEditor();
      expect(desc.value).toContain('other description');
      expect(desc.value).toContain('My description');
    });
  });
});
