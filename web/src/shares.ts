export interface SharedOriginal {
  id: string;
  label: string;
  uploader_name: string;
}

export interface SharedMoment {
  id: string;
  voice: 'student' | 'guardian' | 'scribed';
  note: string | null;
  activity_date: string | null;
  learner_name: string;
  recorder_name: string;
  originals: SharedOriginal[];
}
