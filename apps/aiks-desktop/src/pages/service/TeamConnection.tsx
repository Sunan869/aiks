import {useEffect,useRef,useState} from "react";
import {Building2,LogIn,LogOut,RefreshCw,Users} from "lucide-react";
import {teamApi,TeamRequestGate,type ShareSource,type TeamConnectionStatus,type TeamKnowledgeDetail,type TeamKnowledgeRow} from "../../api/team";
import {KnowledgeReading} from "./Reading";
import ShareDialog from "./ShareDialog";
import {button,primary,errorText} from "./shared";

const labels:Record<ShareSource,string>={mine:"我的",shared_to_me:"分享给我",department:"部门共享"};

export default function TeamConnection(){
  const [connections,setConnections]=useState<TeamConnectionStatus[]>([]);const [selected,setSelected]=useState("");const [origin,setOrigin]=useState("");
  const [rows,setRows]=useState<TeamKnowledgeRow[]>([]);const [detail,setDetail]=useState<TeamKnowledgeDetail|null>(null);const [filter,setFilter]=useState<ShareSource>("mine");
  const [sharing,setSharing]=useState(false);const [message,setMessage]=useState<string|null>(null);const [error,setError]=useState<string|null>(null);const [loading,setLoading]=useState(false);
  const gate=useRef(new TeamRequestGate());
  async function loadConnections(){try{const values=await teamApi.connections();setConnections(values);setSelected(current=>current&&values.some(v=>v.connection_id===current)?current:(values.find(v=>v.state==="signed_in")??values[0])?.connection_id??"");}catch(e){setError(errorText(e));}}
  useEffect(()=>{void loadConnections();},[]);
  useEffect(()=>{gate.current.select(selected||null);setDetail(null);setSharing(false);if(!selected){setRows([]);return;}const connection=connections.find(v=>v.connection_id===selected);if(connection?.state!=="signed_in"){setRows([]);return;}const ticket=gate.current.begin();if(!ticket)return;setLoading(true);void teamApi.knowledgeList(selected).then(value=>{if(gate.current.accepts(ticket))setRows(value);}).catch(e=>{if(gate.current.accepts(ticket))setError(errorText(e));}).finally(()=>{if(gate.current.accepts(ticket))setLoading(false);});},[selected,connections]);
  useEffect(()=>()=>gate.current.select(null),[]);
  const connection=connections.find(v=>v.connection_id===selected);
  async function add(){if(!origin.trim())return;setError(null);try{const created=await teamApi.addConnection(origin.trim());setOrigin("");await loadConnections();setSelected(created.connection_id);}catch(e){setError(errorText(e));}}
  async function begin(){if(!connection)return;setError(null);try{await teamApi.beginLogin(connection.connection_id);setMessage("已在系统浏览器打开钉钉登录。完成授权后返回这里点击“完成登录”。");}catch(e){setError(errorText(e));}}
  async function finish(){if(!connection)return;setError(null);try{await teamApi.finishLogin(connection.connection_id);setMessage(null);await loadConnections();}catch(e){setError(errorText(e));}}
  async function logout(){if(!connection)return;try{await teamApi.logout(connection.connection_id);await loadConnections();}catch(e){setError(errorText(e));}}
  async function openKnowledge(id:string){if(!connection)return;const ticket=gate.current.begin();if(!ticket)return;try{const value=await teamApi.knowledge(connection.connection_id,id);if(gate.current.accepts(ticket))setDetail(value);}catch(e){if(gate.current.accepts(ticket))setError(errorText(e));}}
  const visible=rows.filter(row=>row.share_source===filter);
  return <div className="flex h-full min-h-0 flex-col bg-slate-50"><header className="border-b bg-white px-6 py-5">
    <div className="flex flex-wrap items-center justify-between gap-3"><div><h1 className="flex items-center gap-2 text-2xl font-semibold"><Building2 size={22}/>团队空间</h1><p className="mt-1 text-sm text-slate-500">团队连接为可选功能；登录不会迁移或自动上传你的个人历史。</p></div><button className={button} onClick={()=>void loadConnections()}><RefreshCw size={15} className="mr-1 inline"/>刷新连接</button></div>
    <div className="mt-4 flex flex-wrap gap-2"><input aria-label="团队服务地址" className="min-w-72 flex-1 rounded-lg border px-3 py-2 text-sm" placeholder="https://aiks.example.com" value={origin} onChange={e=>setOrigin(e.target.value)}/><button className={primary} onClick={()=>void add()}>添加公司连接</button></div>
    {error&&<p role="alert" className="mt-3 rounded-lg bg-red-50 p-3 text-sm text-red-700">{error}</p>}{message&&<p className="mt-3 rounded-lg bg-blue-50 p-3 text-sm text-blue-700">{message}</p>}
  </header><div className="flex min-h-0 flex-1"><aside className="w-72 shrink-0 overflow-auto border-r bg-white p-3">
    {connections.length===0?<p className="p-3 text-sm text-slate-500">尚未添加团队连接。</p>:connections.map(item=><button key={item.connection_id} onClick={()=>setSelected(item.connection_id)} className={`mb-2 block w-full rounded-lg border p-3 text-left ${selected===item.connection_id?"border-blue-200 bg-blue-50":"hover:bg-slate-50"}`}><p className="truncate text-sm font-medium">{item.origin}</p><p className="mt-1 text-xs text-slate-500">{item.state==="signed_in"?(item.display_name??"已登录"):item.state==="credentials_missing"?"凭据已失效":"未登录"}</p></button>)}
    {connection&&<div className="mt-3 border-t pt-3">{connection.state==="signed_in"?<button className={button} onClick={()=>void logout()}><LogOut size={14} className="mr-1 inline"/>退出团队登录</button>:<div className="flex flex-col gap-2"><button className={primary} onClick={()=>void begin()}><LogIn size={14} className="mr-1 inline"/>登录钉钉</button><button className={button} onClick={()=>void finish()}>完成登录</button></div>}</div>}
  </aside><main className="flex min-w-0 flex-1 flex-col overflow-hidden">
    {!connection?<div className="p-8 text-sm text-slate-500">选择或添加一个团队连接。</div>:connection.state!=="signed_in"?<div className="p-8"><h2 className="font-semibold">需要登录</h2><p className="mt-2 text-sm text-slate-500">团队知识不会回退为个人身份；请完成钉钉登录后再访问。</p></div>:<>
      <div className="flex shrink-0 gap-2 border-b bg-white p-4">{(["mine","shared_to_me","department"] as ShareSource[]).map(key=><button key={key} className={filter===key?primary:button} onClick={()=>setFilter(key)}>{labels[key]}</button>)}</div>
      <div className="flex min-h-0 flex-1"><aside className="w-80 shrink-0 overflow-auto border-r bg-white">{loading&&<p className="p-4 text-sm text-slate-500">正在读取团队知识…</p>}{!loading&&visible.length===0&&<p className="p-4 text-sm text-slate-500">当前分类暂无知识。</p>}{visible.map(row=><button key={row.id} onClick={()=>void openKnowledge(row.id)} className="block w-full border-b px-4 py-4 text-left hover:bg-slate-50"><p className="text-sm font-medium">{row.title}</p><p className="mt-1 text-xs text-slate-400">{labels[row.share_source]}{row.content_revision!=null?` · 内容版本 ${row.content_revision}`:""}</p></button>)}</aside>
        <section className="min-w-0 flex-1 overflow-auto">{detail?<><div className="flex items-center justify-between border-b bg-white px-6 py-3"><div className="flex items-center gap-2 text-xs text-slate-500"><Users size={14}/>{labels[detail.share_source]}</div>{detail.can_manage&&<button className={button} onClick={()=>setSharing(true)}>管理分享</button>}</div><KnowledgeReading detail={detail}/>{sharing&&detail.can_manage&&<ShareDialog connectionId={connection.connection_id} knowledgeId={detail.id} onClose={()=>setSharing(false)}/>}</>:<div className="flex h-full items-center justify-center p-8 text-sm text-slate-400">选择一条团队知识开始阅读</div>}</section>
      </div>
    </>}
  </main></div></div>;
}
