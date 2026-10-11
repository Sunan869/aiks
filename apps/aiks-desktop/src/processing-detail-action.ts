import type { AiksApi } from "./api";
import type { PipelineSummary } from "./api/types";

/** Old Pipeline rows are historical; final eligibility MUST be checked by Rust.
 * Never enqueue a failed pipeline through the legacy first-run command.
 */
export async function submitProcessingDetailAction(
  api: Pick<AiksApi, "retryFailedAiTask" | "runPipelineForSession">,
  detail: Pick<PipelineSummary, "session_id" | "status">,
): Promise<"retry" | "first_run"> {
  if (!Number.isSafeInteger(detail.session_id) || detail.session_id <= 0) {
    throw new Error("无效的原始 Session 身份，无法提交 AI 任务");
  }
  if (detail.status === "FAILED") {
    await api.retryFailedAiTask(detail.session_id);
    return "retry";
  }
  if (detail.status === "RAW_ONLY") {
    await api.runPipelineForSession(detail.session_id);
    return "first_run";
  }
  throw new Error("当前状态不是可提交的失败任务或未处理原文");
}
