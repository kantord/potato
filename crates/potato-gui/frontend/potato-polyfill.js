(function () {
  var nativeFetch = window.fetch;
  var encoder = new TextEncoder();
  var Channel = window.__TAURI__.core.Channel;
  var invoke = window.__TAURI__.core.invoke;

  window.fetch = function (input, init) {
    var url = typeof input === "string" ? input : input.url;
    var method = (init && init.method) || "GET";

    // Intercept POST /calls — stream via Channel (returns SSE with started event)
    if (url === "/calls" && method === "POST") {
      return streamViaChannel("create_call", { body: init.body || "{}" });
    }

    // Intercept POST /calls/{id}/stdin — forward via command
    var stdinMatch = url.match(/^\/calls\/([^/]+)\/stdin$/);
    if (stdinMatch && method === "POST") {
      return invoke("send_call_stdin", {
        callId: stdinMatch[1],
        data: init.body || "{}",
      }).then(function (text) {
        return new Response(text, {
          status: 200,
          headers: { "Content-Type": "application/json" },
        });
      });
    }

    // Everything else — native fetch
    return nativeFetch.call(window, input, init);
  };

  function streamViaChannel(command, args) {
    var stream = new ReadableStream({
      start: function (controller) {
        var channel = new Channel();

        channel.onmessage = function (data) {
          try {
            var parsed = JSON.parse(data);
            if (parsed.event === "end") {
              controller.close();
              return;
            }
          } catch (e) {}
          controller.enqueue(encoder.encode("data:" + data + "\n\n"));
        };

        args.onEvent = channel;

        invoke(command, args).catch(function (err) {
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
})();
