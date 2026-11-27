export interface Learner {
  id: string;
  display_name: string;
}

export interface Moment {
  id: string;
  learner_id: string;
  learner_name: string;
  recorder_id: string;
  recorder_name: string;
  editor_id: string;
  editor_name: string;
  voice: 'student' | 'guardian' | 'scribed';
  activity_date: string | null;
  created_at: string;
  updated_at: string;
  revision: number;
  note: string | null;
  asset_ids: string[];
  confirmed: boolean;
}

const householdDate = new Intl.DateTimeFormat('en-US', {
  timeZone: 'America/Los_Angeles',
  dateStyle: 'medium',
});

export function readableDate(timestamp: string): string {
  return householdDate.format(new Date(timestamp));
}

export function eventDate(moment: Moment): string {
  return moment.activity_date ?? readableDate(moment.created_at);
}

export function attribution(moment: Moment): string {
  switch (moment.voice) {
    case 'student': return moment.learner_name;
    case 'scribed': return `${moment.learner_name}’s words, entered by ${moment.recorder_name}`;
    case 'guardian': return `Observation by ${moment.recorder_name}`;
  }
}


export interface Comment {
  id: string;
  author_id: string;
  author_name: string;
  body: string;
  kind: 'comment' | 'suggestion';
  created_at: string;
  withdrawn: boolean;
  share_id: string | null;
  feedback_writer: string | null;
  feedback_date: string | null;
  feedback_asset_id: string | null;
  feedback_uploader_name: string | null;
}

export interface MomentDraft {
  edit?: {
    revision: number;
    note: string;
    activityDate: string;
  };
  comment: string;
  kind: Comment['kind'];
  submission: string;
}
