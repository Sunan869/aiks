import {useEffect,useState} from "react";
import {serviceApi,statusText,type Upload,type Job} from "../../api/service";
import {button,errorText} from "./shared";
export default function Tasks({ready}:{ready:boolean}){
  const [rows,setRows]=useState<Upload[]|null>(null);const [job,setJob]=useState<Job|null>(null);const [error,setError]=useState<string|null>(null);
  useEffect(()=>{if(!ready)return;let active=true,inflight=false;
    const refresh=async()=>{if(inflight)return;inflight=true;try{const rows=await serviceApi.uploads();if(active)setRows(rows);}catch(e){if(active)setError(errorText(e));}finally{inflight=false;}};
    void refresh();const timer=setInterval(refresh,3000);return()=>{active=false;clearInterval(timer);};
  },[ready]);
  useEffect(()=>{if(!job||!["RUNNING","PENDING"].includes(job.status))return;let active=true,inflight=false;
    const timer=setInterval(()=>{if(inflight)return;inflight=true;void serviceApi.job(job.job_id).then(next=>{if(active)setJob(next);}).catch(e=>{if(active)setError(errorText(e));}).finally(()=>{inflight=false;});},3000);
    return()=>{active=false;clearInterval(timer);};
  },[job?.job_id,job?.status]);
  return <div className="mx-auto max-w-6xl space-y-5 p-6"><header><h1 className="text-2xl font-semibold">任务记录</h1><p className="mt-2 text-sm text-slate-500">已接收不等于已提炼。版本冲突时保留原快照，不自动覆盖服务端内容。</p></header>
    {error&&<p role="alert" className="rounded border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</p>}
    <section className="rounded-xl border bg-white p-5">{!ready?<p>本机知识服务尚未连接。</p>:rows===null?<p>正在读取任务…</p>:rows.length===0?<p className="text-sm text-slate-500">当前没有上传记录。可以从“采集与同步”选择来源。</p>:rows.map(row=><div key={row.id} className="flex flex-wrap items-center gap-4 border-b py-4 text-sm"><span className="font-medium">{row.source}</span><span>{row.state==="acknowledged"?"已接收":row.error_code==="excluded"?"已排除，停止发送":row.state==="blocked"?"已暂停，需核对版本或授权":row.state==="inflight"?"正在发送":"等待发送或重试"}</span>{row.error_code&&row.error_code!=="excluded"&&<span className="text-red-600">{errorText(row.error_code)}</span>}{row.receipt&&<button className={button} onClick={()=>void serviceApi.job(row.receipt!.job_id).then(setJob).catch(e=>setError(errorText(e)))}>查看处理状态</button>}<details className="text-xs text-slate-400"><summary>诊断详情</summary><pre className="mt-2 max-h-48 overflow-auto">{JSON.stringify(row,null,2)}</pre></details></div>)}</section>
    {job&&<section role="status" className="rounded-xl border bg-white p-5"><h2 className="font-semibold">{statusText({receiptState:"accepted",jobState:job.status})}</h2><p className="mt-2 text-sm text-slate-500">任务来源版本 {job.revision} · 当前来源版本 {job.current_revision}</p>{job.error_code&&<p className="mt-2 text-sm text-red-600">{errorText(job.error_code)}</p>}</section>}
  </div>;
}
