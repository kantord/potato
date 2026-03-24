export {};

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
