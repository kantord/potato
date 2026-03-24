const Channel = window.__TAURI__.core.Channel;
const invoke = window.__TAURI__.core.invoke;
const encoder = new TextEncoder();

export function streamRequest(request: Request, body: string): Response {
  const url = new URL(request.url);
  const stream = new ReadableStream({
    start(controller) {
      const channel = new Channel<string>();

      channel.onmessage = (data: string) => {
        try {
          const parsed = JSON.parse(data);
          if (parsed.event === "end") {
            controller.close();
            return;
          }
        } catch { /* ignore parse errors */ }
        controller.enqueue(encoder.encode("data:" + data + "\n\n"));
      };

      invoke("stream", {
        method: request.method,
        path: url.pathname,
        body: body || undefined,
        onEvent: channel,
      }).catch((err: unknown) => {
        controller.error(err);
      });
    },
  });

  return new Response(stream, {
    status: 200,
    headers: { "Content-Type": "text/event-stream" },
  });
}
