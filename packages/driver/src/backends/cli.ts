import { spawn } from 'node:child_process';
import type { BcpBackend } from '../backend.ts';

const DEFAULT_TIMEOUT_MS = 30_000;

const resolveBinary = (override?: string): string => {
  if (override) return override;
  return process.env['BCP_CLI_PATH'] ?? 'bcp';
};

const spawnProcess = (
  binary: string,
  args: ReadonlyArray<string>,
  timeoutMs: number
): Promise<{ stdout: string; stderr: string; exitCode: number }> => {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, [...args], {
      stdio: ['ignore', 'pipe', 'pipe']
    });

    const stdoutChunks: Buffer[] = [];
    const stderrChunks: Buffer[] = [];

    child.stdout.on('data', (chunk: Buffer) => stdoutChunks.push(chunk));
    child.stderr.on('data', (chunk: Buffer) => stderrChunks.push(chunk));

    const timer = setTimeout(() => {
      child.kill('SIGKILL');
      reject(new Error(`bcp CLI timed out after ${timeoutMs}ms`));
    }, timeoutMs);

    child.on('error', (err: Error) => {
      clearTimeout(timer);
      if ('code' in err && err.code === 'ENOENT') {
        reject(
          new Error(
            `bcp binary not found. Install from https://github.com/nicholasgasior/bit-context-protocol or set BCP_CLI_PATH.`
          )
        );
        return;
      }
      reject(err);
    });

    child.on('close', (code: number | null) => {
      clearTimeout(timer);
      resolve({
        stdout: Buffer.concat(stdoutChunks).toString('utf-8'),
        stderr: Buffer.concat(stderrChunks).toString('utf-8'),
        exitCode: code ?? 1
      });
    });
  });
};

export const createCliBackend = (options?: {
  binaryPath?: string;
  timeoutMs?: number;
}): BcpBackend => {
  const binary = resolveBinary(options?.binaryPath);
  const timeoutMs =
    options?.timeoutMs ??
    (Number(process.env['BCP_CLI_TIMEOUT_MS']) || DEFAULT_TIMEOUT_MS);

  return {
    async exec(args) {
      return spawnProcess(binary, args, timeoutMs);
    },

    async available() {
      try {
        const result = await spawnProcess(binary, ['--version'], 5_000);
        return result.exitCode === 0;
      } catch {
        return false;
      }
    }
  };
};
