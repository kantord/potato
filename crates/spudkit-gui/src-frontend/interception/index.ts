import { FetchInterceptor } from "@mswjs/interceptors/fetch";
import { XMLHttpRequestInterceptor } from "@mswjs/interceptors/XMLHttpRequest";
import { BatchInterceptor } from "@mswjs/interceptors";
import { forwardRequest } from "./forward";
import { streamRequest } from "./stream";

const STREAMING_PATHS = [/^\/__calls$/];
const FORWARD_PATHS = [/^\/__calls\//, /^\/__render\//];

function isStreamingRoute(method: string, path: string): boolean {
  return method === "POST" && STREAMING_PATHS.some((p) => p.test(path));
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

  if (isForwardRoute(path)) {
    controller.respondWith(await forwardRequest(request));
    return;
  }
});
