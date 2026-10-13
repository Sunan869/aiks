/** Independent, privacy-safe health checks; failures never expose raw exception text. */
export type DiagnosticProbeName = "doctor" | "ai" | "workbench";
export type DiagnosticProbeResult<T> = { status: "ok"; value: T } | { status: "error" };

export interface DiagnosticProbeSources<Doctor, Ai, Workbench> {
  doctor: () => Promise<Doctor>;
  ai: () => Promise<Ai>;
  workbench: () => Promise<Workbench>;
}

export interface DiagnosticProbeSnapshot<Doctor, Ai, Workbench> {
  doctor: DiagnosticProbeResult<Doctor>;
  ai: DiagnosticProbeResult<Ai>;
  workbench: DiagnosticProbeResult<Workbench>;
  failures: DiagnosticProbeName[];
}

function discardError<T>(outcome: PromiseSettledResult<T>): DiagnosticProbeResult<T> {
  return outcome.status === "fulfilled"
    ? { status: "ok", value: outcome.value }
    : { status: "error" };
}

/** A failed or synchronously throwing check must not hide results from other checks. */
export async function collectDiagnostics<Doctor, Ai, Workbench>(
  sources: DiagnosticProbeSources<Doctor, Ai, Workbench>,
): Promise<DiagnosticProbeSnapshot<Doctor, Ai, Workbench>> {
  const [doctorResult, aiResult, workbenchResult] = await Promise.allSettled([
    Promise.resolve().then(sources.doctor),
    Promise.resolve().then(sources.ai),
    Promise.resolve().then(sources.workbench),
  ] as const);
  const doctor = discardError(doctorResult);
  const ai = discardError(aiResult);
  const workbench = discardError(workbenchResult);
  const failures: DiagnosticProbeName[] = [];
  if (doctor.status === "error") failures.push("doctor");
  if (ai.status === "error") failures.push("ai");
  if (workbench.status === "error") failures.push("workbench");
  return { doctor, ai, workbench, failures };
}
