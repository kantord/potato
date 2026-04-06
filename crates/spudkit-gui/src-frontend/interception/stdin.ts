const invoke = window.__TAURI__.core.invoke;

export async function sendStdinRequest(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const m = url.pathname.match(/^\/_api\/calls\/([^/]+)\/stdin$/);
  if (!m) return new Response(JSON.stringify({ ok: false }), { status: 400 });

  const callId = m[1];
  let data: unknown = null;
  try {
    const body = await request.json();
    data = body.data ?? null;
  } catch { /* ignore */ }

  await invoke("send_stdin", { callId, data });
  return new Response(JSON.stringify({ ok: true }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}
