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

const nativeFetch = window.fetch;
const encoder = new TextEncoder();
const Channel = window.__TAURI__.core.Channel;
const invoke = window.__TAURI__.core.invoke;

// --- Intercept fetch() ---

window.fetch = function (input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const url = typeof input === "string" ? input : input instanceof URL ? input.toString() : input.url;
  const method = init?.method || "GET";

  // Intercept POST /calls — stream via Channel (returns SSE with started event)
  if (url === "/calls" && method === "POST") {
    return streamViaChannel("create_call", { body: init?.body || "{}" });
  }

  // Intercept POST /calls/{id}/stdin — forward via command
  const stdinMatch = url.match(/^\/calls\/([^/]+)\/stdin$/);
  if (stdinMatch && method === "POST") {
    return invoke("send_call_stdin", {
      callId: stdinMatch[1],
      data: init?.body || "{}",
    }).then((text) => {
      return new Response(text, {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });
  }

  // Intercept POST /render/{script}
  const renderMatch = url.match(/^\/render\/(.+)$/);
  if (renderMatch && method === "POST") {
    const headers = init?.headers as Record<string, string> | undefined;
    const ct = headers?.["Content-Type"] || headers?.["content-type"] || "application/json";
    return invoke("render", {
      script: renderMatch[1],
      body: init?.body || "{}",
      contentType: ct,
    }).then((text) => {
      return new Response(text, {
        status: 200,
        headers: { "Content-Type": "text/html" },
      });
    });
  }

  // Everything else — native fetch
  return nativeFetch.call(window, input, init);
} as typeof fetch;

// --- Intercept XMLHttpRequest (for HTMX 2.x which uses XHR) ---

const NativeXHR = window.XMLHttpRequest;

window.XMLHttpRequest = function (this: XMLHttpRequest) {
  const xhr = new NativeXHR();
  let _method = "GET";
  let _url = "";
  let _contentType = "application/json";

  const origOpen = xhr.open.bind(xhr);
  const origSend = xhr.send.bind(xhr);
  const origSetHeader = xhr.setRequestHeader.bind(xhr);

  xhr.open = function (method: string, url: string | URL) {
    _method = method;
    _url = typeof url === "string" ? url : url.toString();
    _contentType = "application/json";
    origOpen.apply(xhr, arguments as unknown as Parameters<typeof origOpen>);
  };

  xhr.setRequestHeader = function (key: string, value: string) {
    if (key.toLowerCase() === "content-type") _contentType = value;
    origSetHeader(key, value);
  };

  xhr.send = function (body?: Document | XMLHttpRequestBodyInit | null) {
    // Intercept POST /render/{script}
    const renderMatch = _url.match(/^\/render\/(.+)$/);
    if (renderMatch && _method.toUpperCase() === "POST") {
      invoke("render", {
        script: renderMatch[1],
        body: body || "{}",
        contentType: _contentType,
      })
        .then((text) => {
          Object.defineProperty(xhr, "status", { get: () => 200, configurable: true });
          Object.defineProperty(xhr, "responseText", { get: () => text, configurable: true });
          Object.defineProperty(xhr, "response", { get: () => text, configurable: true });
          Object.defineProperty(xhr, "readyState", { get: () => 4, configurable: true });
          xhr.dispatchEvent(new Event("readystatechange"));
          xhr.dispatchEvent(new Event("load"));
          xhr.dispatchEvent(new Event("loadend"));
        })
        .catch(() => {
          xhr.dispatchEvent(new Event("error"));
        });
      return;
    }

    // Everything else — use native XHR
    origSend(body);
  };

  return xhr;
} as unknown as typeof XMLHttpRequest;

// --- Streaming helper ---

function streamViaChannel(command: string, args: Record<string, unknown>): Promise<Response> {
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
        } catch {}
        controller.enqueue(encoder.encode("data:" + data + "\n\n"));
      };

      args.onEvent = channel;

      invoke(command, args).catch((err) => {
        controller.error(err);
      });
    },
  });

  return Promise.resolve(
    new Response(stream, {
      status: 200,
      headers: { "Content-Type": "text/event-stream" },
    })
  );
}
