export interface BcpBackend {
  exec(args: ReadonlyArray<string>): Promise<{ stdout: string; stderr: string; exitCode: number }>;
  available(): Promise<boolean>;
}
