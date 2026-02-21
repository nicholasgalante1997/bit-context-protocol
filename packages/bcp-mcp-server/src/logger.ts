const LOG_LEVELS = { debug: 0, info: 1, warn: 2, error: 3 } as const;
type LogLevel = keyof typeof LOG_LEVELS;

const currentLevel = (): LogLevel => {
  const env = process.env["BCP_LOG_LEVEL"];
  if (env && env in LOG_LEVELS) return env as LogLevel;
  return "info";
};

export const log = (level: LogLevel, ...args: Array<unknown>): void => {
  if (LOG_LEVELS[level] < LOG_LEVELS[currentLevel()]) return;
  const timestamp = new Date().toISOString();
  console.error(`[bcp-mcp-server] [${level.toUpperCase()}] ${timestamp}`, ...args);
};
