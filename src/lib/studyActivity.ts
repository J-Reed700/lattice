import { queryClient } from './queryClient';

export type StudyTab = 'starting_point' | 'lessons' | 'sources' | 'quick-checks' | 'assess-evidence' | 'plan' | 'portability' | 'workbench' | 'practical' | 'notebook' | 'recall' | 'canvas';
export type StudyDestination = { programId?: string; tab?: StudyTab; lessonId?: string; moduleId?: string; formId?: string; deckId?: string; flashcards?: boolean };
type TaskDescription = { title: string; detail: string; involves: string[]; tab?: StudyTab; quickStart?: boolean };

const model = (title: string, tab: StudyTab, ...involves: string[]): TaskDescription => ({
  title, tab, involves, quickStart: true,
  detail: 'This task uses your model to prepare a result. Generating and checking material can take several minutes.',
});
const source = (title: string, ...involves: string[]): TaskDescription => ({
  title, tab: 'sources', involves, quickStart: true,
  detail: 'Large references and slow websites can take a while to read and save.',
});

// Describe the work, never manufacture a current stage or percentage from a timer.
const descriptions: Record<string, TaskDescription> = {
  start_learning_diagnostic: model('Preparing your starting-point check', 'starting_point', 'Use the course goals to create questions', 'Save the questions for you to answer'),
  submit_learning_diagnostic: model('Reviewing your starting-point answers', 'starting_point', 'Assess your answers', 'Save recommendations for your learning plan'),
  generate_learning_card_drafts: model('Creating recall cards', 'recall', 'Read the lesson and its references', 'Draft questions and answers for your review'),
  request_learning_tutor_response: model('Preparing tutor feedback', 'workbench', 'Read your question and current work', 'Prepare guidance for this practice session'),
  reveal_learning_practice_solution: model('Preparing the worked solution', 'workbench', 'Read the assignment and references', 'Explain a solution you can compare with your work'),
  submit_learning_practice_attempt: model('Reviewing your practice work', 'workbench', 'Assess your submitted work', 'Save feedback and suggested next steps'),
  create_learning_assessment_blueprint: model('Preparing your assessment', 'assess-evidence', 'Build questions from the learning objectives', 'Review the questions and answer keys'),
  submit_learning_assessment_form: model('Reviewing your assessment', 'assess-evidence', 'Evaluate the submitted responses', 'Save the assessment evidence and follow-up suggestions'),
  generate_learning_practical_activity: model('Building your practical activity', 'practical', 'Design an activity for this lesson', 'Prepare the instructions and evaluation criteria'),
  send_learning_simulation_turn: model('Preparing the next simulation turn', 'practical', 'Read your response and the simulation history', 'Generate the next turn'),
  finish_learning_simulation: model('Reviewing your simulation', 'practical', 'Evaluate the completed conversation', 'Save feedback on your performance'),
  prepare_learning_runtime_preset: { title: 'Setting up your practice environment', tab: 'practical', quickStart: true, detail: 'The first setup may need a large download. You can browse your course while it runs.', involves: ['Check the local container engine', 'Download the environment if needed', 'Save the environment for future exercises'] },
  add_learning_web_source: source('Saving your reference page', 'Fetch the selected page', 'Extract and save its text as a course reference'),
  add_learning_document_source: source('Saving your document reference', 'Read the selected document', 'Save a reference version for this course'),
  add_learning_text_source: source('Saving your reference text', 'Save your text as a course reference'),
  refresh_learning_source: source('Checking your reference for updates', 'Fetch the source again', 'Compare it with the saved version'),
  search_learning_sources_semantically: { ...source('Finding relevant passages', 'Search the saved references for relevant passages'), detail: 'Searching the text saved in this course for relevant evidence.' },
  export_learning_pack: { title: 'Exporting your study pack', tab: 'portability', quickStart: true, detail: 'Larger courses and attachments take longer to package.', involves: ['Collect the selected course material', 'Write the study pack to your chosen location'] },
  preview_learning_pack_import: { title: 'Checking your study pack', tab: 'portability', quickStart: true, detail: 'Reading the pack and checking its contents before you decide what to import.', involves: ['Read and validate the pack', 'Prepare an import preview'] },
  apply_learning_pack_import: { title: 'Importing your study pack', tab: 'portability', quickStart: true, detail: 'Saving the selected material into your library.', involves: ['Import the selected material', 'Record the import result'] },
  generate_study_deck: { ...model('Creating your flashcard deck', 'recall', 'Read the selected documents', 'Generate cited questions and answers', 'Save your deck'), tab: undefined },
  generate_conversation_study_deck: { ...model('Creating flashcards from your conversation', 'recall', 'Read the verified claims and citations', 'Create and save a deck from those claims'), tab: undefined },
};

// These operations already expose real progress and controls in Studio. A job
// start/retry response only acknowledges queuing; it is not job completion.
const dedicatedProgress = new Set([
  'generate_learning_program', 'repair_learning_outline', 'cancel_learning_outline',
  'prepare_learning_lesson', 'start_learning_generation_job', 'retry_learning_generation_job',
  'cancel_learning_generation_job', 'start_learning_practical_run', 'cancel_learning_practical_run',
]);

