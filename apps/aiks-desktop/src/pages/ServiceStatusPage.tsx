import {DataStorageSetupGate} from "../components/DataStorage";
const storageRequired=()=>{};
import {useEffect,useRef,useState} from "react";
import {serviceApi,statusText,type Job,type SearchResult,type ServiceStatus,type Upload} from "../api/service";

export default function ServiceStatusPage(){
  const [status,setStatus]=useState<ServiceStatus|null>(null);
  const [uploads,setUploads]=useState<Upload[]>([]);
  const [selected,setSelected]=useState<string[]>([]);
  const [exclusions,setExclusions]=useState("");
  const [busy,setBusy]=useState(false);
  const [query,setQuery]=useState("");
  const [searching,setSearching]=useState(false);
  const [result,setResult]=useState<SearchResult|null>(null);
  const [detail,setDetail]=useState<Record<string,unknown>|null>(null);
  const [rows,setRows]=useState<Record<string,unknown>[]>([]);
  const [rowType,setRowType]=useState<"session"|"knowledge">("session");
  const [job,setJob]=useState<Job|null>(null);
  const [error,setError]=useState<string|null>(null);
  const [report,setReport]=useState<Record<string,unknown>|null>(null);
  const searchSequence=useRef(0);
  const mounted=useRef(true);
  const jobId=job?.job_id;
  useEffect(()=>{
    mounted.current=true;let inFlight=false;
    const refresh=async()=>{
      if(inFlight)return;inFlight=true;
      try{
        const next=await serviceApi.status();if(!mounted.current)return;setStatus(next);
        if(next.phase==="ready"){
          const pending=await serviceApi.uploads();if(mounted.current)setUploads(pending);
        }
      }catch(e){if(mounted.current)setError(String(e));}finally{inFlight=false;}
    };
    void refresh();const timer=setInterval(refresh,3000);
    return()=>{mounted.current=false;searchSequence.current++;clearInterval(timer);};
  },[]);
  useEffect(()=>{
    if(!jobId)return;let cancelled=false;let inFlight=false;
    const refresh=async()=>{if(inFlight)return;inFlight=true;try{const next=await serviceApi.job(jobId);if(!cancelled)setJob(next);}catch(e){if(!cancelled)setError(String(e));}finally{inFlight=false;}};
    const timer=setInterval(refresh,3000);return()=>{cancelled=true;clearInterval(timer);};
  },[jobId]);
  const ready=status?.phase==="ready";
  async function collect(){
    setBusy(true);setError(null);setReport(null);
    try{setReport(await serviceApi.collect(selected,exclusions.split(/\r?\n/).map(s=>s.trim()).filter(Boolean)));setUploads(await serviceApi.uploads());}
    catch(e){setError(String(e));}finally{setBusy(false);}
  }
  async function search(){
    const sequence=++searchSequence.current;setSearching(true);setResult(null);setError(null);
    try{const value=await serviceApi.search(query);if(sequence===searchSequence.current)setResult(value);}
    catch(e){if(sequence===searchSequence.current)setError(String(e));}
    finally{if(sequence===searchSequence.current)setSearching(false);}
  }
  async function list(type:"session"|"knowledge"){
    setError(null);try{const data=type==="session"?await serviceApi.sessions():await serviceApi.knowledgeList();setRowType(type);setRows(data);}catch(e){setError(String(e));}
  }
  async function open(type:"session"|"knowledge",id:string){
    setError(null);setDetail(null);try{setDetail(await serviceApi.detail(type,id));}catch(e){setError(String(e));}
  }
  const button="rounded border px-3 py-2 text-sm disabled:opacity-40 hover:bg-gray-50";
  const card="rounded-xl border bg-white p-5";
  return <main className="min-h-screen bg-gray-50 p-6 text-gray-800 space-y-5">
    <DataStorageSetupGate onRequired={storageRequired}/><header className="flex items-center justify-between"><div><h1 className="text-2xl font-semibold">AIKS · 本地知识服务</h1><p className="text-sm text-gray-500 mt-1">独立服务模式 · 业务数据本机存储 · 思源通过内部适配器访问</p></div><span className="rounded-full border bg-white px-4 py-2 text-sm">{ready?"服务已连接":status?.phase??"正在启动"}</span></header>
    <div className="rounded-lg border border-blue-100 bg-blue-50 p-4 text-sm leading-6">这是独立的本地空间，旧个人知识库不会自动迁移或上传。模型配置位于数据目录的 <code>service-local/config/models.toml</code>，修改后重启。AI 提炼：{status?.capabilities?.ai_assist?"已启用":"未启用"}；语义搜索：{status?.capabilities?.semantic_search?"已启用":"未启用"}。AI 请求发送到你配置的模型服务；完全离线时请配置本机模型。未启用 AI 时仍可采集和检索会话。此页面不开放思源全库接口。</div>
    {(error||status?.error_code)&&<div role="alert" className="rounded-lg border border-red-200 bg-red-50 p-4">{error||status?.error_code}<p className="text-sm mt-1">服务未启动时检查终端与模型配置；请使用 scripts/dev.ps1 启动，不会自动回退到旧引擎。</p></div>}
    <section className={card}><h2 className="font-semibold">选择本地来源采集</h2><p className="mt-1 text-sm text-gray-500">只读取勾选来源，每个来源每批最多 100 条，再次采集继续下一批；只提交完整会话。上传前执行规则脱敏，但不能保证识别所有敏感内容。来源目录沿用原有配置。</p>
      <div className="my-4 flex flex-wrap gap-x-5 gap-y-3">{status?.providers?.map(p=><label key={p.key} className="flex items-center gap-2 text-sm"><input type="checkbox" disabled={!p.enabled||busy} checked={selected.includes(p.key)} onChange={e=>setSelected(values=>e.target.checked?[...values,p.key]:values.filter(v=>v!==p.key))}/>{p.display_name}{!p.enabled&&"（已禁用）"}</label>)}</div>
      <details className="mb-3 text-sm"><summary>排除指定会话</summary><textarea aria-label="排除会话 ID" className="mt-2 w-full rounded border p-2" rows={2} placeholder="每行填写一个来源会话 ID" value={exclusions} onChange={e=>setExclusions(e.target.value)}/></details>
      <button className={button} disabled={!ready||busy||selected.length===0} onClick={()=>void collect()}>{busy?"正在采集并写入队列…":"采集所选来源"}</button>
      {report&&<pre className="mt-3 max-h-40 overflow-auto whitespace-pre-wrap text-xs">{JSON.stringify(report,null,2)}</pre>}
    </section>
    <section className={card}><h2 className="font-semibold">搜索与阅读</h2><form className="mt-3 flex gap-2" onSubmit={e=>{e.preventDefault();void search();}}><input aria-label="搜索知识与会话" value={query} onChange={e=>setQuery(e.target.value)} className="flex-1 rounded border px-3 py-2" placeholder="搜索本机服务已接收的知识和会话"/><button className={button} disabled={!ready||!query.trim()||searching}>{searching?"搜索中…":"搜索"}</button></form>
      {result&&<div className="mt-3"><p className="text-sm text-gray-500">{result.hits.length===0?"未找到相关结果":`找到 ${result.hits.length} 条结果`}{result.degraded&&" · 部分检索能力暂不可用"}</p>{result.hits.map(hit=><button key={`${hit.corpus}:${hit.entity_id}`} className="mt-3 block w-full rounded border p-3 text-left" onClick={()=>void open(hit.corpus,hit.entity_id)}><strong>{hit.title||"未命名会话"}</strong><span className="ml-3 text-xs text-gray-500">{hit.corpus} · v{hit.revision}</span><p className="mt-1 line-clamp-3 text-sm">{hit.snippet}</p></button>)}</div>}
      <div className="mt-4 flex gap-2"><button className={button} disabled={!ready} onClick={()=>void list("session")}>已接收会话</button><button className={button} disabled={!ready} onClick={()=>void list("knowledge")}>已提炼知识</button></div>
      <div className="mt-3 space-y-2">{rows.map(row=><button key={String(row.id??row.session_id)} className="block text-sm text-blue-700" onClick={()=>void open(rowType,String(row.id??row.session_id))}>{String(row.title||"未命名")} · v{String(row.revision??"未知")}{row.stale===true?"（来源版本已过期）":""}</button>)}</div>
      {detail&&<div className="mt-4 rounded border bg-gray-50 p-4"><button className="float-right text-sm" onClick={()=>setDetail(null)}>关闭</button><h3 className="font-semibold">{String(detail.title??"会话详情")}</h3><pre className="mt-2 max-h-96 overflow-auto whitespace-pre-wrap break-words text-sm">{typeof detail.content==="string"?detail.content:JSON.stringify(detail,null,2)}</pre></div>}
    </section>
    <section className={card}><h2 className="font-semibold">上传回执与处理任务</h2><p className="text-sm text-gray-500 mt-1">已接收不代表已提炼。版本冲突会保留原快照并停止重传，不自动覆盖服务端内容。</p>
      {uploads.length===0&&<p className="mt-3 text-sm">当前没有上传记录。</p>}
      {uploads.map(upload=><div className="mt-3 flex flex-wrap items-center gap-3 border-t pt-3 text-sm" key={upload.id}><span>{upload.source}</span><span>{upload.state==="acknowledged"?"已接收":upload.state==="blocked"?"已暂停，需核对版本或授权":upload.state==="inflight"?"发送中":"待发送／等待重试"}</span><span>尝试 {upload.attempt} 次</span>{upload.error_code&&<span className="text-red-700">{upload.error_code}</span>}{upload.receipt&&<><span>v{upload.receipt.revision}</span><button className={button} onClick={()=>void serviceApi.job(upload.receipt!.job_id).then(setJob).catch(e=>setError(String(e)))}>查询处理状态</button><button className={button} onClick={()=>void serviceApi.receipt(upload.receipt!.receipt_id).then(v=>setDetail({...v})).catch(e=>setError(String(e)))}>核对服务端回执</button></>}</div>)}
      {job&&<div role="status" className="mt-4 rounded border p-3"><strong>{statusText({receiptState:"accepted",jobState:job.status})}</strong><p className="text-sm">任务版本 {job.revision} · 当前来源版本 {job.current_revision}{job.error_code&&` · ${job.error_code}`}</p></div>}
    </section>
  </main>;
}
