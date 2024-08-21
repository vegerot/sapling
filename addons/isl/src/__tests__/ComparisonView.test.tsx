/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {RenderResult} from '@testing-library/react';

import App from '../App';
import {cancelAllHighlightingTasks} from '../ComparisonView/SplitDiffView/syntaxHighlighting';
import {parsePatchAndFilter, sortFilesByType} from '../ComparisonView/utils';
import platform from '../platform';
import {
  COMMIT,
  expectMessageSentToServer,
  openCommitInfoSidebar,
  resetTestMessages,
  simulateCommits,
  simulateMessageFromServer,
  simulateUncommittedChangedFiles,
  waitForWithTick,
} from '../testUtils';
import {GeneratedStatus} from '../types';
import {act, screen, render, waitFor, fireEvent, cleanup, within} from '@testing-library/react';
import fs from 'node:fs';
import path from 'node:path';
import {ComparisonType} from 'shared/Comparison';
import {nextTick} from 'shared/testUtils';
import {nullthrows} from 'shared/utils';

afterEach(cleanup);

jest.mock('../platform');

const UNCOMMITTED_CHANGES_DIFF = `\
diff --git deletedFile.txt deletedFile.txt
deleted file mode 100644
--- deletedFile.txt
+++ /dev/null
@@ -1,1 +0,0 @@
-Goodbye
diff --git newFile.txt newFile.txt
new file mode 100644
--- /dev/null
+++ newFile.txt
@@ -0,0 +1,1 @@
+hello
diff --git someFile.txt someFile.txt
--- someFile.txt
+++ someFile.txt
@@ -7,5 +7,5 @@
 line 7
 line 8
-line 9
+line 9 - modified
 line 10
 line 11
diff --git -r a1b2c3d4e5f6 some/path/foo.go
--- some/path/foo.go
+++ some/path/foo.go
@@ -0,1 +0,1 @@
-println("hi")
+fmt.Println("hi")
`;

const DIFF_WITH_SYNTAX = `\
diff --git deletedFile.js deletedFile.js
deleted file mode 100644
--- deletedFile.js
+++ /dev/null
@@ -1,1 +0,0 @@
-console.log('goodbye');
diff --git newFile.js newFile.js
new file mode 100644
--- /dev/null
+++ newFile.js
@@ -0,0 +1,1 @@
+console.log('hello');
diff --git someFile.js someFile.js
--- someFile.js
+++ someFile.js
@@ -2,5 +2,5 @@
 function foo() {
   const variable_in_context_line = 0;
-  const variable_in_before = 1;
+  const variable_in_after = 1;
   console.log(variable_in_content_line);
 }
`;

/* eslint-disable @typescript-eslint/no-non-null-assertion */

