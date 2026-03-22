(function () {
  var nativeFetch = window.fetch;
  var encoder = new TextEncoder();
  var Channel = window.__TAURI__.core.Channel;
  var invoke = window.__TAURI__.core.invoke;

  // --- Intercept fetch() ---

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

    // Intercept POST /render/{script}
    var renderMatch = url.match(/^\/render\/(.+)$/);
    if (renderMatch && method === "POST") {
      return invoke("render", { script: renderMatch[1], body: init.body || "{}" }).then(function (text) {
        return new Response(text, {
          status: 200,
          headers: { "Content-Type": "text/html" },
        });
      });
    }

    // Everything else — native fetch
    return nativeFetch.call(window, input, init);
  };

  // --- Intercept XMLHttpRequest (for HTMX 2.x which uses XHR) ---

  var NativeXHR = window.XMLHttpRequest;

  window.XMLHttpRequest = function () {
    var xhr = new NativeXHR();
    var _method = "GET";
    var _url = "";

    var origOpen = xhr.open.bind(xhr);
    var origSend = xhr.send.bind(xhr);
    var origSetHeader = xhr.setRequestHeader.bind(xhr);

    xhr.open = function (method, url) {
      _method = method;
      _url = url;
      origOpen.apply(xhr, arguments);
    };

    xhr.setRequestHeader = function (key, value) {
      origSetHeader(key, value);
    };

    xhr.send = function (body) {
      // Intercept POST /render/{script}
      var renderMatch = _url.match(/^\/render\/(.+)$/);
      if (renderMatch && _method.toUpperCase() === "POST") {
        invoke("render", { script: renderMatch[1], body: body || "{}" })
          .then(function (text) {
            Object.defineProperty(xhr, "status", { get: function () { return 200; } });
            Object.defineProperty(xhr, "responseText", { get: function () { return text; } });
            Object.defineProperty(xhr, "response", { get: function () { return text; } });
            Object.defineProperty(xhr, "readyState", { get: function () { return 4; } });
            xhr.dispatchEvent(new Event("readystatechange"));
            xhr.dispatchEvent(new Event("load"));
            xhr.dispatchEvent(new Event("loadend"));
          })
          .catch(function () {
            xhr.dispatchEvent(new Event("error"));
          });
        return;
      }

      // Everything else — use native XHR
      origSend(body);
    };

    return xhr;
  };

  // --- Streaming helper ---

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
