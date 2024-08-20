/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {MessageBusStatus} from '../src/MessageBus';
import type {Disposable, RepoRelativePath} from '../src/types';
import type {Options as ExecaOptions} from 'execa';
import type {Logger} from 'isl-server/src/logger';
import type {ServerPlatform} from 'isl-server/src/serverPlatform';
import type {RepositoryContext} from 'isl-server/src/serverTypes';
import type {TypedEventEmitter} from 'shared/TypedEventEmitter';

import {onClientConnection} from '../../isl-server/src/index';
import mockedClientMessagebus from '../src/MessageBus';
import {fireEvent, render, screen} from '@testing-library/react';
import {makeServerSideTracker} from 'isl-server/src/analytics/serverSideTracker';
import {runCommand} from 'isl-server/src/commands';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const IS_CI = !!process.env.SANDCASTLE || !!process.env.GITHUB_ACTIONS;

const mockTracker = makeServerSideTracker(
  console,
  {platformName: 'test'} as ServerPlatform,
  '0.1',
  jest.fn(),
);

// fake client message bus that connects to server in the same process
jest.mock('../src/MessageBus', () => {
  const {TypedEventEmitter} =
    // this mock implementation is hoisted above all other imports, so we can't use imports "normally"
    // eslint-disable-next-line @typescript-eslint/no-var-requires,@typescript-eslint/consistent-type-imports
    require('shared/TypedEventEmitter') as typeof import('shared/TypedEventEmitter');

  const {log} = console;

  class IntegrationTestMessageBus {
    disposables: Array<() => void> = [];
    onMessage(handler: (event: MessageEvent<string>) => void | Promise<void>): Disposable {
      const cb = (message: string) => {
        log('[c <- s]', message);
        handler({data: message} as MessageEvent<string>);
      };

      this.serverToClient.on('data', cb);
      return {
        dispose: () => {
          this.serverToClient.off('data', cb);
        },
      };
    }

    postMessage(message: string | ArrayBuffer) {
      log('[c -> s]', message);
      this.clientToServer.emit('data', message as string);
    }

    public statusChangeHandlers = new Set<(status: MessageBusStatus) => unknown>();
    onChangeStatus(handler: (status: MessageBusStatus) => unknown): Disposable {
      // pretend connection opens immediately
      handler({type: 'open'});
      this.statusChangeHandlers.add(handler);

      return {
        dispose: () => {
          this.statusChangeHandlers.delete(handler);
        },
      };
    }

    /**** extra methods for testing ****/

    clientToServer = new TypedEventEmitter<'data', string | ArrayBuffer>();
    serverToClient = new TypedEventEmitter<'data', string>();

    dispose = () => {
      this.clientToServer.removeAllListeners();
      this.serverToClient.removeAllListeners();
    };
  }

  return new IntegrationTestMessageBus();
});

type MockedClientMessageBus = {
  clientToServer: TypedEventEmitter<'data', string | ArrayBuffer>;
  serverToClient: TypedEventEmitter<'data', string>;
  dispose(): void;
};

beforeAll(() => {
  global.ResizeObserver = class ResizeObserver {
    observe() {
      /* noop */
    }
    unobserve() {
      /* noop */
    }
    disconnect() {
      /* noop */
    }
  };
});

const wrapLogger = (logger: Logger, label: string): Logger => ({
  log: (...args: Array<unknown>) => {
    logger.log(label, ...args);
  },
  info: (...args: Array<unknown>) => {
    logger.info(label, ...args);
  },
  warn: (...args: Array<unknown>) => {
    logger.info(label, ...args);
  },
  error: (...args: Array<unknown>) => {
    logger.info(label, ...args);
  },
});

/**
 * Creates an sl repository in a temp dir on disk,
 * creates a single initial commit,
 * then performs an initial render, running both server and client in the same process.
 */
