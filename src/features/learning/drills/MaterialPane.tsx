import type { ComponentProps } from 'react';

import { CodePane } from './CodePane';
import { kindOf } from './drillMaterial';
import { PassagePane } from './PassagePane';

/** The drill's material to read and pick from: code with its files, or an explanation as prose. */
export function MaterialPane(props: ComponentProps<typeof CodePane>) {
  if (kindOf(props.drill) === 'passage') {
    const { activePath: _activePath, onActivePath: _onActivePath, ...pane } = props;
    return <PassagePane {...pane} />;
  }
  return <CodePane {...props} />;
}
