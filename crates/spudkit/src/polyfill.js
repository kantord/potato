(function () {
  // In Tauri the page runs under a custom scheme with no TCP server, so
  // WebSocket connections to /_api/stdin-ws are not possible.  Detect Tauri
  // early and skip the WebSocket path entirely; MSW (loaded as an
  // initialization_script) will intercept the native fetch call instead.
  if (typeof window.__TAURI__ !== "undefined") return;

  let socket = null;
  let connecting = null;

  function connect() {
    if (socket && socket.readyState === WebSocket.OPEN) return Promise.resolve(socket);
    if (connecting) return connecting;
    connecting = new Promise((resolve, reject) => {
      const proto = location.protocol === "https:" ? "wss:" : "ws:";
      const ws = new WebSocket(`${proto}//${location.host}/_api/stdin-ws`);
      ws.onopen = () => { socket = ws; connecting = null; resolve(ws); };
      ws.onclose = () => { socket = null; connecting = null; };
      ws.onerror = () => { connecting = null; reject(new Error("stdin-ws failed")); };
    });
    return connecting;
  }

  // Open eagerly so the connection is ready before the first stdin send.
  connect();

  const _fetch = window.fetch.bind(window);
  window.fetch = function (resource, init) {
    const url = typeof resource === "string" ? resource : resource.url;
    const m = url.match(/\/_api\/calls\/([^/]+)\/stdin$/);
    if (!m) return _fetch(resource, init);
    let data;
    try { data = JSON.parse((init && init.body) || "{}").data; } catch { data = null; }
    connect().then((ws) => ws.send(JSON.stringify({ call_id: m[1], data })));
    return Promise.resolve(new Response(JSON.stringify({ ok: true }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    }));
  };
})();
