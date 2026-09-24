import {useEffect,useRef,useState} from "react";
import {teamApi,type DirectoryEntry,type ShareGrant,type ShareState,type TeamApi} from "../../api/team";
import {button,primary,errorText} from "./shared";

export default function ShareDialog({connectionId,knowledgeId,onClose,api=teamApi}:{connectionId:string;knowledgeId:string;onClose:()=>void;api?:TeamApi}){
  const [state,setState]=useState<ShareState|null>(null);const [query,setQuery]=useState("");const [results,setResults]=useState<DirectoryEntry[]>([]);
  const [labels,setLabels]=useState<Record<string,string>>({});const [descendants,setDescendants]=useState<Record<string,boolean>>({});
  const [error,setError]=useState<string|null>(null);const [saving,setSaving]=useState(false);const searchSeq=useRef(0);
  useEffect(()=>{let active=true;void api.shares(connectionId,knowledgeId).then(value=>{if(active)setState(value);}).catch(e=>{if(active)setError(errorText(e));});return()=>{active=false;searchSeq.current++;};},[api,connectionId,knowledgeId]);
  async function search(){if(!query.trim())return;const seq=++searchSeq.current;setError(null);try{const items=await api.directorySearch(connectionId,query.trim());if(seq===searchSeq.current)setResults(items);}catch(e){if(seq===searchSeq.current)setError(errorText(e));}}
  function add(entry:DirectoryEntry){if(!state)return;const next:ShareGrant={target_type:entry.target_type,target_id:entry.target_id,include_descendants:entry.target_type==="org"?(descendants[entry.target_id]??false):false,permission:"read"};if(state.grants.some(v=>v.target_type===next.target_type&&v.target_id===next.target_id))return;setLabels(v=>({...v,[entry.target_id]:entry.display_name}));setState({...state,grants:[...state.grants,next]});}
  function remove(index:number){if(state)setState({...state,grants:state.grants.filter((_,i)=>i!==index)});}
  async function save(){if(!state)return;setSaving(true);setError(null);try{const version=await api.replaceShares(connectionId,knowledgeId,state.grant_version,state.grants);setState({...state,grant_version:version});onClose();}catch(e){setError(errorText(e));}finally{setSaving(false);}}
  return <div role="dialog" aria-modal="true" aria-label="知识分享" className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/30 p-4"><section className="max-h-[80vh] w-full max-w-2xl overflow-auto rounded-xl bg-white p-5 shadow-xl">
    <div className="flex items-center justify-between gap-3"><div><h2 className="text-lg font-semibold">分享知识</h2><p className="mt-1 text-xs text-slate-500">首版仅支持只读分享；默认不包含子部门。</p></div><button className={button} onClick={onClose}>关闭</button></div>
    {error&&<p role="alert" className="mt-3 rounded-lg bg-red-50 p-3 text-sm text-red-700">{error}</p>}
    {!state?<p className="mt-4 text-sm text-slate-500">正在读取分享设置…</p>:<>
      <div className="mt-5 flex gap-2"><input aria-label="搜索人员或部门" className="min-w-0 flex-1 rounded-lg border px-3 py-2 text-sm" value={query} onChange={e=>setQuery(e.target.value)} placeholder="输入姓名或部门"/><button className={button} onClick={()=>void search()}>搜索</button></div>
      {results.length>0&&<div className="mt-3 divide-y rounded-lg border">{results.map(entry=><div key={entry.target_type+entry.target_id} className="flex items-center gap-3 p-3 text-sm"><div className="min-w-0 flex-1"><p className="truncate font-medium">{entry.display_name}</p><p className="text-xs text-slate-400">{entry.target_type==="user"?"人员":"部门"}</p></div>{entry.target_type==="org"&&<label className="flex items-center gap-1 text-xs text-slate-500"><input type="checkbox" checked={descendants[entry.target_id]??false} onChange={e=>setDescendants(v=>({...v,[entry.target_id]:e.target.checked}))}/>含子部门</label>}<button className={button} onClick={()=>add(entry)}>添加</button></div>)}</div>}
      <div className="mt-5"><h3 className="text-sm font-semibold">当前授权</h3>{state.grants.length===0?<p className="mt-2 text-sm text-slate-500">当前为私有知识。</p>:<div className="mt-2 divide-y rounded-lg border">{state.grants.map((grant,index)=><div key={grant.target_type+grant.target_id} className="flex items-center gap-3 p-3 text-sm"><div className="min-w-0 flex-1"><p className="truncate">{labels[grant.target_id]??grant.target_id}</p><p className="text-xs text-slate-400">{grant.target_type==="user"?"人员只读":grant.include_descendants?"部门及子部门只读":"部门直接成员只读"}</p></div><button className={button} onClick={()=>remove(index)}>移除</button></div>)}</div>}</div>
      <div className="mt-5 flex justify-end gap-2"><button className={button} onClick={onClose}>取消</button><button className={primary} disabled={saving} onClick={()=>void save()}>{saving?"保存中…":"保存分享"}</button></div>
    </>}
  </section></div>;
}