export async function initRepo() {
  const repoDir = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'isl-integration-test-repo-'));
  const testLogger = wrapLogger(console, '[ test ]');

  let cmd = 'sl';
  if (process.env.SANDCASTLE) {
    // On internal CI, it's easiest to run 'hg' instead of 'sl'.
    cmd = 'hg';
    process.env.PATH += ':/bin/hg';
  }

  testLogger.info('sl cmd: ', cmd);

  testLogger.log('temp repo: ', repoDir);
  process.chdir(repoDir);

  const ctx: RepositoryContext = {
    cmd,
    cwd: repoDir,
    logger: testLogger,
    tracker: mockTracker,
  };

  async function sl(args: Array<string>, options?: ExecaOptions) {
    testLogger.log(ctx.cmd, ...args);
    const result = await runCommand(ctx, args, {
      ...options,
      env: {
        ...process.env,
        ...(options?.env ?? {}),
        FB_SCM_DIAGS_NO_SCUBA: '1',
      } as Record<string, string> as NodeJS.ProcessEnv,
      extendEnv: true,
    });
    return result;
  }

  async function writeFileInRepo(filePath: RepoRelativePath, content: string): Promise<void> {
    await fs.promises.writeFile(path.join(repoDir, filePath), content, 'utf8');
  }

  /**
   * create test commit history from a diagram.
   * See https://sapling-scm.com/docs/internals/drawdag
   */
  async function drawdag(dag: string): Promise<void> {
    // by default, drawdag sets date to 0,
    // but this would hide all commits in the ISL,
    // so we set "now" to our javascript date so all new commits are fetched,
    // then make our commits relative to that
    const pythonLabel = 'python:';
    const input = `${dag}
${dag.includes(pythonLabel) ? '' : pythonLabel}
now('${new Date().toISOString()}')  # set the "now" time
commit(date='now')
    `;
    await sl(['debugdrawdag'], {input});
  }

  await sl(['version'])
    .catch(e => {
      testLogger.log('err in version', e);
      return e;
    })
    .then(s => {
      testLogger.log('sl version: ', s.stdout, s.stderr, s.exitCode);
    });

  // set up empty repo
  await sl(['init', '--config=format.use-eager-repo=True', '--config=init.prefer-git=False', '.']);
  await writeFileInRepo('.watchmanconfig', '{}');
  const dotdir = fs.existsSync(path.join(repoDir, '.sl')) ? '.sl' : '.hg';
  // write to repo config
  await writeFileInRepo(
    `${dotdir}/config`,
    ([['paths', [`default=eager:${repoDir}`]]] as [string, string[]][])
      .map(([section, configs]) => `[${section}]\n${configs.join('\n')}`)
      .join('\n'),
  );
  await writeFileInRepo('file.txt', 'hello');
  await sl(['commit', '-A', '-m', 'Initial Commit']);

  const {
    serverToClient,
    clientToServer,
    dispose: disposeClientConnection,
  } = mockedClientMessagebus as unknown as MockedClientMessageBus;

  const serverLogger = wrapLogger(console, '[server]');

  // start "server" in the same process, connected to fake client message bus via eventEmitters
  const disposeServer = onClientConnection({
    cwd: repoDir,
    version: 'integration-test',
    command: cmd,
    logger: serverLogger,
    appMode: {mode: 'isl'},

    postMessage(message: string): Promise<boolean> {
      serverToClient.emit('data', message);
      return Promise.resolve(true);
    },
    onDidReceiveMessage(handler: (event: Buffer, isBinary: boolean) => void | Promise<void>): {
      dispose(): void;
    } {
      const cb = (e: string | ArrayBuffer) => {
        e instanceof ArrayBuffer
          ? handler(e as Buffer, true)
          : handler(Buffer.from(e, 'utf8'), false);
      };
      clientToServer.on('data', cb);
      return {
        dispose: () => {
          clientToServer.off('data', cb);
        },
      };
    },
  });

  const refresh = () => {
    testLogger.log('refreshing');
    fireEvent.click(screen.getByTestId('refresh-button'));
  };

  // Dynamically import App so our test setup happens before App globals like jotai state are run.
  const App = (await import('../src/App')).default;
  // Render the entire app, which automatically starts the connection to the server
  render(<App />);

  return {
    repoDir,
    sl,
    cleanup: async () => {
      testLogger.log(' -------- cleaning up -------- ');
      disposeServer();
      disposeClientConnection();
      if (!IS_CI) {
        testLogger.log('removing repo dir');
        // rm -rf the temp dir with the repo in it
        // skip on CI because it can cause flakiness, and the job will get cleaned up anyway
        await retry(() => fs.promises.rm(repoDir, {recursive: true, force: true})).catch(() => {
          testLogger.log('failed to clean up temp dir: ', repoDir);
        });
      }
    },
    writeFileInRepo,
    drawdag,
    testLogger,
    refresh,
  };
}

async function retry<T>(cb: () => Promise<T>): Promise<T> {
  try {
    return await cb();
  } catch {
    return cb();
  }
}
