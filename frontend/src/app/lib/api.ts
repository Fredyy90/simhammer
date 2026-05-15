// API URL detection: in Electron, the backend serves the frontend on the
// same origin, so window.location.origin always points at the right backend
// (matters when the Electron main process falls back to an ephemeral port
// because 17384 was already in use — see desktop/src/main/backend.js).
export const API_URL =
  typeof window !== 'undefined' && window.electronAPI
    ? window.location.origin
    : (process.env.NEXT_PUBLIC_API_URL ?? '');

/** Fetch JSON with consistent error handling. Throws on non-ok responses. */
export async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const data = await res.json().catch(() => ({}));
    throw new Error(data.detail || `Server error ${res.status}`);
  }
  return res.json();
}
