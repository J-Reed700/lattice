import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandArgList, CommandName } from '@/shared/ipc/transport';

export const learningCall = <C extends CommandName>(command: C, ...args: CommandArgList<C>) =>
  studyActivityCall('learning', command, ...args);
