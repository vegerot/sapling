/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import App from '../../App';
import {
  resetTestMessages,
  expectMessageSentToServer,
  simulateCommits,
  closeCommitInfoSidebar,
  TEST_COMMIT_HISTORY,
} from '../../testUtils';
import {CommandRunner, SucceedableRevset} from '../../types';
import {fireEvent, render, screen, within} from '@testing-library/react';
import {act} from 'react-dom/test-utils';

/*eslint-disable @typescript-eslint/no-non-null-assertion */

jest.mock('../../MessageBus');

describe('hide operation', () => {
  beforeEach(() => {
    resetTestMessages();
    render(<App />);
    act(() => {
      closeCommitInfoSidebar();
      expectMessageSentToServer({
        type: 'subscribe',
        kind: 'smartlogCommits',
        subscriptionID: expect.anything(),
      });
      simulateCommits({
        value: TEST_COMMIT_HISTORY,
      });
    });
  });

  function rightClickAndChooseFromContextMenu(element: Element, choiceMatcher: string) {
    act(() => {
      fireEvent.contextMenu(element);
    });
    const choice = within(screen.getByTestId('context-menu-container')).getByText(choiceMatcher);
    expect(choice).not.toEqual(null);
    act(() => {
      fireEvent.click(choice);
    });
  }

  it('previews hiding a stack of commits', () => {
    rightClickAndChooseFromContextMenu(screen.getByText('Commit B'), 'Hide Commit and Descendents');

    expect(document.querySelectorAll('.commit-preview-hidden-root')).toHaveLength(1);
    expect(document.querySelectorAll('.commit-preview-hidden-descendant')).toHaveLength(3);
  });

  it('runs hide operation', () => {
    rightClickAndChooseFromContextMenu(screen.getByText('Commit B'), 'Hide Commit and Descendents');

    const runHideButton = screen.getByText('Hide');
    expect(runHideButton).toBeInTheDocument();
    fireEvent.click(runHideButton);

    expectMessageSentToServer({
      type: 'runOperation',
      operation: {
        args: ['hide', '--rev', SucceedableRevset('b')],
        id: expect.anything(),
        runner: CommandRunner.Sapling,
        trackEventName: 'HideOperation',
      },
    });
  });

  it('shows optimistic preview of hide', () => {
    rightClickAndChooseFromContextMenu(screen.getByText('Commit B'), 'Hide Commit and Descendents');

    const runHideButton = screen.getByText('Hide');
    fireEvent.click(runHideButton);

    // original commit is hidden
    expect(screen.queryByTestId('commit-b')).not.toBeInTheDocument();
    // same for descendants
    expect(screen.queryByTestId('commit-c')).not.toBeInTheDocument();
    expect(screen.queryByTestId('commit-d')).not.toBeInTheDocument();
    expect(screen.queryByTestId('commit-e')).not.toBeInTheDocument();
  });
});
