import {type ReactNode,useEffect,useRef,useState} from "react";
import {BookOpen,MessageSquare,Search} from "lucide-react";
import {serviceApi,type SearchHit} from "../../api/service";
import PublishToTeam from "./PublishToTeam";
import {button,errorText} from "./shared";

export function TextBody({text}:{text:string}){
  // Text-only rendering: never execute imported HTML or load remote attachments.
  const chunks=text.split(/(```[\s\S]*?```)/g);
  return <div className="space-y-3 break-words text-sm leading-7">{chunks.map((chunk,i)=>chunk.startsWith("```")?
    <pre key={i} className="overflow-auto rounded-lg bg-slate-100 p-4 font-mono text-xs leading-6">{chunk.slice(3,-3).replace(/^[^\n]*\n/,"")}</pre>:
    <div key={i} className="whitespace-pre-wrap">{chunk}</div>)}</div>;
}
export function Conversation({messages}:{messages:unknown[]}){
  return <div className="space-y-5">{messages.map((raw,i)=>{
    const m=raw&&typeof raw==="object"?raw as Record<string,unknown>:{};
    const role=String(m.role??"unknown");
    const text=typeof m.text==="string"?m.text:Array.isArray(m.blocks)?m.blocks.map(raw=>{
      const b=raw&&typeof raw==="object"?raw as Record<string,unknown>:{};
      if(typeof b.text==="string")return b.text;
      if(typeof b.content==="string")return b.content;
      if(b.type==="tool_call")return `工具调用：${String(b.name??"工具")}`;
      if(b.type==="image")return "[图片附件，未自动加载]";
      if(b.type==="file_reference")return `[文件引用：${String(b.name??"附件")}]`;
      return "[其他内容块]";
    }).join("\n"):"";
    return <section key={i} className={`rounded-xl border p-4 ${role==="user"?"border-blue-100 bg-blue-50/50":"bg-white"}`}>
      <div className="mb-2 text-xs font-semibold text-slate-500">{{user:"用户",assistant:"AI 助手",tool:"工具",system:"系统",unknown:"消息"}[role]??"消息"}</div>
      <TextBody text={text}/>
    </section>;
  })}</div>;
}
export function KnowledgeReading({detail,actions}:{detail:Record<string,unknown>;actions?:ReactNode}){
  const session=detail.session&&typeof detail.session==="object"?detail.session as Record<string,unknown>:null;
  return <article className="mx-auto w-full max-w-4xl p-6 lg:p-9">
    <h2 className="mb-2 text-xl font-semibold">{String(detail.title??session?.title??"会话记录")}</h2>
    <div className="mb-6 flex flex-wrap gap-2 text-xs text-slate-500">{detail.revision!=null&&<span>来源版本 {String(detail.revision)}</span>}{detail.content_state==="draft"&&<span>提炼草稿</span>}{detail.stale===true&&<span className="text-amber-700">此条知识来自旧版本，会保留原内容</span>}</div>
    {actions&&<div className="mb-5 flex flex-wrap gap-2">{actions}</div>}
    {session&&Array.isArray(session.messages)?<Conversation messages={session.messages}/>:typeof detail.content==="string"?<TextBody text={detail.content}/>:<p className="text-slate-500">正文暂不可用，请刷新后重试。</p>}
  </article>;
}
export default function Reading({corpus,ready,onSettings}:{corpus:"knowledge"|"session";ready:boolean;onSettings:()=>void}){
  const [query,setQuery]=useState("");const [hits,setHits]=useState<SearchHit[]|null>(null);
  const [rows,setRows]=useState<Record<string,unknown>[]>([]);const [offset,setOffset]=useState(0);
  const [loading,setLoading]=useState(false);const [loaded,setLoaded]=useState(false);const [error,setError]=useState<string|null>(null);
  const [detail,setDetail]=useState<Record<string,unknown>|null>(null);const [selected,setSelected]=useState<string|null>(null);
  const [detailLoading,setDetailLoading]=useState(false);const [degraded,setDegraded]=useState(false);
  const sequence=useRef(0);const openSequence=useRef(0);
  useEffect(()=>{return()=>{sequence.current++;openSequence.current++;};},[]);
  async function list(start=0){
    const request=++sequence.current;setLoading(true);setError(null);setHits(null);setOffset(start);
    try{const data=corpus==="knowledge"?await serviceApi.knowledgeList(start):await serviceApi.sessions(start);
      if(request===sequence.current){setRows(data);setLoaded(true);setDegraded(false);}}
    catch(e){if(request===sequence.current){setLoaded(false);setError(errorText(e));}}
    finally{if(request===sequence.current)setLoading(false);}
  }
  useEffect(()=>{if(ready)void list();else{sequence.current++;setLoading(false);setLoaded(false);}},[ready,corpus]);
  async function search(){
    if(!query.trim()){void list();return;}
    const request=++sequence.current;setLoading(true);setError(null);setHits(null);
    try{const result=await serviceApi.search(query,corpus);if(request===sequence.current){setHits(result.hits);setLoaded(true);setDegraded(result.degraded);}}
    catch(e){if(request===sequence.current){setLoaded(false);setError(errorText(e));}}
    finally{if(request===sequence.current)setLoading(false);}
  }
  async function open(id:string){
    const request=++openSequence.current;setSelected(id);setDetail(null);setDetailLoading(true);setError(null);
    try{const result=await serviceApi.detail(corpus,id);if(request===openSequence.current)setDetail(result);}
    catch(e){if(request===openSequence.current)setError(errorText(e));}
    finally{if(request===openSequence.current)setDetailLoading(false);}
  }
  const items=hits??rows.map(row=>({entity_id:String(row.id??row.session_id),title:String(row.title||"未命名会话"),snippet:String(row.summary??""),revision:row.revision,corpus}));
  return <div className="flex h-full min-h-0 flex-col">
    <header className="border-b bg-white px-6 py-5"><h1 className="text-2xl font-semibold">{corpus==="knowledge"?"知识库":"会话记录"}</h1>
      <p className="mt-1 text-sm text-slate-500">{corpus==="knowledge"?"浏览与阅读沉淀的知识，不需要先完成采集或模型设置。":"查看已接收的 AI 工作对话；原始会话与提炼知识分别管理。"}</p>
      <form className="mt-4 flex gap-2" onSubmit={e=>{e.preventDefault();void search();}}><div className="relative flex-1"><Search size={17} className="absolute left-3 top-3 text-slate-400"/><input aria-label="搜索知识与会话" className="w-full rounded-lg border py-2 pl-10 pr-3 text-sm" placeholder={corpus==="knowledge"?"搜索知识标题或正文":"搜索会话"} value={query} onChange={e=>setQuery(e.target.value)}/></div><button className={button} disabled={!ready||loading}>{loading?"加载中…":"搜索"}</button><button type="button" className={button} disabled={!ready||loading} onClick={()=>{setQuery("");void list();}}>刷新</button></form>
    </header>
    {error&&<div role="alert" className="m-4 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700">{error}</div>}
    {!ready?<div className="p-8 text-sm text-slate-500">正在等待本机知识服务。你可以先查看设置；连接恢复后会自动读取内容。</div>:
    <div className="flex min-h-0 flex-1 flex-col overflow-auto lg:flex-row">
      <aside className="shrink-0 border-b bg-white lg:w-80 lg:overflow-auto lg:border-b-0 lg:border-r">
        {degraded&&<p className="p-4 text-xs text-amber-700">部分检索能力暂不可用，正在展示可用结果。</p>}
        {loading&&<p className="p-5 text-sm text-slate-500">正在读取…</p>}
        {!loading&&loaded&&items.length===0&&<div className="p-6 text-sm text-slate-500"><p>{hits?"未找到相关结果":corpus==="knowledge"?"当前个人空间还没有提炼知识。":"当前个人空间还没有接收会话。"}</p><p className="my-3 text-xs leading-6">未自动导入旧个人库。你可以稍后设置采集；未启用模型不会阻止阅读已有内容。</p><button className={button} onClick={onSettings}>前往采集设置</button></div>}
        {!loading&&loaded&&items.map(row=><button key={row.entity_id} aria-pressed={selected===row.entity_id} onClick={()=>void open(row.entity_id)} className={`block w-full border-b px-5 py-4 text-left hover:bg-slate-50 ${selected===row.entity_id?"bg-blue-50":""}`}><span className="block text-sm font-medium">{row.title}</span>{row.snippet&&<span className="mt-2 line-clamp-3 block text-xs leading-5 text-slate-500">{row.snippet}</span>}<span className="mt-2 block text-xs text-slate-400">{row.revision!=null?`来源版本 ${row.revision}`:"来源版本待确认"}</span></button>)}
        {!hits&&loaded&&<div className="flex items-center justify-between gap-2 p-4 text-xs"><button className={button} disabled={offset===0||loading} onClick={()=>void list(Math.max(0,offset-30))}>上一页</button><span>第 {Math.floor(offset/30)+1} 页</span><button className={button} disabled={rows.length<30||loading} onClick={()=>void list(offset+30)}>下一页</button></div>}
      </aside>
      <div className="min-w-0 flex-1 overflow-auto">{detailLoading?<p className="p-8 text-sm text-slate-500">正在读取正文…</p>:detail?<KnowledgeReading detail={detail} actions={corpus==="knowledge"?<PublishToTeam detail={detail}/>:undefined}/>:<div className="flex h-full min-h-64 flex-col items-center justify-center gap-3 p-8 text-slate-400">{corpus==="knowledge"?<BookOpen size={34}/>:<MessageSquare size={34}/>}<p className="text-sm">选择左侧条目开始阅读</p></div>}</div>
    </div>}
  </div>;
}
