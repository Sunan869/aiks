// Personal service workspace: 模型配置 and sync are optional settings, not a gate.
import {useEffect,useState,useSyncExternalStore} from "react";
import {BookOpen,MessageSquare,SlidersHorizontal,Activity,Settings} from "lucide-react";
import {serviceApi,type ServiceStatus} from "../api/service";
import {DataStorageSettingsSection} from "../components/DataStorage";
import {WorkspaceController,type WorkspacePage} from "./service/onboarding";
import Reading from "./service/Reading";
import SourceSettings from "./service/SourceSettings";
import Tasks from "./service/Tasks";
import {button} from "./service/shared";

export default function ServiceStatusPage(){
  const [controller]=useState(()=>new WorkspaceController(serviceApi));
  const state=useSyncExternalStore(controller.subscribe,controller.getSnapshot,controller.getSnapshot);
  const [status,setStatus]=useState<ServiceStatus|null>(null);const [refresh,setRefresh]=useState(0);
  useEffect(()=>{void controller.initialize();},[controller,refresh]);
  useEffect(()=>{let active=true,inflight=false;const load=async()=>{if(inflight)return;inflight=true;
    try{const status=await serviceApi.status();if(active)setStatus(status);}catch{if(active)setStatus(s=>({...s,mode:"service_local",phase:"unavailable"}));}finally{inflight=false;}};
    void load();const timer=setInterval(load,3000);return()=>{active=false;clearInterval(timer);};
  },[refresh]);
  // Retry just preferences while native state is starting; never scan on mount.
  useEffect(()=>{if(state.preferences)return;const timer=setInterval(()=>{void controller.initialize();},2000);return()=>clearInterval(timer);},[controller,state.preferences]);
  const nav:{key:WorkspacePage;label:string;icon:typeof BookOpen}[]=[{key:"knowledge",label:"知识库",icon:BookOpen},{key:"sessions",label:"会话记录",icon:MessageSquare},{key:"sync",label:"采集与同步",icon:SlidersHorizontal},{key:"tasks",label:"任务记录",icon:Activity},{key:"models",label:"模型与服务设置",icon:Settings}];
  const ready=status?.phase==="ready";
  const sources=state.preferences?.selected_sources??[];
  return <div className="flex h-screen min-h-0 flex-col bg-slate-50 text-slate-800">
    <header className="flex h-14 shrink-0 items-center justify-between border-b bg-white px-5"><div className="flex items-center gap-3"><strong className="text-lg text-blue-600">AIKS</strong><span className="text-xs text-slate-500">本地知识服务 · 个人独立部署版</span></div><span className={`rounded-full px-3 py-1 text-xs ${ready?"bg-emerald-50 text-emerald-700":"bg-amber-50 text-amber-700"}`}>{ready?"服务已连接":status?.phase==="starting"||!status?"服务正在启动":"服务暂不可用"}</span></header>
    <div className="flex min-h-0 flex-1"><nav aria-label="主导航" className="flex w-48 shrink-0 flex-col border-r bg-white p-3"><div className="mb-4 px-3 py-3"><p className="text-sm font-semibold">我的个人空间</p><p className="mt-1 text-xs text-slate-400">本机存储 · 不自动迁移旧库</p></div>{nav.map(item=><button key={item.key} aria-current={state.page===item.key?"page":undefined} onClick={()=>controller.navigate(item.key)} className={`mb-1 flex items-center gap-3 rounded-lg px-3 py-3 text-left text-sm ${state.page===item.key?"bg-blue-50 font-medium text-blue-700":"text-slate-600 hover:bg-slate-50"}`}><item.icon size={17}/>{item.label}</button>)}<div className="mt-auto border-t px-3 pt-4 text-xs leading-6 text-slate-400"><p>AI 提炼：{status?.capabilities?.ai_assist?"已启用":"未启用"}</p><p>语义检索：{status?.capabilities?.semantic_search?"已启用":"未启用"}</p><p className="mt-2">未配置模型也可以阅读和关键词检索。</p></div></nav>
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {!ready&&status?.phase!=="starting"&&status&&<div role="alert" className="flex shrink-0 items-center justify-between gap-3 border-b border-amber-200 bg-amber-50 px-5 py-3 text-sm text-amber-800"><span>知识服务暂不可用，内容将在恢复连接后加载。仍可查看设置，不会回退到旧引擎或显示虚假空库。</span><button className={button} onClick={()=>setRefresh(v=>v+1)}>刷新状态</button></div>}
        {state.error&&<div role="alert" className="shrink-0 border-b border-amber-200 bg-amber-50 px-5 py-2 text-sm text-amber-800">{state.error}</div>}
        {state.page==="knowledge"||state.page==="sessions"?<Reading key={state.page} corpus={state.page==="knowledge"?"knowledge":"session"} ready={ready} onSettings={()=>controller.navigate("sync")}/>:
        <div className="flex-1 overflow-auto">
          {state.page==="setup"&&<section className="mx-auto mt-6 flex max-w-6xl flex-wrap items-center justify-between gap-4 rounded-xl border border-blue-100 bg-blue-50 p-6"><div><h1 className="text-xl font-semibold">欢迎使用 AIKS</h1><p className="mt-2 text-sm text-slate-600">可以先选择采集来源，也可以跳过。以后从“采集与同步”回来设置。</p><p className="mt-1 text-xs text-slate-500">跳过不会启动采集、上传历史会话或调用模型。</p></div><button className={button} onClick={()=>void controller.finish(true)}>跳过，进入知识库</button></section>}
          {(state.page==="sync"||state.page==="setup")&&<SourceSettings status={status} sources={sources} onSave={s=>controller.saveSources(s)} onComplete={state.page==="setup"?()=>controller.finish(false):undefined}/>}
          {state.page==="tasks"&&<Tasks ready={ready}/>}
          {state.page==="models"&&<><div className="mx-auto max-w-6xl space-y-5 p-6"><h1 className="text-2xl font-semibold">模型与服务设置</h1><section className="rounded-xl border bg-white p-5"><h2 className="font-semibold">模型配置（可选）</h2><p className="mt-3 text-sm leading-7 text-slate-600">AI 提炼和语义检索分别按配置启用，不影响阅读已有知识。当前配置文件位于数据目录的 <code>service-local/config/models.toml</code>，修改后正常退出并重启程序。</p><p className="mt-3 text-sm leading-7 text-slate-600">AI 请求会发送到你配置的模型服务。完全离线使用需提前准备本机模型和运行资源；本机存储不等于远程模型调用不出网。</p></section><section className="rounded-xl border bg-white p-5"><h2 className="font-semibold">个人独立部署</h2><p className="mt-3 text-sm leading-7 text-slate-600">当前仅连接本机 AIKS Service。思源通过内部内容接口访问，不提供全库代理。本次不包含部门、公司或多人共享模式。</p><button className={`${button} mt-4`} onClick={()=>controller.navigate("setup")}>重新查看使用引导</button></section></div><DataStorageSettingsSection/></>}
        </div>}
      </main>
    </div>
  </div>;
}
