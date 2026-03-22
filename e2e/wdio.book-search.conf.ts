import type { Options } from "@wdio/types";
import { spawn, type ChildProcess } from "child_process";
import * as path from "path";

const rootDir = path.resolve(__dirname, "..");

let tauriDriver: ChildProcess;
let potatoServer: ChildProcess;

export const config: Options.Testrunner = {
  autoCompileOpts: {
    tsNodeOpts: { project: "./tsconfig.json" },
  },
  specs: ["./specs/book-search.ts"],
  maxInstances: 1,

  hostname: "127.0.0.1",
  port: 4444,

  capabilities: [
    {
      "alwaysMatch": {
        "tauri:options": {
          application: path.join(rootDir, "target/debug/potato-app"),
          args: ["potato-book-search"],
        },
      },
      "firstMatch": [{}],
    } as any,
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 30000,
  },

  onPrepare() {
    potatoServer = spawn(
      path.join(rootDir, "target/debug/potato-server"),
      [],
      {
        stdio: "pipe",
        env: { ...process.env, WEBKIT_DISABLE_DMABUF_RENDERER: "1" },
      }
    );

    tauriDriver = spawn("tauri-driver", ["--port", "4444"], {
      stdio: "pipe",
    });

    return new Promise<void>((resolve) => setTimeout(resolve, 5000));
  },

  onComplete() {
    tauriDriver?.kill();
    potatoServer?.kill();
  },
};
