import { FetchInterceptor } from "@mswjs/interceptors/fetch";
import { XMLHttpRequestInterceptor } from "@mswjs/interceptors/XMLHttpRequest";
import { BatchInterceptor } from "@mswjs/interceptors";

declare global {
  interface Window {
    __TAURI__: {
      core: {
        Channel: new <T>() => { onmessage: ((data: T) => void) | null };
        invoke: (cmd: string, args?: Record<string, unknown>) => Promise<string>;
      };
    };
  }
}

const Channel = window.__TAURI__.core.Channel;
const invoke = window.__TAURI__.core.invoke;
const encoder = new TextEncoder();

// --- Route table ---

type RouteHandler = (
  request: Request,
  match: RegExpMatchArray
) => Promise<Response>;

const routes: Array<[string, RegExp, RouteHandler]> = [
  // POST /calls — streaming via Tauri Channel
  ["POST", /^\/calls$/, async (request) => {
    const body = await request.text();
    return streamViaChannel("create_call", { body: body || "{}" });
  }],

  // POST /calls/{id}/stdin
  ["POST", /^\/calls\/([^/]+)\/stdin$/, async (request, match) => {
    const body = await request.text();
    const text = await invoke("send_call_stdin", {
      callId: match[1],
      data: body || "{}",
    });
    return new Response(text, {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  }],

  // POST /render/{script}
  ["POST", /^\/render\/(.+)$/, async (request, match) => {
    const body = await request.text();
    const contentType = request.headers.get("content-type") || "application/json";
    const text = await invoke("render", {
      script: match[1],
      body: body || "{}",
      contentType,
    });
    return new Response(text, {
      status: 200,
      headers: { "Content-Type": "text/html" },
    });
  }],
];

// --- Interceptor setup ---

const interceptor = new BatchInterceptor({
  name: "potato",
  interceptors: [new FetchInterceptor(), new XMLHttpRequestInterceptor()],
});

interceptor.apply();

interceptor.on("request", async ({ request, controller }) => {
  const url = new URL(request.url);

  for (const [method, pattern, handler] of routes) {
    if (request.method === method) {
      const match = url.pathname.match(pattern);
      if (match) {
        const response = await handler(request, match);
        controller.respondWith(response);
        return;
      }
    }
  }
  // Unmatched requests pass through automatically
});

// --- Streaming helper ---

function streamViaChannel(
  command: string,
  args: Record<string, unknown>
): Response {
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

      args.onEvent = channel;

      invoke(command, args).catch((err: unknown) => {
        controller.error(err);
      });
    },
  });

  return new Response(stream, {
    status: 200,
    headers: { "Content-Type": "text/event-stream" },
  });
}
