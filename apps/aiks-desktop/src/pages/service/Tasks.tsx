import {useEffect,useState} from "react";
import {serviceApi,statusText,type Upload,type Job,type ServiceStatus} from "../../api/service";
import {button,primary,errorText} from "./shared";

const deliveryLabel=(code:string):string=>{
  const labels:Record<string,string>={
    upstream_400:"WeKnora 拒绝请求（HTTP 400）",
    upstream_404:"WeKnora 资源不存在（HTTP 404）",
    upstream_4xx:"WeKnora 拒绝请求（其他 HTTP 4xx）",
    unauthorized:"服务端写入凭据无效（401）",
    forbidden:"服务端写入权限不足（403）",
    payload_too_large:"文档内容超过 WeKnora 限制",
    unprocessable:"WeKnora 无法处理提交内容",
    transport:"Collector 无法连接 WeKnora",
    upstream_5xx:"WeKnora 服务暂时异常",
  };
  return labels[code]??code;
};

export default function Tasks({ready,status}:{ready:boolean;status:ServiceStatus|null}){
  const [rows,setRows]=useState<Upload[]|null>(null);
  const [job,setJob]=useState<Job|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [retrying,setRetrying]=useState(false);
  const [message,setMessage]=useState<string|null>(null);
  const remote=status?.mode==="service_remote";
  const delivery=status?.weknora;
  const bySession=new Map((delivery?.recent??[]).map(item=>[
    `${item.source}:\u0000${item.external_session_id}`,item
  ] as const));

  useEffect(()=>{
    if(!ready){setRows(null);return;}
    let active=true,inflight=false;
    const refresh=async()=>{
      if(inflight)return;
      inflight=true;
      try{
        const records=await serviceApi.uploads();
        if(active){setRows(records);setError(null);}
      }catch(e){if(active)setError(errorText(e));}
      finally{inflight=false;}
    };
    void refresh();
    const timer=setInterval(refresh,5000);
    return()=>{active=false;clearInterval(timer);};
  },[ready]);

  useEffect(()=>{
    if(!job||!["RUNNING","PENDING"].includes(job.status))return;
    let active=true,inflight=false;
    const timer=setInterval(()=>{
      if(inflight)return;
      inflight=true;
      void serviceApi.job(job.job_id).then(next=>{if(active)setJob(next);})
        .catch(e=>{if(active)setError(errorText(e));})
        .finally(()=>{inflight=false;});
    },5000);
    return()=>{active=false;clearInterval(timer);};
  },[job?.job_id,job?.status]);

  async function retryFailed(){
    setRetrying(true);setError(null);setMessage(null);
    try{
      const count=await serviceApi.retryWeKnoraFailed();
      setMessage(count>0?`已将 ${count} 条 Collector 投递失败任务重新排队；稍后刷新查看结果。`:
        "当前知识库没有可重试的 Collector 投递失败任务。");
    }catch(e){setError(errorText(e));}
    finally{setRetrying(false);}
  }

  return <div className="mx-auto max-w-6xl space-y-5 p-6">
    <header><h1 className="text-2xl font-semibold">任务记录</h1>
      <p className="mt-2 text-sm text-slate-500">
        {remote?"分三段查看：本地上传 → Collector 接收 → WeKnora 创建知识。后续解析和向量化在 WeKnora 查看。":
        "已接收不等于已提炼。版本冲突时保留原快照，不自动覆盖服务端内容。"}
      </p>
    </header>
    {error&&<p role="alert" className="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</p>}
    {message&&<p role="status" className="rounded border border-blue-200 bg-blue-50 p-3 text-sm text-blue-700">{message}</p>}
    {remote&&<section className="rounded-xl border bg-white p-5">
      <h2 className="font-semibold">Collector → WeKnora 投递状态</h2>
      {!ready?<p className="mt-3 text-sm text-slate-500">请先登录并连接 Collector。</p>:
      !delivery?.available?<p role="status" className="mt-3 text-sm text-amber-700">
        暂时无法查询投递状态（{delivery?.error_code??"等待状态刷新"}），未知状态不会显示为成功。
      </p>:
      <><div className="mt-4 grid gap-3 sm:grid-cols-3">
        <div className="rounded-lg bg-green-50 p-4"><p className="text-sm text-slate-600">WeKnora 已返回知识 ID</p>
          <strong className="text-2xl text-green-800">{delivery.delivered??0}</strong></div>
        <div className="rounded-lg bg-blue-50 p-4"><p className="text-sm text-slate-600">等待投递或自动重试</p>
          <strong className="text-2xl text-blue-800">{delivery.pending??0}</strong></div>
        <div className="rounded-lg bg-red-50 p-4"><p className="text-sm text-slate-600">投递失败，需人工处理</p>
          <strong className="text-2xl text-red-800">{delivery.terminal??0}</strong></div>
      </div>
      <p className="mt-3 text-xs text-slate-500">成功数量只代表 WeKnora 已创建或更新知识，不代表知识解析、Embedding 或索引已完成。</p>
      {(delivery.terminal??0)>0&&<button className={`${primary} mt-4`} disabled={retrying} onClick={()=>void retryFailed()}>
        {retrying?"正在重新排队…":"重试前 10 条投递失败任务"}
      </button>}
      {(delivery.failures?.length??0)>0&&<div className="mt-4 space-y-2">
        <h3 className="text-sm font-medium">最近投递失败的 Session</h3>
        {delivery.failures!.map(item=><div key={`${item.source}:${item.external_session_id}`} className="rounded-md border border-red-100 p-3 text-sm">
          <p className="break-words font-medium">{item.title||item.external_session_id}</p>
          <p className="mt-1 text-xs text-slate-500">{item.source} · Session: <span className="break-all">{item.external_session_id}</span></p>
          <p className="mt-1 text-xs text-red-700">{deliveryLabel(item.error_code)} · 第 {item.revision} 版 · 尝试 {item.attempts} 次</p>
        </div>)}
      </div>}
      </>}
    </section>}
    <section className="rounded-xl border bg-white p-5"><h2 className="mb-3 font-semibold">本地 → Collector 上传记录（最近 100 条）</h2>
      {!ready?<p>Collector 尚未连接。</p>:
      rows===null?<p>正在读取任务…</p>:
      rows.length===0?<p className="text-sm text-slate-500">当前没有上传记录。可以从“采集与同步”选择来源。</p>:
      rows.map(row=>{
        const item=bySession.get(`${row.source}:\u0000${row.external_session_id}`);
        const current=item&&(!row.receipt||item.revision>=row.receipt.revision)?item:null;
        return <div key={row.id} className="flex flex-wrap items-center gap-4 border-b py-4 text-sm">
        <span className="font-medium">{row.source}</span>
        <span className="max-w-xs break-all text-xs text-slate-500" title={row.external_session_id}>Session: {row.external_session_id}</span>
        <span>{row.state==="acknowledged"?"Collector 已接收":row.error_code==="excluded"?"已排除，停止发送":
          row.state==="blocked"?"上传已暂停，需核对版本或授权":row.state==="inflight"?"正在上传":"等待上传或重试"}</span>
        {remote&&row.state==="acknowledged"&&<span className={
          current?.state==="delivered"?"text-green-700":
          current?.state==="failed"?"text-red-700":"text-amber-700"
        }>{
          !delivery?.available?"WeKnora 状态未知":
          current?.state==="delivered"?"WeKnora 知识已创建（解析状态另查）":
          current?.state==="pending"?"WeKnora 待投递或重试":
          current?.state==="failed"?`WeKnora 投递失败：${deliveryLabel(current.error_code??"unknown")}`:
          "当前版本的 WeKnora 状态不在最近 100 条窗口内"
        }</span>}
        {row.error_code&&row.error_code!=="excluded"&&<span className="text-red-600">{errorText(row.error_code)}</span>}
        {row.receipt&&<button className={button} onClick={()=>void serviceApi.job(row.receipt!.job_id).then(setJob).catch(e=>setError(errorText(e)))}>
          查看 Collector 内部任务
        </button>}
        <details className="text-xs text-slate-400"><summary>诊断详情</summary>
          <pre className="mt-2 max-h-48 overflow-auto">{JSON.stringify(row,null,2)}</pre>
        </details>
      </div>})}
    </section>
    {job&&<section role="status" className="rounded-xl border bg-white p-5">
      <h2 className="font-semibold">{remote&&job.status==="DONE"?"Collector 内部处理完成（不代表 WeKnora 解析完成）":
        statusText({receiptState:"accepted",jobState:job.status})}</h2>
      <p className="mt-2 text-sm text-slate-500">任务来源版本 {job.revision} · 当前来源版本 {job.current_revision}</p>
      {job.error_code&&<p className="mt-2 text-sm text-red-600">{errorText(job.error_code)}</p>}
    </section>}
  </div>;
}
