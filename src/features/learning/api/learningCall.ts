import { studyActivityCall } from '@/lib/studyActivity';
import type { CommandName } from '@/shared/ipc/transport';

export const learningCall = <T>(command: CommandName, args?: Record<string, unknown>) =>
  studyActivityCall<T>('learning', command, args);
