export const button="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-40";
export const primary="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-40";
export function errorText(error:unknown):string{
  const code=error instanceof Error?error.message:String(error);
  const labels:Record<string,string>={collector_busy:"正在采集或发送，请稍后重试。",previous_upload_pending:"这条会话正在发送，请在发送结束后重试。",scan_required:"会话列表已更新，请重新扫描后选择。",not_found:"内容已不存在或列表已过期，请刷新。",invalid_input:"无法完整读取所选会话，请检查来源文件或重新扫描。",content_unavailable:"思源正文暂不可用，请稍后重试。",retryable:"暂时无法连接本机知识服务，请稍后重试。",service_not_ready:"本机知识服务尚未就绪。",collector_storage_unavailable:"无法读取本地采集设置，请检查目录权限或其他 AIKS 进程。",scan_timeout:"扫描用时较长，请减少来源范围后重试。",conflict:"内容版本已变化，请刷新后重试。",too_large:"选择或内容超出本次处理范围，请分批操作。"};
  return labels[code]??`操作未完成（${code}），请检查服务状态后重试。`;
}
