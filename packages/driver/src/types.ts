export const OUTPUT_MODES = {
  xml: 'xml',
  markdown: 'markdown',
  minimal: 'minimal'
} as const;

export type OutputMode = (typeof OUTPUT_MODES)[keyof typeof OUTPUT_MODES];

export const MODEL_FAMILIES = {
  claude: 'claude',
  gpt: 'gpt',
  gemini: 'gemini',
  generic: 'generic'
} as const;

export type ModelFamily = (typeof MODEL_FAMILIES)[keyof typeof MODEL_FAMILIES];

export const VERBOSITIES = {
  full: 'full',
  summary: 'summary',
  adaptive: 'adaptive'
} as const;

export type Verbosity = (typeof VERBOSITIES)[keyof typeof VERBOSITIES];

export interface DriverConfig {
  mode?: OutputMode;
  targetModel?: ModelFamily;
  tokenBudget?: number;
  verbosity?: Verbosity;
  include?: string[];
}

export interface BcpBlock {
  index: number;
  type: string;
  size: number;
  summary?: string;
}

export interface InspectResult {
  blockCount: number;
  totalSize: number;
  blocks: BcpBlock[];
  raw: string;
}

export interface DecodeResult {
  text: string;
  tokenEstimate?: number;
}

export interface EncodeOptions {
  compressBlocks?: boolean;
  compressPayload?: boolean;
  dedup?: boolean;
}

export interface ValidateResult {
  valid: boolean;
  errors: string[];
  raw: string;
}

export interface BcpDriver {
  decode(filePath: string, config?: DriverConfig): Promise<DecodeResult>;
  encode(manifestPath: string, outputPath: string, options?: EncodeOptions): Promise<void>;
  inspect(filePath: string): Promise<InspectResult>;
  validate(filePath: string): Promise<ValidateResult>;
  available(): Promise<boolean>;
}

export interface CreateDriverOptions {
  backend?: 'cli' | 'wasm';
  binaryPath?: string;
  timeoutMs?: number;
}
