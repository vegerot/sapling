/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {EnabledSCMApiFeature} from '../types';
import type {Repository} from 'isl-server/src/Repository';
import type {Logger} from 'isl-server/src/logger';
import type {ServerPlatform} from 'isl-server/src/serverPlatform';
import type {RepositoryContext} from 'isl-server/src/serverTypes';
import type {RepoInfo, ValidatedRepoInfo} from 'isl/src/types';

import {VSCodeReposList} from '../VSCodeRepo';
import {repositoryCache} from 'isl-server/src/RepositoryCache';
import {makeServerSideTracker} from 'isl-server/src/analytics/serverSideTracker';
import {TypedEventEmitter} from 'shared/TypedEventEmitter';
import {nextTick} from 'shared/testUtils';
import * as vscode from 'vscode';

const mockLogger: Logger = {info: jest.fn(), warn: jest.fn(), log: jest.fn(), error: jest.fn()};

const mockTracker = makeServerSideTracker(
  mockLogger,
  {platformName: 'test'} as ServerPlatform,
  '0.1',
  jest.fn(),
);

jest.mock('isl-server/src/Repository', () => {
  class MockRepository implements Partial<Repository> {
    static getRepoInfo = jest.fn((ctx: RepositoryContext): Promise<RepoInfo> => {
      let root: string;
      // resolve cwd into shared mock locations
      if (ctx.cwd.includes('/path/to/repo1')) {
        root = '/path/to/repo1';
      } else if (ctx.cwd.includes('/path/to/repo2')) {
        root = '/path/to/repo2';
      } else {
        return Promise.resolve({type: 'cwdNotARepository', cwd: ctx.cwd});
      }
      return Promise.resolve({
        type: 'success',
        repoRoot: root,
        dotdir: root + '/.sl',
        command: 'sl',
        preferredSubmitCommand: 'pr',
        codeReviewSystem: {type: 'unknown', path: ''},
        pullRequestDomain: undefined,
      });
    });
    constructor(public info: ValidatedRepoInfo, public logger?: Logger) {}

    public disposables: Array<() => void> = [];
    public dispose() {
      this.disposables.forEach(d => d());
    }
    public onDidDispose = (cb: () => void) => this.disposables.push(cb);
    public subscribeToUncommittedChanges = jest.fn();
    public onChangeConflictState = jest.fn();
    public getUncommittedChanges = jest.fn();
    public getMergeConflicts = jest.fn();
  }
  return {
    Repository: MockRepository as unknown as Repository,
  };
});

describe('adding and removing repositories', () => {
  let foldersEmitter: TypedEventEmitter<'value', vscode.WorkspaceFoldersChangeEvent>;
  beforeEach(() => {
    foldersEmitter = new TypedEventEmitter();
    (vscode.workspace.onDidChangeWorkspaceFolders as jest.Mock).mockImplementation(cb => {
      foldersEmitter.on('value', cb);
      return {dispose: () => foldersEmitter.off('value', cb)};
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
    repositoryCache.clearCache();
    foldersEmitter.removeAllListeners();
  });

  const ENABLED = new Set<EnabledSCMApiFeature>(['blame', 'sidebar']);

  it('creates repositories for workspace folders', async () => {
    const repos = new VSCodeReposList(mockLogger, mockTracker, ENABLED);
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1')}],
      removed: [],
    });
    await nextTick();

    expect(vscode.scm.createSourceControl).toHaveBeenCalledTimes(1);
    repos.dispose();
  });

  it('deduplicates among shared repos', async () => {
    const repos = new VSCodeReposList(mockLogger, mockTracker, ENABLED);
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1/foo')}],
      removed: [],
    });
    await nextTick();
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 1, uri: vscode.Uri.file('/path/to/repo1/bar')}],
      removed: [],
    });
    await nextTick();

    expect(vscode.scm.createSourceControl).toHaveBeenCalledTimes(1);

    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 1, uri: vscode.Uri.file('/path/to/repo2/foobar')}],
      removed: [],
    });
    await nextTick();
    expect(vscode.scm.createSourceControl).toHaveBeenCalledTimes(2);

    repos.dispose();
  });

  it('deletes repositories for workspace folders', async () => {
    const repos = new VSCodeReposList(mockLogger, mockTracker, ENABLED);

    // add repo twice, only creates 1 repo
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1/foo')}],
      removed: [],
    });
    await nextTick();
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1/bar')}],
      removed: [],
    });
    await nextTick();
    expect(vscode.scm.createSourceControl).toHaveBeenCalledTimes(1);

    // remove that repo
    foldersEmitter.emit('value', {
      added: [],
      removed: [{name: 'my folder', index: 1, uri: vscode.Uri.file('/path/to/repo1/foo')}],
    });
    await nextTick();
    foldersEmitter.emit('value', {
      added: [],
      removed: [{name: 'my folder', index: 1, uri: vscode.Uri.file('/path/to/repo1/bar')}],
    });
    await nextTick();

    // adding the same repo again must create it again
    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1/foo')}],
      removed: [],
    });
    await nextTick();
    expect(vscode.scm.createSourceControl).toHaveBeenCalledTimes(2);

    repos.dispose();
  });

  it('looks up repos by prefix', async () => {
    const repos = new VSCodeReposList(mockLogger, mockTracker, ENABLED);

    foldersEmitter.emit('value', {
      added: [{name: 'my folder', index: 0, uri: vscode.Uri.file('/path/to/repo1/foo')}],
      removed: [],
    });
    await nextTick();

    expect(repos.repoForPath('/path/to/repo1/foo')).not.toBeUndefined();
    expect(repos.repoForPath('/path/to/repo1/foo/myFile.txt')).not.toBeUndefined();
    expect(repos.repoForPath('/path/to/repo2/foo')).toBeUndefined();

    foldersEmitter.emit('value', {
      added: [],
      removed: [{name: 'my folder', index: 1, uri: vscode.Uri.file('/path/to/repo1/foo')}],
    });
    await nextTick();

    expect(repos.repoForPath('/path/to/repo1/foo')).toBeUndefined();
    expect(repos.repoForPath('/path/to/repo1/foo/myFile.txt')).toBeUndefined();

    repos.dispose();
  });
});