describe('ComparisonView', () => {
  let app: RenderResult | null = null;
  beforeEach(() => {
    mockFetchToSupportSyntaxHighlighting();
    resetTestMessages();
    app = render(<App />);
    act(() => {
      openCommitInfoSidebar();
      simulateCommits({
        value: [
          COMMIT('1', 'some public base', '0', {phase: 'public'}),
          COMMIT('a', 'My Commit', '1'),
          COMMIT('b', 'Another Commit', 'a', {isDot: true}),
        ],
      });
      simulateUncommittedChangedFiles({
        value: [{path: 'src/file1.txt', status: 'M'}],
      });
    });
  });

  // Not afterEach because afterEach runs in a separate tick and can be too
  // late to avoid act(..) warnings or timeout.
  function unmountNow() {
    cancelAllHighlightingTasks();
    app?.unmount();
  }

  afterEach(() => {
    jest.clearAllMocks();
  });

  async function clickComparisonViewButton() {
    await act(async () => {
      const button = screen.getByTestId('open-comparison-view-button-UNCOMMITTED');
      fireEvent.click(button);
      await nextTick();
    });
  }
  async function openUncommittedChangesComparison(
    diffContent?: string,
    genereatedStatuses?: Record<string, GeneratedStatus>,
  ) {
    await clickComparisonViewButton();
    await waitFor(
      () =>
        expectMessageSentToServer({
          type: 'requestComparison',
          comparison: {type: ComparisonType.UncommittedChanges},
        }),
      // Since this dynamically imports the comparison view, it may take a while to load in resource-constrained CI,
      // so add a generous timeout to reducy flakiness.
      {timeout: 10_000},
    );
    act(() => {
      simulateMessageFromServer({
        type: 'fetchedGeneratedStatuses',
        results: genereatedStatuses ?? {},
      });
    });
    await act(async () => {
      simulateMessageFromServer({
        type: 'comparison',
        comparison: {type: ComparisonType.UncommittedChanges},
        data: {diff: {value: diffContent ?? UNCOMMITTED_CHANGES_DIFF}},
      });
      await nextTick();
    });
  }
  function inComparisonView() {
    return within(screen.getByTestId('comparison-view'));
  }

  function closeComparisonView() {
    const closeButton = inComparisonView().getByTestId('close-comparison-view-button');
    expect(closeButton).toBeInTheDocument();
    act(() => {
      fireEvent.click(closeButton);
    });
  }

  it('Loads comparison', async () => {
    await openUncommittedChangesComparison();
    // Prevent act(..) warnings. This cannot be afterEach() which is too late.
    unmountNow();
  });

  it('parses files from comparison', async () => {
    await openUncommittedChangesComparison();
    expect(inComparisonView().getByText('someFile.txt')).toBeInTheDocument();
    expect(inComparisonView().getByText('newFile.txt')).toBeInTheDocument();
    expect(inComparisonView().getByText('deletedFile.txt')).toBeInTheDocument();
    unmountNow();
  });

  it('show file contents', async () => {
    await openUncommittedChangesComparison();
    expect(inComparisonView().getByText('- modified')).toBeInTheDocument();
    expect(inComparisonView().getAllByText('line 7')[0]).toBeInTheDocument();
    expect(inComparisonView().getAllByText('line 8')[0]).toBeInTheDocument();
    expect(inComparisonView().getAllByText('line 9')[0]).toBeInTheDocument();
    expect(inComparisonView().getAllByText('line 10')[0]).toBeInTheDocument();
    expect(inComparisonView().getAllByText('line 11')[0]).toBeInTheDocument();
    unmountNow();
  });

  it('loads remaining lines', async () => {
    await openUncommittedChangesComparison();
    const expandButton = inComparisonView().getByText('Expand 6 lines');
    expect(expandButton).toBeInTheDocument();
    act(() => {
      fireEvent.click(expandButton);
    });
    await waitFor(() => {
      expectMessageSentToServer({
        type: 'requestComparisonContextLines',
        id: {path: 'someFile.txt', comparison: {type: ComparisonType.UncommittedChanges}},
        numLines: 6,
        start: 1,
      });
    });
    act(() => {
      simulateMessageFromServer({
        type: 'comparisonContextLines',
        lines: {value: ['line 1', 'line 2', 'line 3', 'line 4', 'line 5', 'line 6']},
        path: 'someFile.txt',
      });
    });
    await waitFor(() => {
      expect(inComparisonView().getAllByText('line 1')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 2')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 3')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 4')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 5')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 6')[0]).toBeInTheDocument();
    });
    unmountNow();
  });

  it('can close comparison', async () => {
    await openUncommittedChangesComparison();
    expect(inComparisonView().getByText('- modified')).toBeInTheDocument();
    closeComparisonView();
    expect(screen.queryByText('- modified')).not.toBeInTheDocument();
    unmountNow();
  });

  it('invalidates cached remaining lines when the head commit changes', async () => {
    await openUncommittedChangesComparison();
    const clickExpand = () => {
      const expandButton = inComparisonView().getByText('Expand 6 lines');
      expect(expandButton).toBeInTheDocument();
      act(() => {
        fireEvent.click(expandButton);
      });
    };
    clickExpand();
    await waitFor(() => {
      expectMessageSentToServer({
        type: 'requestComparisonContextLines',
        id: {path: 'someFile.txt', comparison: {type: ComparisonType.UncommittedChanges}},
        numLines: 6,
        start: 1,
      });
    });
    act(() => {
      simulateMessageFromServer({
        type: 'comparisonContextLines',
        lines: {value: ['line 1', 'line 2', 'line 3', 'line 4', 'line 5', 'line 6']},
        path: 'someFile.txt',
      });
    });
    await waitForWithTick(() => {
      expect(inComparisonView().getAllByText('line 1')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('line 6')[0]).toBeInTheDocument();
    });

    closeComparisonView();
    resetTestMessages(); // make sure we don't find previous "requestComparisonContextLines" in later assertions

    // head commit changes

    act(() => {
      simulateCommits({
        value: [
          COMMIT('1', 'some public base', '0', {phase: 'public'}),
          COMMIT('a', 'My Commit', '1'),
          COMMIT('b', 'Another Commit', 'a'),
          COMMIT('c', 'New commit!', 'b', {isDot: true}),
        ],
      });
    });
    await openUncommittedChangesComparison();
    expect(inComparisonView().getByText('- modified')).toBeInTheDocument();

    clickExpand();

    // previous context lines are no longer there
    expect(inComparisonView().queryByText('line 1')).not.toBeInTheDocument();

    // it should ask for the line contents from the server again
    await waitFor(() => {
      expectMessageSentToServer({
        type: 'requestComparisonContextLines',
        id: {path: 'someFile.txt', comparison: {type: ComparisonType.UncommittedChanges}},
        numLines: 6,
        start: 1,
      });
    });
    act(() => {
      simulateMessageFromServer({
        type: 'comparisonContextLines',
        lines: {
          value: [
            'different line 1',
            'different line 2',
            'different line 3',
            'different line 4',
            'different line 5',
            'different line 6',
          ],
        },
        path: 'someFile.txt',
      });
    });
    // new data is used
    await waitForWithTick(() => {
      expect(inComparisonView().getAllByText('different line 1')[0]).toBeInTheDocument();
      expect(inComparisonView().getAllByText('different line 6')[0]).toBeInTheDocument();
    });
    unmountNow();
  });

  it('refresh button requests new data', async () => {
    await openUncommittedChangesComparison();
    resetTestMessages();

    act(() => {
      fireEvent.click(inComparisonView().getByTestId('comparison-refresh-button'));
    });

    expectMessageSentToServer({
      type: 'requestComparison',
      comparison: {type: ComparisonType.UncommittedChanges},
    });
    unmountNow();
  });

  it('changing comparison mode requests new data', async () => {
    await openUncommittedChangesComparison();

    act(() => {
      fireEvent.change(inComparisonView().getByTestId('comparison-view-picker'), {
        target: {value: ComparisonType.StackChanges},
      });
    });
    expectMessageSentToServer({
      type: 'requestComparison',
      comparison: {type: ComparisonType.StackChanges},
    });
    unmountNow();
  });

  it('shows a spinner while a fetch is ongoing', async () => {
    await clickComparisonViewButton();
    expect(inComparisonView().getByTestId('comparison-loading')).toBeInTheDocument();

    await act(async () => {
      simulateMessageFromServer({
        type: 'comparison',
        comparison: {type: ComparisonType.UncommittedChanges},
        data: {diff: {value: UNCOMMITTED_CHANGES_DIFF}},
      });
      await nextTick();
    });
    expect(inComparisonView().queryByTestId('comparison-loading')).not.toBeInTheDocument();
    unmountNow();
  });

  it('copies file path on click', async () => {
    await openUncommittedChangesComparison();

    // Click on the "foo.go" of "some/path/foo.go".
    act(() => {
      fireEvent.click(inComparisonView().getByText('foo.go'));
    });
    expect(platform.clipboardCopy).toHaveBeenCalledTimes(1);
    expect(platform.clipboardCopy).toHaveBeenCalledWith('foo.go');

    // Click on the "some/" of "some/path/foo.go".
    act(() => {
      fireEvent.click(inComparisonView().getByText('some/'));
    });
    expect(platform.clipboardCopy).toHaveBeenCalledTimes(2);
    expect(platform.clipboardCopy).toHaveBeenLastCalledWith('some/path/foo.go');
    unmountNow();
  });

  describe('syntax highlighting', () => {
    it('renders syntax highlighting', async () => {
      await openUncommittedChangesComparison(DIFF_WITH_SYNTAX);
      await waitForSyntaxHighlightingToAppear(screen.getByTestId('comparison-view'));

      // console from console.log is highlighted as its own token
      const tokens = within(screen.getByTestId('comparison-view')).queryAllByText('console');
      expect(tokens.length).toBeGreaterThan(0);
      // highlighted tokens have classes like mtk1, mtk2, etc.
      expect(tokens.some(token => /mtk\d+/.test(token.className))).toBe(true);
      unmountNow();
    });

    it('renders highlighting in context lines and diff lines', async () => {
      await openUncommittedChangesComparison(DIFF_WITH_SYNTAX);
      await waitForSyntaxHighlightingToAppear(screen.getByTestId('comparison-view'));

      await waitFor(() => {
        expect(
          within(screen.getByTestId('comparison-view')).queryAllByText('variable_in_context_line'),
        ).toHaveLength(2);
        expect(
          within(screen.getByTestId('comparison-view')).getByText('variable_in_before'),
        ).toBeInTheDocument();
        expect(
          within(screen.getByTestId('comparison-view')).getByText('variable_in_after'),
        ).toBeInTheDocument();
      });
      unmountNow();
    });

    it('highlights expanded context lines', async () => {
      await openUncommittedChangesComparison(DIFF_WITH_SYNTAX);
      await waitForSyntaxHighlightingToAppear(screen.getByTestId('comparison-view'));

      const expandButton = inComparisonView().getByText('Expand 1 line');
      expect(expandButton).toBeInTheDocument();
      act(() => {
        fireEvent.click(expandButton);
      });
      await waitFor(() => {
        expectMessageSentToServer({
          type: 'requestComparisonContextLines',
          id: {path: 'someFile.js', comparison: {type: ComparisonType.UncommittedChanges}},
          numLines: 1,
          start: 1,
        });
      });
      act(() => {
        simulateMessageFromServer({
          type: 'comparisonContextLines',
          lines: {value: ['const loaded_additional_context_variable = 5;']},
          path: 'someFile.js',
        });
      });
      await waitFor(() => {
        // highlighted token appears by itself
        expect(
          inComparisonView().queryAllByText('loaded_additional_context_variable'),
        ).toHaveLength(2);
      });
      unmountNow();
    });
  });

  const makeFileDiff = (name: string, content: string) => {
    return `diff --git file${name}.txt file${name}.txt
--- file${name}.txt
+++ file${name}.txt
@@ -1,2 +1,2 @@
${content}
`;
  };

  describe('collapsing files', () => {
    it('can click to collapse files', async () => {
      const SINGLE_CHANGE = makeFileDiff('1', '+const x = 1;');
      await openUncommittedChangesComparison(SINGLE_CHANGE);

      const collapseButton = screen.getByTestId('split-diff-view-file-header-collapse-button');
      expect(inComparisonView().getByText('const x = 1;')).toBeInTheDocument();
      expect(inComparisonView().getByText('file1.txt')).toBeInTheDocument();
      fireEvent.click(collapseButton);
      expect(inComparisonView().queryByText('const x = 1;')).not.toBeInTheDocument();
      expect(inComparisonView().getByText('file1.txt')).toBeInTheDocument();
    });

    it('first files are expanded, later files are collapsed', async () => {
      // 10 files, 1000 added lines each
      const HUGE_DIFF = [
        ...new Array(10)
          .fill(undefined)
          .map((_, index) =>
            makeFileDiff(String(index), new Array(1001).fill("+console.log('hi');").join('\n')),
          ),
      ].join('\n');
      await openUncommittedChangesComparison(HUGE_DIFF);

      const collapsedStates = inComparisonView().queryAllByTestId(
        /split-diff-view-file-header-(collapse|expand)-button/,
      );
      const collapsedValues = collapsedStates.map(node => node.dataset.testid);
      expect(collapsedValues).toEqual([
        'split-diff-view-file-header-collapse-button',
        'split-diff-view-file-header-collapse-button',
        'split-diff-view-file-header-collapse-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
        'split-diff-view-file-header-expand-button',
      ]);
      unmountNow();
    }, 20_000 /* potentially slow test */);

    it('a single large file is expanded so you always see something', async () => {
      const GIANT_FILE = makeFileDiff(
        'bigChange.txt',
        new Array(5000).fill('+big_file_contents').join('\n'),
      );
      const SMALL_FILE = makeFileDiff('smallChange.txt', '+small_file_contents');
      const GIANT_AND_SMALL = [GIANT_FILE, SMALL_FILE].join('\n');
      await openUncommittedChangesComparison(GIANT_AND_SMALL);

      // the large file starts expanded
      expect(inComparisonView().getAllByText('big_file_contents').length).toBeGreaterThan(0);
      // the small file starts collapsed
      expect(inComparisonView().queryByText('small_file_contents')).not.toBeInTheDocument();
      unmountNow();
    });
  });

  describe('genereated files', () => {
    it('genereated status is fetched for files being compared', async () => {
      const NORMAL_FILE = makeFileDiff('normal1', '+normal_contents');
      const PARTIAL_FILE = makeFileDiff('partial1', '+partial_contents');
      const GENERATED_FILE = makeFileDiff('generated1', '+generated_contents');
      const ALL = [GENERATED_FILE, PARTIAL_FILE, NORMAL_FILE].join('\n');

      await openUncommittedChangesComparison(ALL);
      await waitFor(() => {
        expectMessageSentToServer({
          type: 'fetchGeneratedStatuses',
          paths: ['filegenerated1.txt', 'filepartial1.txt', 'filenormal1.txt'],
        });
      });
      unmountNow();
    });

    it('banner says that files are generated', async () => {
      const NORMAL_FILE = makeFileDiff('normal2', '+normal_contents');
      const PARTIAL_FILE = makeFileDiff('partial2', '+partial_contents');
      const GENERATED_FILE = makeFileDiff('generated2', '+generated_contents');
      const ALL = [GENERATED_FILE, PARTIAL_FILE, NORMAL_FILE].join('\n');

      await openUncommittedChangesComparison(ALL, {
        'filenormal2.txt': GeneratedStatus.Manual,
        'filegenerated2.txt': GeneratedStatus.Generated,
        'filepartial2.txt': GeneratedStatus.PartiallyGenerated,
      });
      expect(inComparisonView().getByText('This file is generated')).toBeInTheDocument();
      expect(inComparisonView().getByText('This file is partially generated')).toBeInTheDocument();
      unmountNow();
    });

    it('generated files are collapsed by default', async () => {
      const NORMAL_FILE = makeFileDiff('normal3', '+normal_contents');
      const PARTIAL_FILE = makeFileDiff('partial3', '+partial_contents');
      const GENERATED_FILE = makeFileDiff('generated3', '+generated_contents');
      const ALL = [GENERATED_FILE, PARTIAL_FILE, NORMAL_FILE].join('\n');

      await openUncommittedChangesComparison(ALL, {
        'filenormal3.txt': GeneratedStatus.Manual,
        'filegenerated3.txt': GeneratedStatus.Generated,
        'filepartial3.txt': GeneratedStatus.PartiallyGenerated,
      });

      // normal, partial start expanded
      expect(inComparisonView().getByText('normal_contents')).toBeInTheDocument();
      expect(inComparisonView().getByText('partial_contents')).toBeInTheDocument();
      await waitFor(() => {
        // genereated starts collapsed
        expect(inComparisonView().queryByText('generated_contents')).not.toBeInTheDocument();
      });

      expect(inComparisonView().getByText('Show anyway')).toBeInTheDocument();
      fireEvent.click(inComparisonView().getByText('Show anyway'));
      await waitFor(() => {
        // genereated now expands
        expect(inComparisonView().getByText('generated_contents')).toBeInTheDocument();
      });
      unmountNow();
    });
  });
});

