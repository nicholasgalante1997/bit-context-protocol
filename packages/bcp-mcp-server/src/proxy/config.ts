export type ProxyConfig = {
  downstream: {
    command: string;
    args: ReadonlyArray<string>;
  };
  detection: {
    enabled: boolean;
    confidenceThreshold: number;
  };
  render: {
    mode: "xml" | "markdown" | "minimal";
    budget?: number;
  };
};

const VALID_MODES = ["xml", "markdown", "minimal"] as const;

export const parseProxyConfig = (argv: ReadonlyArray<string>): ProxyConfig => {
  let command = "";
  let args: Array<string> = [];
  let renderMode: "xml" | "markdown" | "minimal" = "xml";
  let threshold = 0.7;
  let budget: number | undefined;

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = argv[i + 1];

    switch (arg) {
      case "--downstream-command":
        if (next) command = next;
        i++;
        break;
      case "--downstream-args":
        if (next) args = next.split(",");
        i++;
        break;
      case "--render-mode":
        if (next && VALID_MODES.includes(next as typeof VALID_MODES[number])) {
          renderMode = next as typeof VALID_MODES[number];
        }
        i++;
        break;
      case "--detect-threshold":
        if (next) threshold = Number(next);
        i++;
        break;
      case "--budget":
        if (next) budget = Number(next);
        i++;
        break;
    }
  }

  const envMode = process.env["BCP_RENDER_MODE"];
  if (envMode && VALID_MODES.includes(envMode as typeof VALID_MODES[number])) {
    renderMode = envMode as typeof VALID_MODES[number];
  }

  const envThreshold = process.env["BCP_DETECT_THRESHOLD"];
  if (envThreshold) threshold = Number(envThreshold);

  return {
    downstream: { command, args },
    detection: { enabled: true, confidenceThreshold: threshold },
    render: { mode: renderMode, budget }
  };
};