function describe(command: string): TaskDescription {
  if (descriptions[command]) return descriptions[command];
  const read = /^(get|list|search|match)_/.test(command);
  const tab: StudyTab | undefined = command.includes('source') ? 'sources'
    : command.includes('canvas') ? 'canvas' : command.includes('recall') || command.includes('card') ? 'recall'
      : command.includes('practical') || command.includes('runtime') || command.includes('simulation') ? 'practical'
        : command.includes('assessment') ? 'assess-evidence' : command.includes('practice') ? 'workbench'
          : command.includes('diagnostic') ? 'starting_point' : command.includes('curriculum') || command.includes('plan') ? 'plan'
            : command.includes('pack') || command.includes('portability') ? 'portability'
              : command.includes('memory') || command.includes('note') ? 'notebook' : 'lessons';
  const names: Record<StudyTab, string> = { sources: 'references', canvas: 'canvas', recall: 'recall cards', practical: 'practice environment', 'quick-checks': 'quick checks', 'assess-evidence': 'assessment', workbench: 'practice session', starting_point: 'starting-point check', plan: 'learning plan', portability: 'study packs', notebook: 'notebook', lessons: 'course' };
  return {
    title: `${read ? 'Loading' : 'Updating'} your ${command.includes('study_deck') ? 'flashcards' : names[tab]}`,
    tab, detail: read ? 'This is taking longer than usual. The request is still waiting for a response.' : 'Waiting for the app to finish this change.',
    involves: [read ? 'Retrieve the saved material' : 'Apply and save your requested change'],
  };
}

export type StudyActivity = TaskDescription & {
  id: string;
  command: string;
  destination: StudyDestination;
  courseTitle?: string;
  status: 'pending' | 'completed' | 'failed';
  startedAt: number;
  finishedAt?: number;
  error?: string;
};
export const STUDY_ACTIVITY_KEY = ['study-request-activity'] as const;
export const EMPTY_STUDY_ACTIVITY: StudyActivity[] = [];
queryClient.setQueryDefaults(STUDY_ACTIVITY_KEY, { gcTime: Infinity, staleTime: Infinity });

function publish(activity: StudyActivity) {
  queryClient.setQueryData<StudyActivity[]>(STUDY_ACTIVITY_KEY, (current = []) => {
    const next = [...current.filter(item => item.id !== activity.id), activity];
    // Bound receipts, never evict an active request. This is not an execution limit.
    const finished = next.filter(item => item.status !== 'pending').slice(-12);
    return next.filter(item => item.status === 'pending' || finished.includes(item));
  });
}

export function dismissStudyActivity(id?: string) {
  queryClient.setQueryData<StudyActivity[]>(STUDY_ACTIVITY_KEY, (current = []) =>
    current.filter(item => item.status === 'pending' || (id !== undefined && item.id !== id)));
}

/** UI receipts for slow IPC requests, not a second store of course/job data.
 * No request payload, source text, answer, or result is retained here. */
export function beginStudyActivity(domain: string, command: string, args?: Record<string, unknown>) {
  if (!['learning', 'study'].includes(domain) || dedicatedProgress.has(command)) return undefined;
  const request = args?.request as { programId?: unknown; lessonId?: unknown; moduleId?: unknown; formId?: unknown; saveOnly?: boolean } | undefined;
  const description = command === 'submit_learning_diagnostic' && request?.saveOnly
    ? { title: 'Saving your starting-point answers', tab: 'starting_point' as const, detail: 'Waiting for your answers to be saved.', involves: ['Save your answers so you can return to them'] }
    : describe(command);
  const programId = typeof request?.programId === 'string' ? request.programId
    : typeof args?.programId === 'string' ? args.programId
      : /^(get_learning_(program|plan|memory|.*workspace)|delete_learning_program)$/.test(command) && typeof args?.id === 'string' ? args.id : undefined;
  const course = programId ? queryClient.getQueryData<{ summary: { title: string } }>(['learning-program', programId]) : undefined;
  const activity: StudyActivity = {
    ...description, id: crypto.randomUUID(), command,
    destination: {
      programId, tab: description.tab,
      lessonId: typeof request?.lessonId === 'string' ? request.lessonId : undefined,
      moduleId: typeof request?.moduleId === 'string' ? request.moduleId : undefined,
      formId: typeof request?.formId === 'string' ? request.formId : undefined,
      flashcards: domain === 'study', deckId: command === 'get_study_deck' && typeof args?.id === 'string' ? args.id : undefined,
    },
    courseTitle: course?.summary?.title,
    status: 'pending', startedAt: Date.now(),
  };
  let visible = false;
  let settled = false;
  // Only delay presentation. There is deliberately no timeout, abort, or retry.
  const timer = setTimeout(() => { visible = true; publish(activity); }, description.quickStart ? 800 : 4000);
  return {
    finish(error?: string, result?: unknown) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (!visible) return;
      const deckId = domain === 'study' && command.startsWith('generate_') && result && typeof result === 'object' && 'id' in result && typeof result.id === 'string' ? result.id : undefined;
      publish({ ...activity, status: error === undefined ? 'completed' : 'failed', error, finishedAt: Date.now(), destination: deckId ? { ...activity.destination, deckId } : activity.destination });
    },
  };
}