function waitForSyntaxHighlightingToAppear(inside: HTMLElement): Promise<void> {
  return waitFor(() => {
    const tokens = inside.querySelectorAll('.mtk1');
    expect(tokens.length).toBeGreaterThan(0);
  });
}

function mockFetchToSupportSyntaxHighlighting(): jest.SpyInstance {
  return jest.spyOn(global, 'fetch').mockImplementation(
    jest.fn(async (url: URL) => {
      if (url.pathname.includes('generated/textmate')) {
        const match = /.*generated\/textmate\/(.*)$/.exec(url.pathname);
        const filename = nullthrows(match)[1];
        const toPublicDir = (filename: string) =>
          path.normalize(path.join(__dirname, '../../public/generated/textmate', filename));
        if (filename === 'onig.wasm') {
          const file = await fs.promises.readFile(toPublicDir(filename));
          return {
            headers: new Map(),
            arrayBuffer: () => file.buffer,
          };
        } else {
          const file = await fs.promises.readFile(toPublicDir(filename), 'utf-8');
          return {text: () => file};
        }
      }
      throw new Error(`${url} not found`);
    }) as jest.Mock,
  );
}

describe('ComparisonView utils', () => {
  describe('sortFilesByType', () => {
    it('sorts by type', () => {
      const files = parsePatchAndFilter(UNCOMMITTED_CHANGES_DIFF);

      expect(files).toEqual([
        expect.objectContaining({newFileName: 'deletedFile.txt', type: 'Removed'}),
        expect.objectContaining({newFileName: 'newFile.txt', type: 'Added'}),
        expect.objectContaining({newFileName: 'someFile.txt', type: 'Modified'}),
        expect.objectContaining({newFileName: 'some/path/foo.go', type: 'Modified'}),
      ]);

      sortFilesByType(files);

      expect(files).toEqual([
        expect.objectContaining({newFileName: 'some/path/foo.go', type: 'Modified'}),
        expect.objectContaining({newFileName: 'someFile.txt', type: 'Modified'}),
        expect.objectContaining({newFileName: 'newFile.txt', type: 'Added'}),
        expect.objectContaining({newFileName: 'deletedFile.txt', type: 'Removed'}),
      ]);
    });
  });
});
