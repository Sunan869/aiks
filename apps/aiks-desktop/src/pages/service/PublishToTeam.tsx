import {useState} from "react";
import {buildImportRequest,teamApi,type TeamConnectionStatus} from "../../api/team";
import {button,primary,errorText} from "./shared";

export default function PublishToTeam({detail}:{detail:Record<string,unknown>}){
  const [open,setOpen]=useState(false);const [connections,setConnections]=useState<TeamConnectionStatus[]>([]);const [selected,setSelected]=useState("");
  const [error,setError]=useState<string|null>(null);const [saving,setSaving]=useState(false);const [done,setDone]=useState(false);
  async function show(){setError(null);setDone(false);try{const rows=(await teamApi.connections()).filter(v=>v.state==="signed_in");setConnections(rows);setSelected(rows[0]?.connection_id??"");setOpen(true);}catch(e){setError(errorText(e));setOpen(true);}}
  async function confirm(){const connection=connections.find(v=>v.connection_id===selected);if(!connection){setError("请先登录一个团队连接。");return;}let request;try{request=buildImportRequest(detail);}catch(e){setError(errorText(e));return;}setSaving(true);setError(null);try{await teamApi.importKnowledge(connection.connection_id,request);setDone(true);}catch(e){setError(errorText(e));}finally{setSaving(false);}}
  const selectedConnection=connections.find(v=>v.connection_id===selected);
  return <><button className={button} onClick={()=>void show()}>发布到公司</button>{open&&<div role="dialog" aria-modal="true" aria-label="发布到公司" className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/30 p-4"><section className="w-full max-w-lg rounded-xl bg-white p-5 shadow-xl">
    <h2 className="text-lg font-semibold">发布到公司空间</h2><p className="mt-2 text-sm leading-6 text-slate-600">将复制当前知识正文到所选团队空间。本地原件保留；复制后默认仍为私有知识，不会自动分享原始会话或附件。</p>
    {connections.length>0?<div className="mt-4 space-y-3"><label className="block text-sm">团队连接<select aria-label="选择团队连接" className="mt-1 w-full rounded-lg border px-3 py-2" value={selected} onChange={e=>setSelected(e.target.value)}>{connections.map(v=><option key={v.connection_id} value={v.connection_id}>{v.origin} · {v.display_name??"已登录账号"}</option>)}</select></label>{selectedConnection?.company_id&&<p className="text-xs text-slate-500">公司标识：{selectedConnection.company_id}</p>}<div className="rounded-lg bg-slate-50 p-3 text-sm"><p className="font-medium">{String(detail.title??"未命名知识")}</p><p className="mt-1 text-xs text-slate-500">正文 {typeof detail.content==="string"?new TextEncoder().encode(detail.content).length:0} 字节</p></div></div>:<p className="mt-4 text-sm text-amber-700">还没有已登录的团队连接，请先切换到“团队空间”完成登录。</p>}
    {error&&<p role="alert" className="mt-3 rounded-lg bg-red-50 p-3 text-sm text-red-700">{error}</p>}{done&&<p className="mt-3 rounded-lg bg-emerald-50 p-3 text-sm text-emerald-700">已复制到团队空间，当前仍为私有知识。</p>}
    <div className="mt-5 flex justify-end gap-2"><button className={button} onClick={()=>setOpen(false)}>取消</button><button className={primary} disabled={saving||done||!selected} onClick={()=>void confirm()}>{saving?"发布中…":"确认发布"}</button></div>
  </section></div>}</>;
}
