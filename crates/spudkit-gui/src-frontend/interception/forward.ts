const invoke = window.__TAURI__.core.invoke;

export async function forwardRequest(request: Request): Promise<Response> {
  const url = new URL(request.url);
  const body = await request.text();
  const contentType = request.headers.get("content-type") || undefined;

  const text = await invoke("forward", {
    method: request.method,
    path: url.pathname,
    body: body || undefined,
    contentType,
  });

  return new Response(text, {
    status: 200,
    headers: { "Content-Type": contentType || "application/json" },
  });
}
