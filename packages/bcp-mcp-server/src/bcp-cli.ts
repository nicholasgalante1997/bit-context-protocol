import { log } from "./logger.ts";

const DEFAULT_TIMEOUT_MS = 30_000;

export const resolveBcpBinary = (): string => {
  return process.env["BCP_CLI_PATH"] ?? "bcp";
};

export const runBcpCli = async (
  args: ReadonlyArray<string>
): Promise<{ stdout: string; stderr: string; exitCode: number }> => {
  const binary = resolveBcpBinary();
  const timeoutMs = Number(process.env["BCP_CLI_TIMEOUT_MS"]) || DEFAULT_TIMEOUT_MS;

  log("debug", `Running: ${binary} ${args.join(" ")}`);

  const proc = Bun.spawn([binary, ...args], {
    stdout: "pipe",
    stderr: "pipe"
  });

  const timeout = new Promise<never>((_, reject) => {
    setTimeout(() => {
      proc.kill();
      reject(new Error(`bcp CLI timed out after ${timeoutMs}ms`));
    }, timeoutMs);
  });

  try {
    const [stdout, stderr, exitCode] = await Promise.race([
      Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
        proc.exited
      ]),
      timeout.then(() => { throw new Error("timeout"); })
    ]);

    return { stdout, stderr, exitCode };
  } catch (err) {
    return {
      stdout: "",
      stderr: err instanceof Error ? err.message : "Unknown error",
      exitCode: 1
    };
  }
};

export const validateBcpAvailable = async (): Promise<boolean> => {
  try {
    const result = await runBcpCli(["--version"]);
    return result.exitCode === 0;
  } catch {
    return false;
  }
};
