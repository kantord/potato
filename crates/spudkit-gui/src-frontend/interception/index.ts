import { FetchInterceptor } from "@mswjs/interceptors/fetch";
import { XMLHttpRequestInterceptor } from "@mswjs/interceptors/XMLHttpRequest";
import { BatchInterceptor } from "@mswjs/interceptors";
import { forwardRequest } from "./forward";
import { streamRequest } from "./stream";
import { sendStdinRequest } from "./stdin";

const STREAMING_PATHS = [/^\/_api\/calls$/];
const STDIN_PATHS = [/^\/_api\/calls\/[^/]+\/stdin$/];
const FORWARD_PATHS = [/^\/_api\/calls\//, /^\/_api\/render\//];

function isStreamingRoute(method: string, path: string): boolean {
  return method === "POST" && STREAMING_PATHS.some((p) => p.test(path));
}

function isStdinRoute(path: string): boolean {
  return STDIN_PATHS.some((p) => p.test(path));
}

function isForwardRoute(path: string): boolean {
  return FORWARD_PATHS.some((p) => p.test(path));
}

const interceptor = new BatchInterceptor({
  name: "spudkit",
  interceptors: [new FetchInterceptor(), new XMLHttpRequestInterceptor()],
});

interceptor.apply();

interceptor.on("request", async ({ request, controller }) => {
  const path = new URL(request.url).pathname;

  if (isStreamingRoute(request.method, path)) {
    const body = await request.text();
    controller.respondWith(streamRequest(request, body));
    return;
  }

  if (isStdinRoute(path)) {
    controller.respondWith(await sendStdinRequest(request));
    return;
  }

  if (isForwardRoute(path)) {
    controller.respondWith(await forwardRequest(request));
    return;
  }
});
