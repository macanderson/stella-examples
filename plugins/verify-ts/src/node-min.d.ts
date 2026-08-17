// The entire Node surface this plugin uses, declared by hand so `npm install`
// fetches the TypeScript compiler and nothing else — no `@types/node`, no SDK.
// If this list ever needs a sixth entry, ask whether the plugin has started
// doing something the wire contract should have handed it instead.
declare function require(id: string): any;
declare const process: {
  env: Record<string, string | undefined>;
  stdout: { write(chunk: string): boolean };
  stderr: { write(chunk: string): boolean };
  exitCode: number | undefined;
};
