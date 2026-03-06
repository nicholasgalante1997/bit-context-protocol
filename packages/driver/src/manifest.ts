export const PRIORITIES = {
  critical: 'critical',
  high: 'high',
  normal: 'normal',
  low: 'low',
  background: 'background'
} as const;

export type Priority = (typeof PRIORITIES)[keyof typeof PRIORITIES];

export const ROLES = {
  system: 'system',
  user: 'user',
  assistant: 'assistant',
  tool: 'tool'
} as const;

export type Role = (typeof ROLES)[keyof typeof ROLES];

export const STATUSES = {
  ok: 'ok',
  error: 'error'
} as const;

export type Status = (typeof STATUSES)[keyof typeof STATUSES];

// --- Block definitions matching the BCP JSON manifest schema ---

interface BlockBase {
  priority?: Priority;
  summary?: string;
}

export interface CodeBlock extends BlockBase {
  type: 'code';
  lang: string;
  path: string;
  content: string;
}

export interface ConversationBlock extends BlockBase {
  type: 'conversation';
  role: Role;
  content: string;
}

export interface FileTreeEntry {
  name: string;
  kind: 'file' | 'dir';
  size?: number;
  children?: FileTreeEntry[];
}

export interface FileTreeBlock extends BlockBase {
  type: 'file_tree';
  root: string;
  entries: FileTreeEntry[];
}

export interface ToolResultBlock extends BlockBase {
  type: 'tool_result';
  name: string;
  status?: Status;
  content: string;
}

export interface DocumentBlock extends BlockBase {
  type: 'document';
  title: string;
  content: string;
  format?: string;
}

export interface StructuredDataBlock extends BlockBase {
  type: 'structured_data';
  format: string;
  content: string;
}

export interface DiffHunk {
  old_start: number;
  new_start: number;
  lines: string;
}

export interface DiffBlock extends BlockBase {
  type: 'diff';
  path: string;
  hunks: DiffHunk[];
}

export interface AnnotationBlock extends BlockBase {
  type: 'annotation';
  target_block_id: number;
  kind: string;
  value: string;
}

export interface ManifestBlock {
  type: string;
  [key: string]: unknown;
}

export interface Manifest {
  description?: string;
  blocks: ManifestBlock[];
}
