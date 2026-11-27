import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import { api, RequestError } from './identity';

export interface Attachment {
  id: string;
  label: string;
  media_type: string;
  uploader_id: string;
  uploader_name: string;
  activity_date: string | null;
  from_inbox: boolean;
  matched_by: string | null;
  matched_by_name: string | null;
  source_url: string | null;
  copy_asset_id: string | null;
}

interface Upload {
  id: string;
  received: number;
  asset_id: string | null;
}

export const photoTypes = 'image/jpeg,image/png,image/webp';
export const fileTypes = photoTypes + ',application/pdf,application/zip,video/mp4,video/quicktime,video/webm';
export const pickerLimit = 8 * 1024 * 1024 * 1024;
const partBytes = 8 * 1024 * 1024;
export const inbox = '/family/photo-inbox';

export function matchPath(asset: string, moment: string) {
  return `${inbox}/${asset}/match/${moment}`;
}

export function attachmentPath(moment: string, asset: string) {
  return `/moments/${moment}/attachments/${asset}`;
}

export async function uploadFile(file: File, submission: string, csrf: string, progress: (bytes: number) => void) {
  const digest = sha256.create();
  for (let offset = 0; offset < file.size; offset += partBytes) {
    digest.update(new Uint8Array(await file.slice(offset, offset + partBytes).arrayBuffer()));
  }
  let upload = await api<Upload>('/uploads', 'POST', {
    submission_key: submission,
    label: file.name,
    byte_count: file.size,
    media_type: file.type === 'application/x-zip-compressed' ? 'application/zip'
      : file.type || 'application/octet-stream',
    sha256: bytesToHex(digest.digest()),
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
