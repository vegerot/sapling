/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Heartbeat} from '../heartbeat';
import type {ReactNode} from 'react';
import type {ExclusiveOr} from 'shared/typeUtils';

import {colors} from '../../../components/theme/tokens.stylex';
import {debugLogMessageTraffic} from '../ClientToServerAPI';
import {Column, Row} from '../ComponentUtils';
import {DropdownField, DropdownFields} from '../DropdownFields';
import messageBus from '../MessageBus';
import {enableReactTools, enableReduxTools} from '../atoms/debugToolAtoms';
import {holdingCtrlAtom} from '../atoms/keyboardAtoms';
import {DagCommitInfo} from '../dag/dagCommitInfo';
import {useHeartbeat} from '../heartbeat';
import {t, T} from '../i18n';
import {atomWithOnChange} from '../jotaiUtils';
import {NopOperation} from '../operations/NopOperation';
import {useRunOperation} from '../operationsState';
import platform from '../platform';
import {dagWithPreviews} from '../previews';
import {RelativeDate} from '../relativeDate';
import {
  latestCommitsData,
  latestUncommittedChangesData,
  mergeConflicts,
  repositoryInfo,
} from '../serverAPIState';
import {showToast} from '../toast';
import {isDev} from '../utils';
import {ComponentExplorerButton} from './ComponentExplorer';
import {readInterestingAtoms, serializeAtomsState} from './getInterestingAtoms';
import * as stylex from '@stylexjs/stylex';
import {Badge} from 'isl-components/Badge';
import {Button} from 'isl-components/Button';
import {Checkbox} from 'isl-components/Checkbox';
import {InlineErrorBadge} from 'isl-components/ErrorNotice';
import {Subtle} from 'isl-components/Subtle';
import {Tooltip} from 'isl-components/Tooltip';
import {atom, useAtom, useAtomValue} from 'jotai';
import {useState, useCallback, useEffect} from 'react';

import './DebugToolsMenu.css';

/* eslint-disable no-console */

export default function DebugToolsMenu({dismiss}: {dismiss: () => unknown}) {
  return (
    <DropdownFields
      title={<T>Internal Debugging Tools</T>}
      icon="pulse"
      data-testid="internal-debug-tools-dropdown"
      className="internal-debug-tools-dropdown">
      <Subtle>
        <T>
          This data is only intended for debugging Interactive Smartlog and may not capture all
          issues.
        </T>
      </Subtle>
      <DropdownField title={<T>Performance</T>}>
        <DebugPerfInfo />
      </DropdownField>
      <DropdownField title={<T>Commit graph</T>}>
        <DebugDagInfo />
      </DropdownField>
      <DropdownField title={<T>Internal State</T>}>
        <InternalState />
      </DropdownField>
      <DropdownField title={<T>Server/Client Messages</T>}>
        <Column alignStart>
          <ServerClientMessageLogging />
          <Row>
            <ForceDisconnectButton />
            <NopOperationButtons />
          </Row>
        </Column>
      </DropdownField>
      <DropdownField title={<T>Component Explorer</T>}>
        <ComponentExplorerButton dismiss={dismiss} />
      </DropdownField>
    </DropdownFields>
  );
}

