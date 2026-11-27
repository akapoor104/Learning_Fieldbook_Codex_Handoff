import { api, RequestError } from './identity';

export interface Attachment {
  id: string;
  label: string;
  uploader_id: string;
  uploader_name: string;
  activity_date: string | null;
  from_inbox: boolean;
  matched_by: string | null;
  matched_by_name: string | null;
}

interface Upload {
  id: string;
  received: number;
  asset_id: string | null;
}

export const photoTypes = 'image/jpeg,image/png,image/webp';
export const fileTypes = photoTypes + ',application/pdf,application/zip,video/mp4,video/quicktime,video/webm';
export const pickerLimit = 64 * 1024 * 1024;
const partBytes = 8 * 1024 * 1024;
export const inbox = '/family/photo-inbox';

export function matchPath(asset: string, moment: string) {
  return `${inbox}/${asset}/match/${moment}`;
}

export async function uploadFile(file: File, submission: string, csrf: string, progress: (bytes: number) => void) {
  // This picker bounds the one-shot browser digest. The server still accepts
  // larger resumable transfers from clients that compute digests incrementally.
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer()));
  const sha256 = Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join('');
  let upload = await api<Upload>('/uploads', 'POST', {
    submission_key: submission,
    label: file.name,
    byte_count: file.size,
    media_type: file.type === 'application/x-zip-compressed' ? 'application/zip'
      : file.type || 'application/octet-stream',
    sha256,
  }, csrf);
  while (upload.received < file.size) {
    progress(upload.received);
    const response = await fetch(`/api/v1/uploads/${upload.id}/parts/${upload.received}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/octet-stream', 'X-CSRF-Token': csrf },
      body: file.slice(upload.received, upload.received + partBytes),
      cache: 'no-store',
    });
    if (!response.ok) throw new RequestError(response.status, null);
    upload = await response.json();
  }
  progress(file.size);
  if (!upload.asset_id) {
    upload = await api<Upload>(`/uploads/${upload.id}/finalize`, 'POST', undefined, csrf);
  }
  if (!upload.asset_id) throw new Error('Upload was not finalized');
  return upload.asset_id;
}
