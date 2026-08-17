// The entire Node surface this plugin uses, declared by hand so `npm install`
// fetches the TypeScript compiler and nothing else — no `@types/node`, no SDK.
// If this list ever needs another entry, ask whether the plugin has started
// doing something the wire contract should have handed it instead.
//
// It got SHORTER in #3516: `process.env` is gone, because the candidate grant
// carries the root and the test invocation and there is no environment left to
// read. A plugin that tried would now fail to compile.
declare function require(id: string): any;
declare const process: {
  stdout: { write(chunk: string): boolean };
  stderr: { write(chunk: string): boolean };
  exitCode: number | undefined;
};