function InternalState() {
  const [reduxTools, setReduxTools] = useAtom(enableReduxTools);
  const [reactTools, setReactTools] = useAtom(enableReactTools);
  const needSerialize = useAtomValue(holdingCtrlAtom);

  const generate = () => {
    // No need for useAtomValue - no need to re-render or recalculate this function.
    const atomsState = readInterestingAtoms();
    const value = needSerialize ? serializeAtomsState(atomsState) : atomsState;
    console.log(`jotai state (${needSerialize ? 'JSON' : 'objects'}):`, value);
    showToast(`logged jotai state to console!${needSerialize ? ' (serialized)' : ''}`);
  };

  return (
    <Column alignStart>
      <Row>
        <Tooltip
          placement="bottom"
          title={t(
            'Capture a snapshot of selected Jotai atom states, log it to the dev tools console.\n\n' +
              'Hold Ctrl to use serailzied JSON instead of Javascript objects.',
          )}>
          <Button onClick={generate}>
            {needSerialize ? <T>Take Snapshot (JSON)</T> : <T>Take Snapshot (objects)</T>}
          </Button>
        </Tooltip>
        <Tooltip
          placement="bottom"
          title={t(
            'Log persisted state (localStorage or vscode storage) to the dev tools console.',
          )}>
          <Button
            onClick={() => {
              console.log('persisted state:', platform.getAllPersistedState());
              showToast('logged persisted state to console!');
            }}>
            <T>Log Persisted State</T>
          </Button>
        </Tooltip>
        <Tooltip
          placement="bottom"
          title={t(
            'Clear any persisted state (localStorage or vscode storage). Usually only matters after restarting.',
          )}>
          <Button
            onClick={() => {
              platform.clearPersistedState();
              console.log('--- cleared isl persisted state ---');
              showToast('cleared persisted state');
            }}>
            <T>Clear Persisted State</T>
          </Button>
        </Tooltip>
      </Row>
      {isDev && (
        <Row>
          <T>Integrate with: </T>
          <Checkbox checked={reduxTools} onChange={setReduxTools}>
            <T>Redux DevTools</T>
          </Checkbox>
          <Checkbox checked={reactTools} onChange={setReactTools}>
            {t('React <DebugAtoms/>')}
          </Checkbox>
        </Row>
      )}
    </Column>
  );
}

const logMessagesState = atomWithOnChange(
  atom(debugLogMessageTraffic.shoudlLog),
  newValue => {
    debugLogMessageTraffic.shoudlLog = newValue;
    console.log(`----- ${newValue ? 'Enabled' : 'Disabled'} Logging Messages -----`);
  },
  /* skipInitialCall */ true,
);

function ServerClientMessageLogging() {
  const [shouldLog, setShouldLog] = useAtom(logMessagesState);
  return (
    <div>
      <Checkbox checked={shouldLog} onChange={checked => setShouldLog(checked)}>
        <T>Log messages</T>
      </Checkbox>
    </div>
  );
}

function DebugPerfInfo() {
  const latestStatus = useAtomValue(latestUncommittedChangesData);
  const latestLog = useAtomValue(latestCommitsData);
  const latestConflicts = useAtomValue(mergeConflicts);
  const heartbeat = useHeartbeat();
  const repoInfo = useAtomValue(repositoryInfo);
  let commandName = 'sl';
  if (repoInfo?.type === 'success') {
    commandName = repoInfo.command;
  }
  return (
    <div>
      {heartbeat.type === 'timeout' ? (
        <InlineErrorBadge error={new Error(t('Heartbeat timeout'))}>
          <T>Heartbeat timed out</T>
        </InlineErrorBadge>
      ) : (
        <FetchDurationInfo
          name={<T>ISL Server Ping</T>}
          duration={(heartbeat as Heartbeat & {type: 'success'})?.rtt}
        />
      )}
      <FetchDurationInfo
        name={<T replace={{$cmd: commandName}}>$cmd status</T>}
        start={latestStatus.fetchStartTimestamp}
        end={latestStatus.fetchCompletedTimestamp}
      />
      <FetchDurationInfo
        name={<T replace={{$cmd: commandName}}>$cmd log</T>}
        start={latestLog.fetchStartTimestamp}
        end={latestLog.fetchCompletedTimestamp}
      />
      <FetchDurationInfo
        name={<T>Merge Conflicts</T>}
        start={latestConflicts?.fetchStartTimestamp}
        end={latestConflicts?.fetchCompletedTimestamp}
      />
    </div>
  );
}

