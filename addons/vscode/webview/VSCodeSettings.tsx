/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import type {Json} from 'shared/typeUtils';

import serverAPI from '../../isl/src/ClientToServerAPI';
import {ComparisonPanelMode, comparisonPanelMode, setComparisonPanelMode} from './state';
import {Checkbox} from 'isl-components/Checkbox';
import {Dropdown} from 'isl-components/Dropdown';
import {Tooltip} from 'isl-components/Tooltip';
import {Setting} from 'isl/src/Setting';
import {T, t} from 'isl/src/i18n';
import {writeAtom} from 'isl/src/jotaiUtils';
import {atom, useAtom, useAtomValue} from 'jotai';

export default function VSCodeSettings() {
  const panelMode = useAtomValue(comparisonPanelMode);
  const [openBesides, setOpenBesides] = useAtom(openBesidesSetting);
  return (
    <Setting title={<T>VS Code Settings</T>}>
      <Tooltip
        title={t(
          'Whether to always open a separate panel to view comparisons, or to open the comparison inside an existing ISL window.',
        )}>
        <div className="dropdown-container setting-inline-dropdown">
          <label>
            <T>Comparison Panel Mode</T>
          </label>
          <Dropdown
            options={Object.values(ComparisonPanelMode).map(name => ({name, value: name}))}
            value={panelMode}
            onChange={event =>
              setComparisonPanelMode(event.currentTarget.value as ComparisonPanelMode)
            }
          />
        </div>
      </Tooltip>
      <Tooltip
        title={t(
          'If true, files, diffs, and comparisons will open beside the existing ISL panel instead of in the same View Column. Useful to keep ISL open and visible when clicking on files.',
        )}>
        <Checkbox checked={openBesides} onChange={checked => setOpenBesides(checked)}>
          <T>Open Besides</T>
        </Checkbox>
      </Tooltip>
    </Setting>
  );
}

const openBesidesSetting = vscodeConfigBackedAtom<boolean>('sapling.isl.openBeside', false);

function vscodeConfigBackedAtom<T extends Json>(
  configName: string,
  defaultValue: T,
  scope: 'global' | 'workspace' = 'global',
) {
  const primitiveAtom = atom<T>(defaultValue);

  serverAPI.postMessage({
    type: 'platform/subscribeToVSCodeConfig',
    config: configName,
  });
  serverAPI.onMessageOfType('platform/vscodeConfigChanged', config => {
    if (config.config === configName) {
      writeAtom(primitiveAtom, config.value as T);
    }
  });

  return atom<T, [T | ((prev: T) => T)], void>(
    get => get(primitiveAtom),
    (get, set, update) => {
      const newValue = typeof update === 'function' ? update(get(primitiveAtom)) : update;
      set(primitiveAtom, newValue);
      serverAPI.postMessage({
        type: 'platform/setVSCodeConfig',
        config: configName,
        value: newValue,
        scope,
      });
    },
  );
}
