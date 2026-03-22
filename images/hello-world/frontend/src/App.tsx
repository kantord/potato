import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

async function runCommand(
  cmd: string[],
  stdin: unknown,
  onEvent: (event: string, data: unknown) => void
): Promise<void> {
  // Single request: POST /calls returns SSE stream
  const res = await fetch("/calls", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cmd }),
  });

  const reader = res.body?.getReader();
  if (!reader) return;

  const decoder = new TextDecoder();
  let buffer = "";
  let callId: string | null = null;

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";

    for (const line of lines) {
      if (!line.startsWith("data:")) continue;
      const json = line.slice(5).trim();
      if (!json) continue;
      try {
        const msg = JSON.parse(json);
        if (msg.event === "started" && msg.data?.call_id) {
          callId = msg.data.call_id;
          // Send stdin now that the process is running
          if (stdin !== undefined) {
            fetch(`/calls/${callId}/stdin`, {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ data: stdin }),
            });
          }
          continue;
        }
        onEvent(msg.event || "output", msg.data);
      } catch {
        // ignore malformed lines
      }
    }
  }
}

function App() {
  const [a, setA] = useState("0");
  const [b, setB] = useState("0");
  const [result, setResult] = useState("");

  async function calc(op: string) {
    setResult("Calculating...");
    await runCommand(["/calculate.sh"], { a: parseFloat(a), b: parseFloat(b), op }, (event, data) => {
      if (event === "error") {
        setResult("Error: " + JSON.stringify(data));
      } else if (event === "output") {
        const d = data as Record<string, number>;
        setResult(`Result: ${d.result}`);
      }
    });
  }

  async function benchmark() {
    setResult("Benchmarking...");
    const duration = 3000;
    const start = performance.now();
    let count = 0;
    while (performance.now() - start < duration) {
      await runCommand(
        ["/calculate.sh"],
        { a: 2, b: 3, op: "add" },
        () => {}
      );
      count++;
      setResult(`Benchmarking... ${count} calls`);
    }
    const elapsed = (performance.now() - start) / 1000;
    const rps = (count / elapsed).toFixed(1);
    const avg = ((elapsed / count) * 1000).toFixed(1);
    setResult(
      `${count} calls in ${elapsed.toFixed(1)}s — ${rps} req/s, ${avg}ms avg`
    );
  }

  return (
    <div className="min-h-screen flex flex-col items-center justify-center gap-6">
      <h1 className="text-5xl font-bold">Hello from Potato!</h1>
      <div className="flex gap-2 items-center">
        <Input
          type="number"
          value={a}
          onChange={(e) => setA(e.target.value)}
          className="w-24 text-center"
        />
        <Input
          type="number"
          value={b}
          onChange={(e) => setB(e.target.value)}
          className="w-24 text-center"
        />
      </div>
      <div className="flex gap-2">
        <Button onClick={() => calc("add")}>Add</Button>
        <Button onClick={() => calc("multiply")}>Multiply</Button>
        <Button variant="secondary" onClick={benchmark}>
          Benchmark
        </Button>
      </div>
      {result && <p className="text-2xl">{result}</p>}
    </div>
  );
}

export default App;
