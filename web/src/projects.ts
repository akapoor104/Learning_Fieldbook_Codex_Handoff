export type LabelKind = 'domain' | 'project_type' | 'event_type' | 'role' | 'skill' | 'tag';

export const labelKinds: Record<LabelKind, string> = {
  domain: 'Domain', project_type: 'Project type', event_type: 'Event type',
  role: 'Event role', skill: 'Skill', tag: 'Tag',
};

export interface Label {
  id: string;
  kind: LabelKind;
  name: string;
  retired: boolean;
}

export interface Project {
  id: string;
  title: string;
  type_id: string | null;
  domain_ids: string[];
  question: string | null;
  success_criterion: string | null;
  state: string;
}

export interface Catalog {
  projects: Project[];
  labels: Label[];
}

export interface Source {
  moment_id: string;
  note: string | null;
  activity_date: string | null;
  recorder_name: string;
  learner_name: string;
  voice: string;
}

export interface TimelineEntry extends Source {
  occurred_on: string;
  recorded_at: string;
  primary_project_id: string | null;
  withdrawn: boolean;
}

export interface AttemptDetails {
  event: string | null;
  event_type_id: string | null;
  outcome: string | null;
  my_part: string | null;
  others_part: string | null;
  help: string | null;
  checking: string | null;
  prior_attempt_id: string | null;
  role_ids: string[];
}

export interface Attempt {
  project_id: string;
  revision: number;
  editor_id: string;
  editor_name: string;
  details: AttemptDetails;
}

export const emptyAttempt = (): AttemptDetails => ({
  event: null, event_type_id: null, outcome: null, my_part: null, others_part: null,
  help: null, checking: null, prior_attempt_id: null, role_ids: [],
});

export function labelsFor(labels: Label[], kind: LabelKind, retained: string[] = []) {
  return labels.filter((label) => label.kind === kind && (!label.retired || retained.includes(label.id)));
}

export function labelName(labels: Label[], id: string | null) {
  const label = labels.find((item) => item.id === id);
  return label ? `${label.name}${label.retired ? ' (retired)' : ''}` : '';
}

export function sourceName(source: Source) {
  return source.note || 'Moment with attachments';
}

export function sourceVoice(source: Source) {
  if (source.voice === 'guardian') return `Observation by ${source.recorder_name}`;
  if (source.voice === 'scribed') return `${source.learner_name}’s words, entered by ${source.recorder_name}`;
  return source.learner_name;
}
