/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import {codeReviewProvider, diffSummary} from './codeReview/CodeReviewInfo';
import {t, T} from './i18n';
import {UncommitOperation} from './operations/Uncommit';
import {useRunOperation} from './operationsState';
import foundPlatform from './platform';
import {dagWithPreviews} from './previews';
import {Button} from 'isl-components/Button';
import {Icon} from 'isl-components/Icon';
import {DOCUMENTATION_DELAY, Tooltip} from 'isl-components/Tooltip';
import {useAtomValue} from 'jotai';

export function UncommitButton() {
  const dag = useAtomValue(dagWithPreviews);
  const headCommit = dag.resolve('.');

  const provider = useAtomValue(codeReviewProvider);
  const diff = useAtomValue(diffSummary(headCommit?.diffId));
  const isClosed = provider != null && diff.value != null && provider?.isDiffClosed(diff.value);

  const runOperation = useRunOperation();
  if (!headCommit || dag.children(headCommit?.hash).size > 0) {
    // if the head commit has children, we can't uncommit
    return null;
  }

  if (isClosed) {
    return null;
  }
  return (
    <Tooltip
      delayMs={DOCUMENTATION_DELAY}
      title={t(
        'Remove this commit, but keep its changes as uncommitted changes, as if you never ran commit.',
      )}>
      <Button
        onClick={async () => {
          const confirmed = await foundPlatform.confirm(
            t('Are you sure you want to Uncommit?'),
            t(
              'Uncommitting will remove this commit, but keep its changes as uncommitted changes, as if you never ran commit.',
            ),
          );
          if (!confirmed) {
            return;
          }
          runOperation(new UncommitOperation(headCommit));
        }}
        icon>
        <Icon icon="debug-step-out" slot="start" />
        <T>Uncommit</T>
      </Button>
    </Tooltip>
  );
}
