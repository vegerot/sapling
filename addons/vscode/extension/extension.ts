/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {EnabledSCMApiFeature} from './types';
import type {Logger} from 'isl-server/src/logger';
import type {ServerPlatform} from 'isl-server/src/serverPlatform';
import type {RepositoryContext} from 'isl-server/src/serverTypes';

import {DeletedFileContentProvider} from './DeletedFileContentProvider';
import {registerSaplingDiffContentProvider} from './DiffContentProvider';
import {Internal} from './Internal';
import {VSCodeReposList} from './VSCodeRepo';
import {InlineBlameProvider} from './blame/blame';
import {registerCommands} from './commands';
import {getCLICommand} from './config';
import {ensureTranslationsLoaded} from './i18n';
import {registerISLCommands} from './islWebviewPanel';
import {extensionVersion} from './utils';
import {getVSCodePlatform} from './vscodePlatform';
import {makeServerSideTracker} from 'isl-server/src/analytics/serverSideTracker';
import * as util from 'node:util';
import * as vscode from 'vscode';

export async function activate(context: vscode.ExtensionContext) {
  const start = Date.now();
  const [outputChannel, logger] = createOutputChannelLogger();
  const platform = getVSCodePlatform(context);
  const extensionTracker = makeServerSideTracker(
    logger,
    platform as ServerPlatform,
    extensionVersion,
  );
  try {
    const ctx: RepositoryContext = {
      cmd: getCLICommand(),
      cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd(),
      logger,
      tracker: extensionTracker,
    };
    const [, enabledSCMApiFeatures] = await Promise.all([
      ensureTranslationsLoaded(context),
      Internal.getEnabledSCMApiFeatures?.(ctx) ??
        new Set<EnabledSCMApiFeature>(['blame', 'sidebar', 'autoresolve']),
    ]);
    logger.info('enabled features: ', [...enabledSCMApiFeatures].join(', '));
    Internal.maybeOverwriteIslEnabledSetting?.(ctx);
    context.subscriptions.push(registerISLCommands(context, platform, logger));
    context.subscriptions.push(outputChannel);
    const reposList = new VSCodeReposList(logger, extensionTracker, enabledSCMApiFeatures);
    context.subscriptions.push(reposList);
    if (enabledSCMApiFeatures.has('blame')) {
      context.subscriptions.push(new InlineBlameProvider(reposList, ctx));
    }
    context.subscriptions.push(registerSaplingDiffContentProvider(ctx));
    context.subscriptions.push(new DeletedFileContentProvider());
    let inlineCommentsProvider;
    if (enabledSCMApiFeatures.has('comments') && Internal.inlineCommentsProvider) {
      inlineCommentsProvider = Internal.inlineCommentsProvider(
        context,
        reposList,
        ctx,
        enabledSCMApiFeatures.has('comments-v1'),
      );
      context.subscriptions.push(inlineCommentsProvider);
    }
    if (Internal.SaplingISLUriHandler != null) {
      context.subscriptions.push(
        vscode.window.registerUriHandler(
          new Internal.SaplingISLUriHandler(reposList, ctx, inlineCommentsProvider),
        ),
      );
    }

    context.subscriptions.push(...registerCommands(ctx));

    Internal?.registerInternalBugLogsProvider != null &&
      context.subscriptions.push(Internal.registerInternalBugLogsProvider(logger));

    extensionTracker.track('VSCodeExtensionActivated', {duration: Date.now() - start});
  } catch (error) {
    extensionTracker.error('VSCodeExtensionActivated', 'VSCodeActivationError', error as Error, {
      duration: Date.now() - start,
    });
  }
}

const logFileContents: Array<string> = [];
function createOutputChannelLogger(): [vscode.OutputChannel, Logger] {
  const outputChannel = vscode.window.createOutputChannel('Sapling ISL');
  const log = (...data: Array<unknown>) => {
    const line = util.format(...data);
    logFileContents.push(line);
    outputChannel.appendLine(line);
  };
  const outputChannelLogger = {
    log,
    info: log,
    warn: log,
    error: log,

    getLogFileContents() {
      return Promise.resolve(logFileContents.join('\n'));
    },
  } as Logger;
  return [outputChannel, outputChannelLogger];
}
