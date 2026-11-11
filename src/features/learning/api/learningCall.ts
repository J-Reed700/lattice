import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandArgs } from '@/lib/bindings';
import type { CommandName } from '@/shared/ipc/transport';

export const learningCall = <C extends CommandName>(command: C, args: CommandArgs[C]) =>
  studyActivityCall('learning', command, args);