const styles = stylex.create({
  slow: {
    color: colors.signalFg,
    backgroundColor: colors.signalBadBg,
  },
  ok: {
    color: colors.signalFg,
    backgroundColor: colors.signalMediumBg,
  },
  fast: {
    color: colors.signalFg,
    backgroundColor: colors.signalGoodBg,
  },
});

function FetchDurationInfo(
  props: {name: ReactNode} & ExclusiveOr<{start?: number; end?: number}, {duration: number}>,
) {
  const {name} = props;
  const {end, start, duration} = props;
  const deltaMs = duration != null ? duration : end == null || start == null ? null : end - start;
  const xstyle =
    deltaMs == null
      ? undefined
      : deltaMs < 1000
      ? styles.fast
      : deltaMs < 3000
      ? styles.ok
      : styles.slow;
  return (
    <div className={`fetch-duration-info`}>
      {name} <Badge xstyle={xstyle}>{deltaMs == null ? 'N/A' : `${deltaMs}ms`}</Badge>
      {end == null ? null : (
        <Subtle>
          <Tooltip title={new Date(end).toLocaleString()} placement="right">
            <RelativeDate date={end} />
          </Tooltip>
        </Subtle>
      )}
    </div>
  );
}

function useMeasureDuration(slowOperation: () => void): number | null {
  const [measured, setMeasured] = useState<null | number>(null);
  useEffect(() => {
    requestIdleCallback(() => {
      const startTime = performance.now();
      slowOperation();
      const endTime = performance.now();
      setMeasured(endTime - startTime);
    });
    return () => setMeasured(null);
  }, [slowOperation]);
  return measured;
}

function DebugDagInfo() {
  const dag = useAtomValue(dagWithPreviews);
  const dagRenderBenchmark = useCallback(() => {
    // Slightly change the dag to invalidate its caches.
    const noise = performance.now();
    const newDag = dag.add([DagCommitInfo.fromCommitInfo({hash: `dummy-${noise}`, parents: []})]);
    newDag.renderToRows(newDag.subsetForRendering());
  }, [dag]);

  const dagSize = dag.all().size;
  const dagDisplayedSize = dag.subsetForRendering().size;
  const dagSortMs = useMeasureDuration(dagRenderBenchmark);

  return (
    <div>
      <T>Size: </T>
      {dagSize}
      <br />
      <T>Displayed: </T>
      {dagDisplayedSize}
      <br />
      <>
        <T>Render calculation: </T>
        {dagSortMs == null ? (
          <T>(Measuring)</T>
        ) : (
          <>
            {dagSortMs.toFixed(1)} <T>ms</T>
          </>
        )}
        <br />
      </>
    </div>
  );
}

const forceDisconnectDuration = atom<number>(3000);

function ForceDisconnectButton() {
  const [duration, setDuration] = useAtom(forceDisconnectDuration);
  const forceDisconnect = messageBus.forceDisconnect?.bind(messageBus);
  if (forceDisconnect == null) {
    return null;
  }
  return (
    <Button
      onClick={() => forceDisconnect(duration)}
      onWheel={e => {
        // deltaY is usually -100 +100 per event.
        const dy = e.deltaY;
        const scale = duration < 20000 ? 10 : 100;
        if (dy > 0) {
          setDuration(v => Math.max(v - dy * scale, 1000));
        } else if (dy < 0) {
          setDuration(v => Math.min(v - dy * scale, 300000));
        }
      }}>
      <T replace={{$sec: (duration / 1000).toFixed(1)}}>Force disconnect for $sec seconds</T>
    </Button>
  );
}

function NopOperationButtons() {
  const runOperation = useRunOperation();
  return (
    <>
      {[2, 5, 10].map(durationSeconds => (
        <Button
          key={durationSeconds}
          onClick={() => runOperation(new NopOperation(durationSeconds))}>
          <T replace={{$sec: durationSeconds}}>Nop $sec s</T>
        </Button>
      ))}
    </>
  );
}
