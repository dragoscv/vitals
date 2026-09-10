/**
 * How the webview asks Rust for a verdict.
 *
 * Request/response rather than a subscription: the question is asked when a
 * button is pressed, and the answer is worth computing only then. The webview
 * supplies the frame it already holds so Rust does not clone six hundred
 * processes per tick on the off chance someone asks.
 */

import { invoke } from '@tauri-apps/api/core';

import type { Diagnosis, Process, SystemMetrics } from '@vitals/protocol';

export interface DiagnosisSource {
  diagnose(system: SystemMetrics, processes: readonly Process[]): Promise<Diagnosis>;
}

export function createTauriDiagnosisSource(): DiagnosisSource {
  return {
    diagnose: (system, processes) => invoke<Diagnosis>('diagnose', { system, processes }),
  };
}
