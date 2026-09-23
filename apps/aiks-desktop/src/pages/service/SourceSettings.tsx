import {useEffect,useRef,useState} from "react";
import {serviceApi,type LocalSessionPage,type LocalPreview,type ServiceStatus} from "../../api/service";
import {button,primary,errorText} from "./shared";
import {Conversation} from "./Reading";

export default function SourceSettings({status,sources,onSave,onComplete}:{status:ServiceStatus|null;sources:string[];onSave:(sources:string[])=>Promise<void>;onComplete?:()=>Promise<void>}){
  const [selected,setSelected]=useState(sources);const [busy,setBusy]=useState(false);
  const [error,setError]=useState<string|null>(null);const [notice,setNotice]=useState<string|null>(null);
  const [report,setReport]=useState<Record<string,unknown>|null>(null);
  const [source,setSource]=useState("");const [page,setPage]=useState<LocalSessionPage|null>(null);
  const [query,setQuery]=useState("");const [scanning,setScanning]=useState(false);
  const [preview,setPreview]=useState<LocalPreview|null>(null);const [ruleBusy,setRuleBusy]=useState(false);
  const sequence=useRef(0);const previewSequence=useRef(0);const mounted=useRef(true);
  useEffect(()=>{setSelected(sources);},[sources.join("|")]);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;sequence.current++;previewSequence.current++;};},[]);
  async function save(collect=false,finish=false){
    setBusy(true);setError(null);setNotice(null);
    try{await onSave(selected);if(collect){const result=await serviceApi.collect(selected);if(mounted.current)setReport(result);}if(finish&&onComplete)await onComplete();else if(mounted.current)setNotice(collect?"采集已提交到本机队列，处理状态可在“任务记录”查看。":"来源选择已保存；保存不会自动采集或发送会话。");}
    catch(e){if(mounted.current)setError(errorText(e));}finally{if(mounted.current)setBusy(false);}
  }
  function changeSource(next:string){sequence.current++;previewSequence.current++;setSource(next);setPage(null);setQuery("");setPreview(null);setError(null);setNotice(null);}
  async function scan(){
    const request=++sequence.current;setScanning(true);setPage(null);setPreview(null);setError(null);
    try{const result=await serviceApi.scanSessions(source);if(request===sequence.current)setPage(result);}
    catch(e){if(request===sequence.current)setError(errorText(e));}finally{if(request===sequence.current)setScanning(false);}
  }
  async function browse(offset=0){
    const request=++sequence.current;setScanning(true);setError(null);
    try{const result=await serviceApi.browseSessions(source,query,offset);if(request===sequence.current)setPage(result);}
    catch(e){if(request===sequence.current)setError(errorText(e));}finally{if(request===sequence.current)setScanning(false);}
  }
  async function open(key:string){
    const request=++previewSequence.current;setError(null);setPreview(null);
    try{const result=await serviceApi.previewSession(source,key);if(request===previewSequence.current)setPreview(result);}
    catch(e){if(request===previewSequence.current)setError(errorText(e));}
  }
  async function exclude(keys:string[],excluded:boolean){
    setRuleBusy(true);setError(null);setNotice(null);
    try{const result=await serviceApi.excludeSessions(source,keys,excluded);
      if(mounted.current){setNotice(`${excluded?"排除规则":"恢复同步规则"}已保存。${result.paused>0?`已暂停 ${result.paused} 条尚未发送的会话。`:""}${result.already_received>0?`其中 ${result.already_received} 条已被接收，已入库内容不会删除。`:""}`);await browse(page?.offset??0);}}
    catch(e){if(mounted.current)setError(errorText(e));}finally{if(mounted.current)setRuleBusy(false);}
  }
  const providers=status?.providers??[];
  const reports=Array.isArray(report?.sources)?report.sources as {source:string;report:Record<string,unknown>}[]:[];
  return <div className="mx-auto max-w-6xl space-y-6 p-6">
    <header><h1 className="text-2xl font-semibold">采集与同步</h1><p className="mt-2 text-sm leading-6 text-slate-500">设置要采集的本地 AI 工具。设置可稍后完成，不影响进入知识库；扫描与预览不会上传或调用模型。</p></header>
    {error&&<div role="alert" className="rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div>}
    {notice&&<div role="status" className="rounded-lg border border-blue-100 bg-blue-50 p-3 text-sm text-blue-800">{notice}</div>}
    <section className="rounded-xl border bg-white p-5"><h2 className="font-semibold">采集来源</h2><div className="my-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">{providers.map(p=><label key={p.key} className={`flex items-center gap-3 rounded-lg border p-3 text-sm ${selected.includes(p.key)?"border-blue-200 bg-blue-50/50":""}`}><input type="checkbox" disabled={!p.enabled||busy} checked={selected.includes(p.key)} onChange={e=>setSelected(v=>e.target.checked?[...v,p.key]:v.filter(s=>s!==p.key))}/><span>{p.display_name}{!p.enabled&&<span className="ml-2 text-xs text-slate-400">已禁用</span>}</span></label>)}</div>
      <p className="mb-4 text-xs leading-6 text-slate-500">目录沿用本机来源配置。采集时只提交完整会话，执行规则脱敏；自动规则不能保证移除所有敏感内容。每次每个来源最多处理 100 条，再次采集继续下一批。</p>
      <div className="flex flex-wrap gap-3"><button className={button} disabled={busy} onClick={()=>void save()}>保存来源选择</button><button className={primary} disabled={status?.phase!=="ready"||busy||!selected.length} onClick={()=>void save(true)}>{busy?"正在处理…":"采集所选来源"}</button>{onComplete&&<button className={button} disabled={busy} onClick={()=>void save(false,true)}>保存并进入知识库</button>}</div>
      {reports.length>0&&<div className="mt-5 overflow-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b text-slate-500">{["来源","发现","待发送","未变化","已排除","稍后处理","失败"].map(h=><th key={h} className="py-2 pr-3 font-medium">{h}</th>)}</tr></thead><tbody>{reports.map(r=><tr key={r.source} className="border-b"><td className="py-3">{providers.find(p=>p.key===r.source)?.display_name??r.source}</td>{["discovered","queued","unchanged","excluded","deferred","failed"].map(k=><td key={k}>{String(r.report[k]??"—")}</td>)}</tr>)}</tbody></table></div>}
    </section>
    <section className="rounded-xl border bg-white p-5"><h2 className="font-semibold">管理同步会话 <span className="ml-2 text-xs font-normal text-slate-400">可选</span></h2><p className="my-2 text-sm leading-6 text-slate-500">先扫描列表，再按标题、项目和时间识别会话。勾选“排除同步”立即保存，重启后仍有效；取消勾选恢复后续同步。已入库内容不会因此删除。</p>
      <div className="my-4 flex flex-wrap gap-3"><select aria-label="扫描来源" className="rounded-lg border px-3 py-2 text-sm" value={source} disabled={scanning||ruleBusy} onChange={e=>changeSource(e.target.value)}><option value="">选择要查看的来源</option>{providers.filter(p=>p.enabled).map(p=><option key={p.key} value={p.key}>{p.display_name}</option>)}</select><button className={button} disabled={!source||scanning||ruleBusy||busy} onClick={()=>void scan()}>{scanning?"正在读取列表…":"扫描本地会话"}</button></div>
      {!page&&!scanning&&<p className="rounded-lg bg-slate-50 p-4 text-sm text-slate-500">尚未扫描。扫描只读取本机会话，不会加入发送队列。</p>}
      {page&&<>
        {!page.complete&&<p className="mb-3 rounded-lg bg-amber-50 p-3 text-sm text-amber-800">扫描未完整完成，当前显示可读取的部分。未显示的会话不会被自动排除。</p>}
        {!page.can_save&&<p className="mb-3 text-sm text-amber-700">当前可预览列表；连接本机知识服务后才能保存目标空间的排除规则。</p>}
        <form className="mb-3 flex gap-2" onSubmit={e=>{e.preventDefault();void browse();}}><input aria-label="筛选本地会话" placeholder="搜索会话标题或项目" className="min-w-0 flex-1 rounded-lg border px-3 py-2 text-sm" value={query} onChange={e=>setQuery(e.target.value)}/><button className={button} disabled={scanning||ruleBusy}>筛选</button></form>
        <div className="mb-3 flex flex-wrap items-center gap-3 text-xs text-slate-500"><span>找到 {page.total} 条，当前第 {Math.floor(page.offset/page.limit)+1} 页</span><button className={button} disabled={!page.can_save||ruleBusy||scanning||!page.items.length} onClick={()=>void exclude(page.items.map(r=>r.key),true)}>排除本页</button><button className={button} disabled={!page.can_save||ruleBusy||scanning||!page.items.length} onClick={()=>void exclude(page.items.map(r=>r.key),false)}>恢复本页同步</button></div>
        <div className="overflow-auto"><table className="w-full text-left text-sm"><thead><tr className="border-b text-xs text-slate-500"><th className="w-28 py-3">排除同步</th><th>会话标题</th><th className="px-4">项目</th><th>最近更新</th><th>消息数</th><th/></tr></thead><tbody>{page.items.map(row=><tr key={row.key} className={`border-b ${row.excluded?"bg-slate-50 text-slate-500":""}`}><td className="py-4"><label className="flex items-center gap-2"><input aria-label={`排除同步：${row.title}`} type="checkbox" checked={row.excluded} disabled={!page.can_save||ruleBusy||scanning||busy} onChange={e=>void exclude([row.key],e.target.checked)}/>{row.excluded?"已排除":"同步"}</label></td><td className="max-w-md py-3 font-medium">{row.title}</td><td className="px-4">{row.project||"—"}</td><td className="whitespace-nowrap text-xs">{row.updated_at?new Date(row.updated_at).toLocaleString("zh-CN"):"—"}</td><td className="px-3">{row.message_count||"—"}</td><td><button className={button} disabled={busy||scanning} onClick={()=>void open(row.key)}>预览</button></td></tr>)}</tbody></table>{!page.items.length&&<p className="py-5 text-sm text-slate-500">没有匹配的会话。</p>}</div>
        <div className="mt-4 flex justify-between"><button className={button} disabled={!page.offset||scanning||ruleBusy} onClick={()=>void browse(Math.max(0,page.offset-page.limit))}>上一页</button><button className={button} disabled={page.offset+page.limit>=page.total||scanning||ruleBusy} onClick={()=>void browse(page.offset+page.limit)}>下一页</button></div>
      </>}
      {preview&&<aside className="mt-5 rounded-xl border bg-slate-50 p-5"><button className={`${button} float-right`} onClick={()=>{previewSequence.current++;setPreview(null);}}>关闭预览</button><h3 className="mb-2 font-semibold">{preview.title}</h3><p className="mb-4 text-xs text-slate-500">本地预览，未上传。{preview.truncated&&"长会话仅展示前一部分。"}</p><div className="max-h-96 overflow-auto"><Conversation messages={preview.messages}/></div></aside>}
    </section>
  </div>;
}
