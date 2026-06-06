import { invokeCommand } from '../invoke';
import { isElectronRuntime } from '../runtime';

export function toggleNativeDevtools(label?: string): Promise<void> {
  const args = label ? { label } : undefined;

  if (isElectronRuntime()) {
    return invokeCommand<void>('toggle_devtools', args);
  }

  return invokeCommand<void>('toggle_devtools', args);
}
